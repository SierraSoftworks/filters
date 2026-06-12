//! Compiled pattern representations used by the `like` and `matches` operators.
//!
//! Patterns are compiled once at parse time (within [`Filter::new`](crate::Filter::new))
//! so that evaluating a filter against an object performs no pattern-related
//! heap allocation.

use std::fmt::{Debug, Display};

use crate::case_sensitivity::{casefold, casefold_char};

/// A single element of a compiled glob pattern.
#[derive(PartialEq)]
enum GlobToken {
    /// Matches exactly the given case-folded character. Pattern literals are
    /// folded at compile time, so multi-character expansions (e.g. `ß` → `ss`)
    /// become a sequence of these tokens.
    Literal(char),
    /// `?` — matches exactly one character of the case-folded input.
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
/// Matching is case-insensitive by default ([`Glob::compile`]), using the
/// same character folding rules as the rest of the filter language (see
/// [`crate::case_sensitivity`]): both the pattern (at compile time) and the
/// input (at match time) are folded character-by-character via Unicode
/// lowercasing, with all Greek sigma forms (`Σ`, `σ`, `ς`) treated as
/// equivalent, and the pattern is matched against the folded input stream.
/// Characters whose lowercase form expands to several characters participate
/// fully (e.g. `ß` folds to `ss`, so `"groß"` matches `gro*ss` and
/// `"GRÜSSE"` matches `grüße*`).
///
/// Because matching operates on the *folded* stream, `?` consumes exactly
/// one folded character: an input `ß` counts as the two characters `ss`,
/// and is matched by `??` rather than `?`.
///
/// Case-*sensitive* globs ([`Glob::compile_cs`], powering the `like_cs`
/// operator) skip folding entirely: literals match exact characters, and `?`
/// consumes exactly one character of the raw input.
#[derive(PartialEq)]
pub struct Glob {
    pattern: String,
    tokens: Vec<GlobToken>,
    case_sensitive: bool,
}

impl Glob {
    /// Compiles the provided pattern for case-insensitive matching.
    /// Compilation cannot fail: every string is a valid glob pattern.
    pub fn compile(pattern: &str) -> Self {
        Self::compile_with(pattern, false)
    }

    /// Compiles the provided pattern for case-sensitive matching, in which
    /// literal characters match exactly and no case folding takes place.
    pub fn compile_cs(pattern: &str) -> Self {
        Self::compile_with(pattern, true)
    }

    fn compile_with(pattern: &str, case_sensitive: bool) -> Self {
        let mut tokens = Vec::new();
        let mut chars = pattern.chars();
        let literal = |tokens: &mut Vec<GlobToken>, c: char| {
            if case_sensitive {
                tokens.push(GlobToken::Literal(c));
            } else {
                // Literal characters are case-folded at compile time so that
                // matching can compare them directly against the folded input
                // stream (multi-character expansions become several tokens).
                tokens.extend(casefold_char(c).map(GlobToken::Literal));
            }
        };

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
                '\\' => literal(&mut tokens, chars.next().unwrap_or('\\')),
                c => literal(&mut tokens, c),
            }
        }

