//! Integration tests for the `like` (glob) and `matches` (regex) operators,
//! exercised end-to-end through the public `Filter` API.

use filt_rs::{Filter, FilterValue, Filterable};
use rstest::rstest;

struct Branch {
    name: &'static str,
    protected: bool,
    reviewers: Vec<&'static str>,
}

impl Default for Branch {
    fn default() -> Self {
        Self {
            name: "feat/login",
            protected: false,
            reviewers: vec!["Alice", "Bob"],
        }
    }
}

impl Branch {
    fn named(name: &'static str) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }
}

impl Filterable for Branch {
    fn get(&self, key: &str) -> FilterValue<'_> {
        match key {
            "branch.name" => self.name.into(),
            "branch.protected" => self.protected.into(),
            "branch.reviewers" => self
                .reviewers
                .iter()
                .map(|&r| r.into())
                .collect::<Vec<FilterValue<'_>>>()
                .into(),
            _ => FilterValue::Null,
        }
    }
}

fn matches_branch(filter: &str, branch: &Branch) -> bool {
    Filter::new(filter)
        .expect("the filter should parse")
        .matches(branch)
        .expect("the filter should evaluate")
}

mod like {
    use super::*;

    #[rstest]
    #[case("feat/login", true)]
    #[case("feat/", true)]
    #[case("FEAT/LOGIN", true)] // case-insensitive
    #[case("fix/typo", false)]
    #[case("feat", false)]
    fn the_headline_example(#[case] name: &'static str, #[case] expected: bool) {
        assert_eq!(
            matches_branch(r#"branch.name like "feat/*""#, &Branch::named(name)),
            expected
        );
    }

    #[rstest]
    // Suffix, infix, and single-character wildcards.
    #[case(r#"branch.name like "*fix""#, "hotfix", true)]
    #[case(r#"branch.name like "*fix""#, "fixes", false)]
    #[case(r#"branch.name like "*at/lo*""#, "feat/login", true)]
    #[case(r#"branch.name like "v?.?""#, "v1.2", true)]
    #[case(r#"branch.name like "v?.?""#, "v1.23", false)]
    // `*` matches everything, including the empty string.
    #[case(r#"branch.name like "*""#, "", true)]
    #[case(r#"branch.name like "*""#, "anything", true)]
    // The empty pattern only matches the empty string.
    #[case(r#"branch.name like """#, "", true)]
    #[case(r#"branch.name like """#, "a", false)]
    // Unicode: `?` consumes a character, not a byte, and casing folds.
    #[case(r#"branch.name like "h?llo""#, "héllo", true)]
    #[case(r#"branch.name like "über/*""#, "ÜBER/MUT", true)]
    // Backtracking traps.
    #[case(r#"branch.name like "*a*ab""#, "aaab", true)]
    #[case(r#"branch.name like "*a*ab""#, "aaba", false)]
    // Escaped wildcards match literally.
    #[case(r#"branch.name like "a\*b""#, "a*b", true)]
    #[case(r#"branch.name like "a\*b""#, "axb", false)]
    #[case(r#"branch.name like "a\?b""#, "a?b", true)]
    #[case(r#"branch.name like "a\?b""#, "axb", false)]
    // Literal-only patterns are case-insensitive equality.
    #[case(r#"branch.name like "main""#, "Main", true)]
    #[case(r#"branch.name like "main""#, "remain", false)]
    fn glob_semantics(#[case] filter: &str, #[case] name: &'static str, #[case] expected: bool) {
        assert_eq!(matches_branch(filter, &Branch::named(name)), expected);
    }

    #[rstest]
    // Tuples match when any string element matches.
    #[case(r#"branch.reviewers like "ali*""#, true)]
    #[case(r#"branch.reviewers like "b?b""#, true)]
    #[case(r#"branch.reviewers like "carol""#, false)]
    // Non-string values never match, even against the match-all pattern.
    #[case(r#"branch.protected like "*""#, false)]
    #[case(r#"branch.missing like "*""#, false)]
    fn non_string_semantics(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches_branch(filter, &Branch::default()), expected);
    }

