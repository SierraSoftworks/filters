//! Compiled pattern representations used by the `like` and `matches` operators.
//!
//! Patterns are compiled once at parse time (within [`Filter::new`](crate::Filter::new))
//! so that evaluating a filter against an object performs no pattern-related
//! heap allocation.

use std::fmt::{Debug, Display};

/// A single element of a compiled glob pattern.
#[derive(PartialEq)]
enum GlobToken {
    /// Matches exactly the given character (case-insensitively).
    Literal(char),
    /// `?` — matches exactly one character (one `char`, not one byte).
    AnyChar,
    /// `*` — matches any sequence of characters, including the empty sequence.
    AnySequence,
}

/// A glob pattern, compiled at parse time for allocation-free matching.
///
/// The supported syntax is intentionally small:
///
/// - `*` matches any sequence of characters (including none).
/// - `?` matches exactly one character.
/// - `\*`, `\?`, and `\\` match a literal `*`, `?`, and `\` respectively. A
///   backslash followed by any other character matches that character
///   literally, and a trailing backslash matches a literal `\`.
/// - Every other character matches itself, ignoring case.
///
/// Character classes (`[a-z]`) and alternation (`{a,b}`) are *not* supported.
///
/// Matching is case-insensitive: two characters are considered equal when
/// their Unicode lowercase expansions (via [`char::to_lowercase`]) are equal.
/// This is a per-character ("simple") case fold, so multi-character lowercase
/// expansions (e.g. `İ` → `i̇`) only compare equal to themselves, and `?`
/// always consumes exactly one character of the input.
#[derive(PartialEq)]
pub struct Glob {
    pattern: String,
    tokens: Vec<GlobToken>,
}

impl Glob {
    /// Compiles the provided pattern. Compilation cannot fail: every string is
    /// a valid glob pattern.
    pub fn compile(pattern: &str) -> Self {
        let mut tokens = Vec::new();
        let mut chars = pattern.chars();
        while let Some(c) = chars.next() {
            match c {
                // Consecutive `*`s are equivalent to a single `*`; collapsing
                // them keeps the backtracking matcher linear in practice.
                '*' => {
                    if tokens.last() != Some(&GlobToken::AnySequence) {
                        tokens.push(GlobToken::AnySequence);
                    }
                }
                '?' => tokens.push(GlobToken::AnyChar),
                '\\' => tokens.push(GlobToken::Literal(chars.next().unwrap_or('\\'))),
                c => tokens.push(GlobToken::Literal(c)),
            }
        }

        Self {
            pattern: pattern.to_string(),
            tokens,
        }
    }

    /// Gets the original (uncompiled) pattern this glob was built from.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Tests whether the entire input matches this pattern.
    ///
    /// This uses an iterative two-pointer algorithm with single-star
    /// backtracking, performing no heap allocation regardless of the input or
    /// pattern.
    pub fn is_match(&self, input: &str) -> bool {
        let tokens = &self.tokens;
        let mut t = 0; // Index of the pattern token being matched.
        let mut s = 0; // Byte offset of the input character being matched.

        // The token index following the most recent `*`, along with the byte
        // offset at which that `*` should resume consuming input if the
        // remainder of the pattern fails to match.
        let mut star: Option<(usize, usize)> = None;

        while s < input.len() {
            // SAFETY OF UNWRAP: `s` always sits on a character boundary, and
            // `s < input.len()`, so there is always a next character.
            let c = input[s..].chars().next().unwrap();
            match tokens.get(t) {
                Some(GlobToken::AnySequence) => {
                    star = Some((t + 1, s));
                    t += 1;
                }
                Some(GlobToken::AnyChar) => {
                    t += 1;
                    s += c.len_utf8();
                }
                Some(GlobToken::Literal(p)) if chars_eq_ignore_case(*p, c) => {
                    t += 1;
                    s += c.len_utf8();
                }
                _ => {
                    // Mismatch (or pattern exhausted): backtrack to the most
                    // recent `*` and let it swallow one more input character.
                    if let Some((star_t, star_s)) = star {
                        let swallowed = input[star_s..].chars().next().unwrap();
                        let resume = star_s + swallowed.len_utf8();
                        star = Some((star_t, resume));
                        t = star_t;
                        s = resume;
                    } else {
                        return false;
                    }
                }
            }
        }

        // Any trailing `*`s can match the empty remainder of the input.
        while tokens.get(t) == Some(&GlobToken::AnySequence) {
            t += 1;
        }

        t == tokens.len()
    }
}