        Self {
            pattern: pattern.to_string(),
            tokens,
            case_sensitive,
        }
    }

    /// Gets the original (uncompiled) pattern this glob was built from.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Whether this glob matches case-sensitively (see [`Glob::compile_cs`]).
    pub fn is_case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    /// Tests whether the entire input matches this pattern (after case
    /// folding, unless this glob was compiled with [`Glob::compile_cs`]).
    pub fn is_match(&self, input: &str) -> bool {
        if self.case_sensitive {
            self.matches_stream(input.chars().peekable())
        } else {
            self.matches_stream(casefold(input).peekable())
        }
    }

    /// Tests whether the entire character stream matches this pattern.
    ///
    /// This uses an iterative two-pointer algorithm with single-star
    /// backtracking over the character stream, performing no heap allocation
    /// regardless of the input or pattern: the stream positions used for
    /// backtracking are cheap clones of the iterator.
    fn matches_stream<I: Iterator<Item = char> + Clone>(
        &self,
        mut chars: std::iter::Peekable<I>,
    ) -> bool {
        let tokens = &self.tokens;
        let mut t = 0; // Index of the pattern token being matched.

        // The token index following the most recent `*`, along with the
        // stream position at which that `*` should resume consuming input if
        // the remainder of the pattern fails to match.
        let mut star = None;

        loop {
            let progressed = match tokens.get(t) {
                Some(GlobToken::AnySequence) => {
                    t += 1;
                    star = Some((t, chars.clone()));
                    true
                }
                Some(GlobToken::AnyChar) => {
                    t += 1;
                    chars.next().is_some()
                }
                Some(GlobToken::Literal(p)) if chars.peek() == Some(p) => {
                    t += 1;
                    chars.next();
                    true
                }
                Some(GlobToken::Literal(..)) => false,
                // Pattern exhausted: this is a match iff the input is too.
                None if chars.peek().is_none() => return true,
                None => false,
            };

            if !progressed {
                // Mismatch: backtrack to the most recent `*` and let it
                // swallow one more character of the folded input.
                match star.take() {
                    Some((star_t, mut star_chars)) => {
                        if star_chars.next().is_none() {
                            return false;
                        }

                        t = star_t;
                        chars = star_chars.clone();
                        star = Some((star_t, star_chars));
                    }
                    None => return false,
                }
            }
        }
    }
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
    #[case("über*", "ÜBERMUT", true)]
    // Multi-character folds participate fully: ß folds to ss on either side.
    #[case("grüße*", "GRÜSSE", true)]
    #[case("GRÜSSE", "grüße", true)]
    #[case("*ss", "groß", true)]
    #[case("stra*e", "STRASSE", true)]
    // `?` consumes one character of the *folded* input, so ß counts as two.
    #[case("gro?", "groß", false)]
    #[case("gro??", "groß", true)]
    // Greek sigma forms are equivalent, consistent with contains/startswith/endswith.
    #[case("λογος", "ΛΟΓΟΣ", true)]
    #[case("ΛΟΓΟΣ", "λογος", true)]
    #[case("*ς", "ΛΟΓΟΣ", true)]
    #[case("*σ", "λογος", true)]
    #[case("σ", "ς", true)]
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

    #[rstest]
    // Wildcards behave exactly as they do in case-insensitive globs.
    #[case("feat/*", "feat/login", true)]
    #[case("feat/*", "fix/login", false)]
    #[case("v?.?", "v1.2", true)]
    // ...but literal characters must match exactly.
    #[case("Main", "Main", true)]
    #[case("Main", "main", false)]
    #[case("FEAT/*", "feat/login", false)]
    #[case("*Fix", "hotFix", true)]
    #[case("*Fix", "hotfix", false)]
    // No case folding takes place: ß stays a single character.
    #[case("gro?", "groß", true)]
    #[case("groß", "groß", true)]
    #[case("gross", "groß", false)]
    #[case("stra*e", "STRASSE", false)]
    // Escapes still make wildcards literal.
    #[case("a\\*b", "a*b", true)]
    #[case("a\\*b", "axb", false)]
    fn case_sensitive_glob_matching(
        #[case] pattern: &str,
        #[case] input: &str,
        #[case] expected: bool,
    ) {
        let glob = Glob::compile_cs(pattern);
        assert_eq!(
            glob.is_match(input),
            expected,
            "expected '{pattern}' matching '{input}' case-sensitively to be {expected}"
        );
    }

    #[test]
    fn glob_exposes_its_pattern() {
        let glob = Glob::compile("feat/*");
        assert_eq!(glob.pattern(), "feat/*");
        assert!(!glob.is_case_sensitive());

        let glob = Glob::compile_cs("feat/*");
        assert_eq!(glob.pattern(), "feat/*");
        assert!(glob.is_case_sensitive());
    }

    #[test]
    fn glob_display_quotes_and_escapes() {
        let glob = Glob::compile("a\\*\"b");
        assert_eq!(glob.to_string(), "\"a\\\\*\\\"b\"");
        assert_eq!(format!("{glob:?}"), "\"a\\\\*\\\"b\"");
    }

    #[test]
    fn glob_equality_is_based_on_the_pattern_and_sensitivity() {
        assert_eq!(Glob::compile("a*"), Glob::compile("a*"));
        assert_ne!(Glob::compile("a*"), Glob::compile("b*"));
        assert_ne!(Glob::compile("a*"), Glob::compile_cs("a*"));
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