    #[test]
    fn like_composes_with_other_operators() {
        let branch = Branch::default();
        assert!(matches_branch(
            r#"branch.name like "feat/*" && !branch.protected"#,
            &branch
        ));
        assert!(matches_branch(
            r#"branch.name like "fix/*" || branch.name like "feat/*""#,
            &branch
        ));
        assert!(matches_branch(r#"!(branch.name like "fix/*")"#, &branch));
    }

    #[test]
    fn debug_output_shows_the_compiled_expression() {
        let filter = Filter::new(r#"branch.name like "feat/*""#).expect("parse the filter");
        assert_eq!(
            format!("{filter:?}"),
            r#"(like (property branch.name) "feat/*")"#
        );
    }

    #[test]
    fn display_round_trips_the_raw_expression() {
        let raw = r#"branch.name like "feat/*""#;
        let filter = Filter::new(raw).expect("parse the filter");
        assert_eq!(filter.to_string(), raw);
        assert_eq!(filter.raw(), raw);
    }

    #[rstest]
    #[case(
        r#"branch.name like branch.other"#,
        "must be followed by its pattern as a string literal"
    )]
    #[case(
        r#"branch.name like 5"#,
        "must be followed by its pattern as a string literal"
    )]
    #[case(
        r#"branch.name like ["feat/*"]"#,
        "must be followed by its pattern as a string literal"
    )]
    #[case(
        r#"branch.name like"#,
        "while looking for the string pattern of the 'like' operator"
    )]
    fn non_literal_patterns_fail_to_parse(#[case] filter: &str, #[case] message: &str) {
        let error = Filter::new(filter).expect_err("the filter should fail to parse");
        assert!(
            error.to_string().contains(message),
            "expected the error to contain '{message}', got '{error}'"
        );
    }
}

mod like_cs {
    use super::*;

    #[rstest]
    // Wildcards are unchanged, but literal characters must match exactly.
    #[case(r#"branch.name like_cs "feat/*""#, "feat/login", true)]
    #[case(r#"branch.name like_cs "feat/*""#, "FEAT/LOGIN", false)]
    #[case(r#"branch.name like_cs "Feat/*""#, "Feat/login", true)]
    #[case(r#"branch.name like_cs "main""#, "main", true)]
    #[case(r#"branch.name like_cs "main""#, "Main", false)]
    #[case(r#"branch.name like_cs "v?.?""#, "v1.2", true)]
    // No case folding: ß is a single character and never matches "ss".
    #[case(r#"branch.name like_cs "gro?""#, "groß", true)]
    #[case(r#"branch.name like_cs "gro*ss""#, "groß", false)]
    fn glob_semantics(#[case] filter: &str, #[case] name: &'static str, #[case] expected: bool) {
        assert_eq!(matches_branch(filter, &Branch::named(name)), expected);
    }

    #[rstest]
    #[case(r#"branch.reviewers like_cs "Ali*""#, true)]
    #[case(r#"branch.reviewers like_cs "ali*""#, false)]
    #[case(r#"branch.protected like_cs "*""#, false)]
    fn non_string_semantics(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches_branch(filter, &Branch::default()), expected);
    }

    #[test]
    fn debug_output_shows_the_compiled_expression() {
        let filter = Filter::new(r#"branch.name like_cs "Feat/*""#).expect("parse the filter");
        assert_eq!(
            format!("{filter:?}"),
            r#"(like_cs (property branch.name) "Feat/*")"#
        );
    }

    #[test]
    fn non_literal_patterns_fail_to_parse() {
        let error = Filter::new(r#"branch.name like_cs branch.other"#)
            .expect_err("the filter should fail to parse");
        assert!(
            error
                .to_string()
                .contains("must be followed by its pattern as a string literal"),
            "unexpected error: {error}"
        );
    }
}

mod case_sensitive_operators {
    use super::*;

    #[rstest]
    #[case(r#"branch.name contains_cs "feat""#, true)]
    #[case(r#"branch.name contains_cs "FEAT""#, false)]
    #[case(r#"branch.name contains "FEAT""#, true)]
    #[case(r#""feat" in_cs branch.name"#, true)]
    #[case(r#""FEAT" in_cs branch.name"#, false)]
    #[case(r#"branch.name startswith_cs "feat/""#, true)]
    #[case(r#"branch.name startswith_cs "Feat/""#, false)]
    #[case(r#"branch.name endswith_cs "login""#, true)]
    #[case(r#"branch.name endswith_cs "LOGIN""#, false)]
    // Multi-character folds don't apply to the case-sensitive operators.
    #[case(r#""straße" contains_cs "ss""#, false)]
    #[case(r#""straße" contains "ss""#, true)]
    // Tuple membership compares elements case-sensitively too.
    #[case(r#"branch.reviewers contains_cs "Alice""#, true)]
    #[case(r#"branch.reviewers contains_cs "alice""#, false)]
    #[case(r#""Bob" in_cs branch.reviewers"#, true)]
    #[case(r#""bob" in_cs branch.reviewers"#, false)]
    // Cross-type comparisons remain lenient.
    #[case(r#"branch.missing contains_cs "x""#, false)]
    #[case(r#"branch.protected startswith_cs "f""#, false)]
    fn semantics(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches_branch(filter, &Branch::default()), expected);
    }
}

