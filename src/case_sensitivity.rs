//! Centralized case-insensitivity rules for the filter language.
//!
//! All of the filter language's case-insensitive string operations (`==`,
//! `contains`, `startswith`, `endswith`, and the `like` glob operator) are
//! routed through this module so that every operator folds case in exactly
//! the same way.
//!
//! Each character is folded through a lowercase → uppercase → lowercase
//! round-trip (using [`char::to_lowercase`]/[`char::to_uppercase`], which
//! never allocate). This closely approximates Unicode full case folding and
//! has two properties a plain lowercase pass lacks:
//!
//! - Characters whose uppercase form expands to several characters fold to
//!   that expansion's lowercase form: `ß` and `ẞ` both fold to `ss`, so
//!   `"STRASSE" == "straße"` and `"groß" like "*ss"` both hold.
//! - The Greek sigma forms all fold to a single character (`ς` → `Σ` → `σ`),
//!   so `Σ`, `σ`, and `ς` compare equal regardless of their position within
//!   a word — [`str::to_lowercase`]'s context-sensitive final-sigma rule
//!   would make results depend on position instead.

/// Iterates over the case-folded form of a single character without
/// allocating, by round-tripping it through lowercase → uppercase →
/// lowercase. Folded forms may span multiple characters (e.g. `İ` folds to
/// `i` followed by a combining dot above, and `ß` folds to `ss`).
pub(crate) fn casefold_char(c: char) -> impl DoubleEndedIterator<Item = char> + Clone {
    c.to_lowercase()
        .flat_map(char::to_uppercase)
        .flat_map(char::to_lowercase)
}

/// Iterates over the case-folded characters of a string without allocating.
pub(crate) fn casefold(s: &str) -> impl Iterator<Item = char> + Clone + '_ {
    s.chars().flat_map(casefold_char)
}

/// Iterates over the case-folded characters of a string in reverse order
/// without allocating. Equivalent to reversing [`casefold`].
pub(crate) fn casefold_rev(s: &str) -> impl Iterator<Item = char> + Clone + '_ {
    s.chars().rev().flat_map(|c| casefold_char(c).rev())
}

/// Determines whether two strings are equal under this module's case
/// folding rules, comparing their folded character streams without
/// allocating. This powers the `==` and `!=` operators for strings.
pub(crate) fn caseless_eq(a: &str, b: &str) -> bool {
    casefold(a).eq(casefold(b))
}

/// Determines whether `prefix` is a prefix of `haystack`, comparing the
/// two character streams element-wise.
fn is_char_prefix(
    mut haystack: impl Iterator<Item = char>,
    prefix: impl Iterator<Item = char>,
) -> bool {
    for c in prefix {
        if haystack.next() != Some(c) {
            return false;
        }
    }

    true
}

/// Determines whether the case-folded `needle` appears anywhere within the
/// case-folded `haystack`, without allocating.
pub(crate) fn caseless_contains(haystack: &str, needle: &str) -> bool {
    let mut start = casefold(haystack);
    loop {
        if is_char_prefix(start.clone(), casefold(needle)) {
            return true;
        }

        if start.next().is_none() {
            return false;
        }
    }
}

/// Determines whether the case-folded `haystack` starts with the
/// case-folded `needle`, without allocating.
pub(crate) fn caseless_starts_with(haystack: &str, needle: &str) -> bool {
    is_char_prefix(casefold(haystack), casefold(needle))
}

/// Determines whether the case-folded `haystack` ends with the case-folded
/// `needle`, without allocating.
pub(crate) fn caseless_ends_with(haystack: &str, needle: &str) -> bool {
    is_char_prefix(casefold_rev(haystack), casefold_rev(needle))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("hello", "hello", true)]
    #[case("Hello", "hELLO", true)]
    #[case("hello", "hellos", false)]
    #[case("", "", true)]
    #[case("Jürgen", "JÜRGEN", true)]
    // All Greek sigma forms are equivalent, regardless of position.
    #[case("ΛΟΓΟΣ", "λογος", true)]
    #[case("λογοσ", "λογος", true)]
    // Multi-character expansions participate in equality.
    #[case("straße", "STRASSE", true)]
    #[case("straße", "strasse", true)]
    #[case("straẞe", "strasse", true)] // capital sharp s folds like ß
    #[case("İstanbul", "i\u{307}stanbul", true)]
    #[case("İstanbul", "istanbul", false)] // the combining mark is significant
    fn test_caseless_eq(#[case] a: &str, #[case] b: &str, #[case] expected: bool) {
        assert_eq!(caseless_eq(a, b), expected);
        assert_eq!(caseless_eq(b, a), expected);
    }

    #[rstest]
    #[case("Hello World", "hello world")]
    #[case("ΛΟΓΟΣ", "λογοσ")] // final sigma folds to σ, not ς
    #[case("λογος", "λογοσ")]
    #[case("İstanbul", "i\u{307}stanbul")]
    #[case("straße", "strasse")] // ß folds through SS to ss
    #[case("STRASSE", "strasse")]
    fn test_casefold(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(casefold(input).collect::<String>(), expected);
        assert_eq!(
            casefold_rev(input).collect::<String>(),
            expected.chars().rev().collect::<String>()
        );
    }

    #[rstest]
    #[case("Hello World", "WORLD", true)]
    #[case("Hello World", "mars", false)]
    #[case("ΛΟΓΟΣ", "ς", true)]
    fn test_caseless_contains(
        #[case] haystack: &str,
        #[case] needle: &str,
        #[case] expected: bool,
    ) {
        assert_eq!(caseless_contains(haystack, needle), expected);
    }

    #[rstest]
    #[case("Hello World", "hello", true)]
    #[case("Hello World", "world", false)]
    fn test_caseless_starts_with(
        #[case] haystack: &str,
        #[case] needle: &str,
        #[case] expected: bool,
    ) {
        assert_eq!(caseless_starts_with(haystack, needle), expected);
    }

    #[rstest]
    #[case("Hello World", "WORLD", true)]
    #[case("Hello World", "hello", false)]
    #[case("ΛΟΓΟΣ", "Σ", true)]
    fn test_caseless_ends_with(
        #[case] haystack: &str,
        #[case] needle: &str,
        #[case] expected: bool,
    ) {
        assert_eq!(caseless_ends_with(haystack, needle), expected);
    }
}