/// Compares two characters case-insensitively using their full Unicode
/// lowercase expansions, without allocating.
fn chars_eq_ignore_case(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

impl Display for Glob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "\"{}\"",
            self.pattern().replace('\\', "\\\\").replace('"', "\\\"")
        )
    }
}

impl Debug for Glob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

/// A regular expression, compiled at parse time, which compares equal to other
/// instances built from the same pattern (the underlying [`regex::Regex`] does
/// not implement [`PartialEq`]).
#[cfg(feature = "regex")]
pub struct CompiledRegex(regex::Regex);

#[cfg(feature = "regex")]
impl CompiledRegex {
    /// Compiles the provided regular expression pattern.
    pub fn compile(pattern: &str) -> Result<Self, regex::Error> {
        regex::Regex::new(pattern).map(Self)
    }

    /// Gets the original pattern this regular expression was built from.
    pub fn pattern(&self) -> &str {
        self.0.as_str()
    }

    /// Tests whether the input matches this regular expression.
    ///
    /// Note that, unlike [`Glob::is_match`], regex matching is unanchored
    /// (use `^`/`$` to anchor) and case-sensitive (use `(?i)` to ignore case).
    /// Matching is *amortized* allocation-free: the underlying regex engine
    /// lazily allocates scratch space on first use (per thread) and reuses it
    /// for subsequent calls.
    pub fn is_match(&self, input: &str) -> bool {
        self.0.is_match(input)
    }
}

#[cfg(feature = "regex")]
impl PartialEq for CompiledRegex {
    fn eq(&self, other: &Self) -> bool {
        self.pattern() == other.pattern()
    }
}

#[cfg(feature = "regex")]
impl Display for CompiledRegex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "\"{}\"",
            self.pattern().replace('\\', "\\\\").replace('"', "\\\"")
        )
    }
}