#[cfg(feature = "regex")]
mod matches {
    use super::*;

    #[rstest]
    #[case("release/v1.2.3", true)]
    #[case("release/v10.20.30", true)]
    #[case("release/v1.2", false)]
    #[case("release/v1.2.3.4", false)]
    #[case("feat/login", false)]
    fn the_headline_example(#[case] name: &'static str, #[case] expected: bool) {
        assert_eq!(
            matches_branch(
                r#"branch.name matches r"^release/v\d+(\.\d+){2}$""#,
                &Branch::named(name)
            ),
            expected
        );
    }

    #[rstest]
    // Unanchored by default.
    #[case(r#"branch.name matches r"eat""#, "feat/login", true)]
    // Case-sensitive as written, opt out with (?i).
    #[case(r#"branch.name matches r"FEAT""#, "feat/login", false)]
    #[case(r#"branch.name matches r"(?i)FEAT""#, "feat/login", true)]
    // Plain strings work too, with standard escape processing applied first.
    #[case(r#"branch.name matches "^feat/\\w+$""#, "feat/login", true)]
    #[case(r#"branch.name matches "^feat$""#, "feat/login", false)]
    fn regex_semantics(#[case] filter: &str, #[case] name: &'static str, #[case] expected: bool) {
        assert_eq!(matches_branch(filter, &Branch::named(name)), expected);
    }

    #[rstest]
    // Tuples match when any string element matches.
    #[case(r#"branch.reviewers matches r"^A\w+$""#, true)]
    #[case(r#"branch.reviewers matches r"^C\w+$""#, false)]
    // Non-string values never match.
    #[case(r#"branch.protected matches r".*""#, false)]
    #[case(r#"branch.missing matches r".*""#, false)]
    fn non_string_semantics(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches_branch(filter, &Branch::default()), expected);
    }

    #[test]
    fn debug_output_shows_the_compiled_expression() {
        let filter = Filter::new(r#"branch.name matches r"^feat/.+$""#).expect("parse the filter");
        assert_eq!(
            format!("{filter:?}"),
            r#"(matches (property branch.name) "^feat/.+$")"#
        );
    }

    #[test]
    fn display_round_trips_the_raw_expression() {
        let raw = r#"branch.name matches r"^release/v\d+(\.\d+){2}$""#;
        let filter = Filter::new(raw).expect("parse the filter");
        assert_eq!(filter.to_string(), raw);
        assert_eq!(filter.raw(), raw);
    }

    #[test]
    fn invalid_patterns_fail_to_parse_with_details() {
        let error = Filter::new(r#"branch.name matches r"(unclosed""#)
            .expect_err("the filter should fail to parse");
        let message = error.to_string();
        assert!(
            message.contains("Failed to compile the regular expression pattern"),
            "unexpected error: {message}"
        );
        assert!(
            message.contains("unclosed group"),
            "unexpected error: {message}"
        );
    }

    #[rstest]
    #[case(
        r#"branch.name matches branch.other"#,
        "must be followed by its pattern as a string literal"
    )]
    #[case(
        r#"branch.name matches"#,
        "while looking for the string pattern of the 'matches' operator"
    )]
    #[case(
        r#"branch.name matches r"unterminated"#,
        "without finding the closing quote for a raw string"
    )]
    fn non_literal_patterns_fail_to_parse(#[case] filter: &str, #[case] message: &str) {
        let error = Filter::new(filter).expect_err("the filter should fail to parse");
        assert!(
            error.to_string().contains(message),
            "expected the error to contain '{message}', got '{error}'"
        );
    }
}

#[cfg(not(feature = "regex"))]
mod matches_without_the_regex_feature {
    use super::*;

    #[test]
    fn fails_to_parse_with_friendly_advice() {
        let error = Filter::new(r#"branch.name matches r"^feat/.+$""#)
            .expect_err("the filter should fail to parse");
        let message = error.to_string();
        assert!(
            message.contains("does not include regular expression support"),
            "unexpected error: {message}"
        );
        assert!(
            message.contains("Enable the 'regex' feature"),
            "unexpected error: {message}"
        );
        // Suppress the unused-fixture warnings in this configuration.
        let _ = matches_branch(r#"branch.name like "feat/*""#, &Branch::default());
    }
}
