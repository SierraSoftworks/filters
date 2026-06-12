//! Centralized case-insensitivity rules for the filter language.
//!
//! Most of the filter language's string operations compare case-insensitively
//! (`contains`, `startswith`, `endswith`, and the `like` glob operator). All
//! of those comparisons are routed through this module so that every operator
//! folds case in exactly the same way.
//!
//! Folding is performed character-by-character using [`char::to_lowercase`]
//! (which never allocates), with one refinement: the Greek word-final sigma
//! (`ς`) is normalized to the regular lowercase sigma (`σ`), so all three
//! sigma forms (`Σ`, `σ`, `ς`) compare equal regardless of their position
//! within a word. [`str::to_lowercase`]'s context-sensitive final-sigma rule
//! would make results depend on position; folding to a single form mirrors
//! Unicode simple case folding and keeps comparisons symmetric.
//!
//! Note that string *equality* (`==`) deliberately uses ASCII-only case
//! folding ([`str::eq_ignore_ascii_case`]) and does not go through this
//! module — that pre-existing asymmetry is pinned by the crate's behavioural
//! tests.

/// Folds a lowercase Greek final sigma (`ς`) into the regular lowercase
/// sigma (`σ`), mirroring Unicode simple case folding.
///
/// [`char::to_lowercase`] always produces `σ` for an uppercase `Σ` (it has
/// no knowledge of the character's position within a word), so folding `ς`
/// as well makes the case-insensitive comparisons in this module treat all
/// three sigma forms as equivalent regardless of their position.
fn fold_sigma(c: char) -> char {
    if c == 'ς' { 'σ' } else { c }
}

/// Iterates over the case-folded form of a single character without
/// allocating. Lowercase expansions may span multiple characters (e.g. `İ`
/// lowercases to `i` followed by a combining dot above).
fn casefold_char(c: char) -> impl Iterator<Item = char> + Clone {
    c.to_lowercase().map(fold_sigma)
}

/// Iterates over the case-folded characters of a string without allocating.
///
/// This matches the characters produced by [`str::to_lowercase`] except for
/// the Greek final-sigma context rule: all sigma forms are normalized to
/// `σ` (see [`fold_sigma`]).
pub(crate) fn casefold(s: &str) -> impl Iterator<Item = char> + Clone + '_ {
    s.chars().flat_map(casefold_char)
}

/// Iterates over the case-folded characters of a string in reverse order
/// without allocating. Equivalent to reversing [`casefold`].
pub(crate) fn casefold_rev(s: &str) -> impl Iterator<Item = char> + Clone + '_ {
    s.chars()
        .rev()
        .flat_map(|c| c.to_lowercase().rev().map(fold_sigma))
}

/// Determines whether two characters are equal under this module's case
/// folding rules, comparing their folded expansions without allocating.
///
/// This is the single-character counterpart of [`casefold`], used by the
/// glob matcher (where `?` and `*` consume whole input characters, so
/// literal characters are compared pairwise). Multi-character expansions
/// (e.g. `İ`) only compare equal to characters with the same expansion.
pub(crate) fn chars_eq(a: char, b: char) -> bool {
    a == b || casefold_char(a).eq(casefold_char(b))
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
    #[case('a', 'a', true)]
    #[case('a', 'A', true)]
    #[case('A', 'a', true)]
    #[case('a', 'b', false)]
    #[case('ü', 'Ü', true)]
    // All Greek sigma forms are equivalent, regardless of position.
    #[case('σ', 'ς', true)]
    #[case('ς', 'σ', true)]
    #[case('Σ', 'ς', true)]
    #[case('Σ', 'σ', true)]
    // Multi-character expansions only equal characters with the same expansion.
    #[case('İ', 'İ', true)]
    #[case('İ', 'i', false)]
    #[case('ß', 'ß', true)]
    #[case('ß', 's', false)]
    fn test_chars_eq(#[case] a: char, #[case] b: char, #[case] expected: bool) {
        assert_eq!(chars_eq(a, b), expected);
        assert_eq!(chars_eq(b, a), expected);
    }

    #[rstest]
    #[case("Hello World", "hello world")]
    #[case("ΛΟΓΟΣ", "λογοσ")] // final sigma folds to σ, not ς
    #[case("λογος", "λογοσ")]
    #[case("İstanbul", "i\u{307}stanbul")]
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