#[cfg(feature = "regex")]
impl Debug for CompiledRegex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    // Prefix patterns (the headline use-case).
    #[case("feat/*", "feat/login", true)]
    #[case("feat/*", "feat/", true)]
    #[case("feat/*", "fix/login", false)]
    #[case("feat/*", "feat", false)]
    // Suffix and infix patterns.
    #[case("*fix", "hotfix", true)]
    #[case("*fix", "fixes", false)]
    #[case("*mid*", "amidst", true)]
    #[case("*mid*", "mid", true)]
    #[case("*mid*", "madam", false)]
    // Single-character wildcards.
    #[case("?", "a", true)]
    #[case("?", "", false)]
    #[case("?", "ab", false)]
    #[case("v?.?", "v1.2", true)]
    #[case("v?.?", "v1.23", false)]
    // `*` alone, empty patterns, and empty inputs.
    #[case("*", "", true)]
    #[case("*", "anything at all", true)]
    #[case("", "", true)]
    #[case("", "a", false)]
    #[case("a*", "", false)]
    // Literal-only patterns behave like case-insensitive equality.
    #[case("main", "main", true)]
    #[case("main", "Main", true)]
    #[case("main", "maine", false)]
    #[case("main", "remain", false)]
    // Case-insensitivity applies to wildcard patterns too.
    #[case("FEAT/*", "feat/login", true)]
    #[case("feat/*", "FEAT/LOGIN", true)]
    // Multiple stars and backtracking traps.
    #[case("*a*ab", "aaab", true)]
    #[case("*a*ab", "aaba", false)]
    #[case("*ab*ba*", "abba", true)]
    #[case("*ab*ba*", "abxba", true)]
    #[case("*ab*ba*", "aba", false)]
    #[case("a**b", "ab", true)]
    #[case("a**b", "axxb", true)]
    #[case("*a*a*a*", "aaa", true)]
    #[case("*a*a*a*a*", "aaa", false)]
    // Unicode inputs: `?` consumes one character, not one byte.
    #[case("?", "é", true)]
    #[case("??", "hé", true)]
    #[case("???", "hé", false)]
    #[case("h?llo", "héllo", true)]
    #[case("*ö*", "schön", true)]
    #[case("grüße*", "GRÜSSE", false)] // ß does not case-fold to "ss" per-char
    #[case("über*", "ÜBERMUT", true)]
    // Escapes make wildcards literal.
    #[case("a\\*b", "a*b", true)]
    #[case("a\\*b", "axb", false)]
    #[case("a\\?b", "a?b", true)]
    #[case("a\\?b", "axb", false)]
    #[case("a\\\\b", "a\\b", true)]
    #[case("a\\xb", "axb", true)]
    #[case("trailing\\", "trailing\\", true)]
    fn glob_matching(#[case] pattern: &str, #[case] input: &str, #[case] expected: bool) {
        let glob = Glob::compile(pattern);
        assert_eq!(
            glob.is_match(input),
            expected,
            "expected '{pattern}' matching '{input}' to be {expected}"
        );
    }

    #[test]
    fn glob_exposes_its_pattern() {
        let glob = Glob::compile("feat/*");
        assert_eq!(glob.pattern(), "feat/*");
    }

    #[test]
    fn glob_display_quotes_and_escapes() {
        let glob = Glob::compile("a\\*\"b");
        assert_eq!(glob.to_string(), "\"a\\\\*\\\"b\"");
        assert_eq!(format!("{glob:?}"), "\"a\\\\*\\\"b\"");
    }

    #[test]
    fn glob_equality_is_based_on_the_pattern() {
        assert_eq!(Glob::compile("a*"), Glob::compile("a*"));
        assert_ne!(Glob::compile("a*"), Glob::compile("b*"));
    }

    #[cfg(feature = "regex")]
    mod regex_tests {
        use super::*;

        #[test]
        fn regex_compilation_reports_errors() {
            assert!(CompiledRegex::compile("(unclosed").is_err());
            assert!(CompiledRegex::compile("^release/v\\d+$").is_ok());
        }

        #[rstest]
        #[case("^release/v\\d+(\\.\\d+){2}$", "release/v1.2.3", true)]
        #[case("^release/v\\d+(\\.\\d+){2}$", "release/v1.2", false)]
        #[case("ell", "hello", true)] // unanchored by default
        #[case("(?i)hello", "HELLO", true)]
        #[case("hello", "HELLO", false)] // case-sensitive by default
        fn regex_matching(#[case] pattern: &str, #[case] input: &str, #[case] expected: bool) {
            let regex = CompiledRegex::compile(pattern).expect("compile the pattern");
            assert_eq!(regex.is_match(input), expected);
        }

        #[test]
        fn regex_exposes_its_pattern() {
            let regex = CompiledRegex::compile("^a$").expect("compile the pattern");
            assert_eq!(regex.pattern(), "^a$");
        }

        #[test]
        fn regex_equality_is_based_on_the_pattern() {
            let a = CompiledRegex::compile("^a$").expect("compile the pattern");
            let b = CompiledRegex::compile("^a$").expect("compile the pattern");
            let c = CompiledRegex::compile("^c$").expect("compile the pattern");
            assert_eq!(a, b);
            assert_ne!(a, c);
        }

        #[test]
        fn regex_display_quotes_and_escapes() {
            let regex = CompiledRegex::compile("^v\\d+$").expect("compile the pattern");
            assert_eq!(regex.to_string(), "\"^v\\\\d+$\"");
            assert_eq!(format!("{regex:?}"), "\"^v\\\\d+$\"");
        }
    }
}
