//! Integration tests exercising the public API of the `filt-rs` crate,
//! with a focus on edge cases, failure modes, and behavioural quirks
//! which the unit tests don't cover.

use filt_rs::{Filter, FilterValue, Filterable};
use rstest::rstest;

/// A reasonably rich object for exercising the filter language end-to-end.
struct Document {
    title: String,
    pages: f64,
    published: bool,
    rating: Option<f64>,
    tags: Vec<&'static str>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            title: "The Rust Book".to_string(),
            pages: 552.0,
            published: true,
            rating: Some(4.8),
            tags: vec!["rust", "programming", "free"],
        }
    }
}

impl Filterable for Document {
    fn get(&self, key: &str) -> FilterValue<'_> {
        match key {
            "doc.title" => self.title.as_str().into(),
            "doc.pages" => self.pages.into(),
            "doc.published" => self.published.into(),
            "doc.rating" => self.rating.into(),
            "doc.tags" => self
                .tags
                .iter()
                .map(|&t| t.into())
                .collect::<Vec<FilterValue<'_>>>()
                .into(),
            "doc.nan" => f64::NAN.into(),
            _ => FilterValue::Null,
        }
    }
}

fn matches(filter: &str) -> bool {
    Filter::new(filter)
        .expect("the filter should parse")
        .matches(&Document::default())
        .expect("the filter should evaluate")
}

fn parse_error(filter: &str) -> String {
    Filter::new(filter)
        .expect_err("the filter should fail to parse")
        .to_string()
}

mod construction {
    use super::*;

    #[test]
    fn accepts_both_str_and_string() {
        Filter::new("true").expect("&str should parse");
        Filter::new(String::from("true")).expect("String should parse");
    }

    #[test]
    fn empty_filters_are_rejected() {
        assert!(parse_error("").contains("end of your filter expression"));
        assert!(parse_error("   \t\n  ").contains("end of your filter expression"));
    }

    #[test]
    fn filters_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Filter>();
    }

    #[test]
    fn filters_can_be_shared_across_threads() {
        let filter = Filter::new("doc.pages > 100").expect("parse filter");
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    assert!(filter.matches(&Document::default()).unwrap());
                });
            }
        });
    }

    #[test]
    fn a_filter_can_be_reused_across_many_objects() {
        let filter = Filter::new("doc.pages > 100").expect("parse filter");
        for pages in 0..1000 {
            let doc = Document {
                pages: pages as f64,
                ..Document::default()
            };
            assert_eq!(filter.matches(&doc).unwrap(), pages > 100);
        }
    }
}

mod literals_and_truthiness {
    use super::*;

    #[rstest]
    #[case("true", true)]
    #[case("false", false)]
    #[case("null", false)]
    #[case("0", false)]
    #[case("0.0", false)]
    #[case("42", true)]
    #[case("0.001", true)]
    #[case("\"\"", false)]
    #[case("\"false\"", true)] // non-empty strings are truthy, even "false"
    #[case("[]", false)]
    #[case("[false]", true)] // non-empty tuples are truthy, even [false]
    fn literal_truthiness(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches(filter), expected);
    }

    #[rstest]
    #[case("007 == 7", true)] // leading zeros are permitted
    #[case("30 == 30.0", true)] // all numbers are floats internally
    #[case("0.5 < 1", true)]
    #[case("4.8 == 4.8", true)]
    fn number_semantics(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches(filter), expected);
    }

    #[test]
    fn negative_numbers_are_not_literals() {
        // There is no unary minus: a leading '-' is the subtraction operator,
        // so "-5" on its own is a parse error rather than a negative literal.
        // (Earlier versions lexed "-5" as a property named "-5"; that quirk
        // was retired when arithmetic was introduced.)
        assert!(parse_error("-5 == null").contains("unexpected '-'"));

        // Negative values can be produced via subtraction instead.
        assert!(matches("0 - 5 < 0"));
        assert!(matches("0 - 5 == 0 - 5"));
    }
}

mod arithmetic {
    use super::*;

    #[rstest]
    #[case("1 + 2 == 3", true)]
    #[case("doc.pages - 2 == 550", true)]
    #[case("doc.pages + 100 > 600", true)] // arithmetic binds tighter than comparisons
    #[case("1 + 2 + 3 - 4 == 2", true)] // chains are evaluated left-to-right
    #[case("doc.pages + null == null", true)] // mismatched operand types yield null
    #[case("\"a\" + \"b\" == null", true)] // there is no string concatenation
    fn arithmetic_semantics(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches(filter), expected);
    }

    #[test]
    fn hyphenated_property_names_are_not_subtraction() {
        // A '-' inside an identifier remains part of the property name, so
        // this resolves the (undefined) property "doc.pages-2" rather than
        // subtracting 2 from doc.pages...
        assert!(matches("doc.pages-2 == null"));

        // ...while surrounding the '-' with whitespace makes it an operator.
        assert!(matches("doc.pages - 2 == 550"));

        // Properties like asset.source-code keep working as a single name.
        assert!(matches("asset.source-code == null"));
    }

    #[test]
    fn there_is_no_unary_minus_or_plus() {
        assert!(parse_error("-5").contains("unexpected '-'"));
        assert!(parse_error("+5").contains("unexpected '+'"));
        assert!(parse_error("doc.pages > -5").contains("unexpected '-'"));
    }
}

mod properties {
    use super::*;

    #[rstest]
    #[case("doc.title", true)]
    #[case("doc.published", true)]
    #[case("doc.rating", true)]
    #[case("missing", false)]
    fn property_truthiness(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches(filter), expected);
    }

    #[test]
    fn missing_properties_are_null() {
        assert!(matches("missing == null"));
        assert!(matches("!(missing != null)"));
        assert!(matches("no.such.property-name == null"));
    }

    #[test]
    fn option_none_properties_are_null() {
        let doc = Document {
            rating: None,
            ..Document::default()
        };
        let filter = Filter::new("doc.rating == null").expect("parse filter");
        assert!(filter.matches(&doc).unwrap());
    }

    #[rstest]
    #[case("infra")] // starts with the "in" keyword
    #[case("containsx")] // starts with the "contains" keyword
    #[case("truthy")] // starts with the "true" keyword
    #[case("nullable")] // starts with the "null" keyword
    fn keyword_prefixed_identifiers_are_properties(#[case] name: &str) {
        // These must parse as (null) property references, not keywords.
        assert!(matches(&format!("{name} == null")));
    }

    #[test]
    fn keywords_cannot_be_used_as_property_names() {
        // `contains` is an operator, so a property of that name is unreachable.
        assert!(Filter::new("contains == null").is_err());
    }
}

mod strings {
    use super::*;

    #[rstest]
    #[case(r#"doc.title == "the rust book""#, true)] // equality ignores case
    #[case(r#"doc.title == "THE RUST BOOK""#, true)]
    #[case(r#"doc.title contains "RUST""#, true)]
    #[case(r#"doc.title startswith "the""#, true)]
    #[case(r#"doc.title endswith "BOOK""#, true)]
    fn string_operations_ignore_case(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches(filter), expected);
    }

    #[test]
    fn escaped_quotes_round_trip() {
        assert!(matches(r#""say \"hi\"" == "say \"hi\"""#));
        assert!(matches(r#""say \"hi\"" contains "\"hi\"""#));
    }

    #[test]
    fn unicode_strings_are_supported() {
        let doc = Document {
            title: "Jürgen's Café Guide ☕".to_string(),
            ..Document::default()
        };
        let filter = Filter::new(r#"doc.title contains "café""#).expect("parse filter");
        assert!(filter.matches(&doc).unwrap());

        let filter = Filter::new(r#"doc.title contains "☕""#).expect("parse filter");
        assert!(filter.matches(&doc).unwrap());
    }

    #[test]
    fn equality_uses_unicode_case_folding() {
        // Equality folds case with the same Unicode rules as the rest of the
        // language, so non-ASCII characters compare case-insensitively too.
        let doc = Document {
            title: "JÜRGEN".to_string(),
            ..Document::default()
        };

        let eq = Filter::new(r#"doc.title == "jürgen""#).expect("parse filter");
        assert!(eq.matches(&doc).unwrap());

        let contains = Filter::new(r#"doc.title contains "jürgen""#).expect("parse filter");
        assert!(contains.matches(&doc).unwrap());
    }

    #[test]
    fn multi_character_case_folds_are_supported() {
        // Characters whose case-folded form spans several characters (e.g.
        // ß → ss) participate fully in every case-insensitive operator.
        assert!(matches(r#""straße" == "STRASSE""#));
        assert!(matches(r#""groß" endswith "SS""#));
        assert!(matches(r#""ss" in "groß""#));
    }

    #[test]
    fn strings_spanning_lines_are_supported() {
        assert!(matches("\"multi\nline\" contains \"multi\""));
    }

    #[test]
    fn greek_sigma_forms_are_interchangeable() {
        // The case-insensitive operators fold all Greek sigma forms (Σ, σ,
        // and the word-final ς) together, so sigma comparisons do not depend
        // on where the character appears within a word.
        assert!(matches(r#""ΛΟΓΟΣ" endswith "Σ""#));
        assert!(matches(r#""ΛΟΓΟΣ" endswith "ς""#));
        assert!(matches(r#""λογος" contains "ΓΟΣ""#));
    }
}

mod tuples {
    use super::*;

    #[rstest]
    #[case(r#"doc.tags contains "rust""#, true)]
    #[case(r#"doc.tags contains "RUST""#, true)] // membership ignores case
    #[case(r#"doc.tags contains "go""#, false)]
    #[case(r#""rust" in doc.tags"#, true)]
    #[case(r#"doc.tags == ["rust", "programming", "free"]"#, true)]
    #[case(r#"doc.tags == ["rust", "programming"]"#, false)] // length matters
    #[case(r#"doc.tags == ["free", "programming", "rust"]"#, false)] // order matters
    #[case(r#"doc.tags != []"#, true)]
    fn tuple_operations(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches(filter), expected);
    }

    #[test]
    fn tuples_may_contain_mixed_literal_types() {
        assert!(matches(r#"[1, "two", true, null] contains "TWO""#));
        assert!(matches(r#"null in [1, "two", true, null]"#));
    }

    #[test]
    fn nested_tuples_are_not_supported() {
        assert!(parse_error("[[1]]").contains("unexpected '['"));
    }

    #[test]
    fn expressions_inside_tuples_are_not_supported() {
        // Tuples may only contain literals, not arbitrary expressions.
        assert!(Filter::new("[doc.pages]").is_err());
        assert!(Filter::new("[1 + 2]").is_err());
    }
}

mod comparisons {
    use super::*;

    #[rstest]
    #[case("doc.pages > 100", true)]
    #[case("doc.pages >= 552", true)]
    #[case("doc.pages < 1000", true)]
    #[case("doc.pages <= 551", false)]
    #[case("true > false", true)] // booleans are ordered
    #[case("\"abc\" < \"abd\"", true)] // strings are ordered (case-sensitively)
    fn ordering(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches(filter), expected);
    }

    #[rstest]
    #[case("doc.pages > \"100\"")] // number vs string
    #[case("doc.pages < \"100\"")]
    #[case("doc.title > 5")] // string vs number
    #[case("doc.published > null")] // bool vs null
    #[case("doc.pages == doc.title")] // mismatched equality
    fn mismatched_types_never_match(#[case] filter: &str) {
        assert!(!matches(filter));
    }

    #[rstest]
    #[case("doc.nan == doc.nan", false)] // NaN is not equal to itself
    #[case("doc.nan != doc.nan", true)]
    #[case("doc.nan > 0", false)]
    #[case("doc.nan < 0", false)]
    #[case("doc.nan", true)] // ...but NaN is truthy (it isn't zero)
    fn nan_semantics(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches(filter), expected);
    }

    #[test]
    fn comparisons_cannot_be_chained() {
        assert!(parse_error("1 < 2 < 3").contains("unexpected '<'"));
        assert!(parse_error("true == true == true").contains("unexpected '=='"));
    }
}

mod logic {
    use super::*;

    #[rstest]
    #[case("doc.published && doc.pages > 100 && doc.rating >= 4", true)]
    #[case("!doc.published || doc.pages > 100", true)]
    #[case("!(doc.published && doc.pages < 100)", true)]
    #[case("!!doc.published", true)]
    #[case("!!!doc.published", false)]
    fn combinations(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches(filter), expected);
    }

    #[test]
    fn not_binds_tighter_than_comparisons() {
        // `!a == b` parses as `(!a) == b`, not `!(a == b)`.
        assert!(matches("!doc.published == false"));
    }

    #[test]
    fn and_binds_tighter_than_or() {
        // `a || b && c` parses as `a || (b && c)`.
        assert!(matches("true || true && false"));
        assert!(!matches("(true || true) && false"));
    }

    #[test]
    fn logical_operators_return_operand_values() {
        // Like many scripting languages, && and || return the deciding
        // operand's value rather than a boolean.
        assert!(matches(r#"(doc.title && doc.pages) == 552"#));
        assert!(matches(r#"(null || doc.title) == "the rust book""#));
    }

    #[test]
    fn deeply_nested_groups_parse_and_evaluate() {
        let depth = 100;
        let filter = format!("{}true{}", "(".repeat(depth), ")".repeat(depth));
        assert!(matches(&filter));
    }

    #[test]
    fn long_operator_chains_parse_and_evaluate() {
        let filter = vec!["doc.published"; 500].join(" && ");
        assert!(matches(&filter));

        let filter = vec!["missing"; 500].join(" || ");
        assert!(!matches(&filter));
    }
}

mod functions {
    use super::*;

    #[test]
    fn unknown_functions_fail_at_parse_time() {
        let error = parse_error("nope()");
        assert!(
            error.contains("unknown function 'nope()'"),
            "expected an unknown-function error, got: {error}"
        );
        assert!(
            error.contains("now()"),
            "expected the error to list the supported functions, got: {error}"
        );
    }

    #[test]
    fn unclosed_function_calls_are_rejected() {
        assert!(parse_error("now(1").contains("didn't find the closing ')'"));
        assert!(parse_error("now(").contains("end of your filter expression"));
    }

    #[cfg(not(feature = "chrono"))]
    #[test]
    fn now_requires_the_chrono_feature() {
        assert!(parse_error("now()").contains("'chrono' feature"));
    }
}

mod durations {
    use super::*;

    #[rstest]
    #[case("5x")]
    #[case("5mm")]
    #[case("5min")]
    #[case("1h30")]
    fn malformed_durations_are_rejected(#[case] filter: &str) {
        let error = parse_error(filter);
        assert!(
            error.contains("duration"),
            "expected a duration error for '{filter}', got: {error}"
        );
    }

    #[cfg(not(feature = "chrono"))]
    #[test]
    fn duration_literals_require_the_chrono_feature() {
        assert!(parse_error("5m").contains("'chrono' feature"));
        assert!(parse_error("uploaded.age > 1h30m").contains("'chrono' feature"));
    }
}

#[cfg(feature = "chrono")]
mod datetimes {
    use super::*;
    use chrono::{Duration, Utc};

    struct Event {
        timestamp: chrono::DateTime<Utc>,
    }

    impl Filterable for Event {
        fn get(&self, key: &str) -> FilterValue<'_> {
            match key {
                "event.timestamp" => self.timestamp.into(),
                _ => FilterValue::Null,
            }
        }
    }

    #[test]
    fn events_can_be_filtered_by_relative_time() {
        let filter = Filter::new("event.timestamp > now() - 5m").expect("parse filter");

        let recent = Event {
            timestamp: Utc::now(),
        };
        assert!(filter.matches(&recent).expect("run filter"));

        let stale = Event {
            timestamp: Utc::now() - Duration::minutes(10),
        };
        assert!(!filter.matches(&stale).expect("run filter"));
    }

    #[test]
    fn now_is_evaluated_at_filtering_time() {
        // A single parsed filter sees a fresh "now" on every matches() call,
        // so an event which is too old right now can still match later.
        let filter = Filter::new("now() - event.timestamp >= 1ms").expect("parse filter");

        let event = Event {
            timestamp: Utc::now(),
        };
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(filter.matches(&event).expect("run filter"));
    }

    #[rstest]
    #[case("now() + 1h > now()", true)]
    #[case("now() - now() < 1s", true)]
    #[case("now() - now() > 0s - 1s", true)] // approximately the zero duration
    #[case("5m < 1h", true)]
    #[case("60s == 1m", true)]
    #[case("1h30m == 90m", true)]
    #[case("500ms + 500ms == 1s", true)]
    #[case("1w == 7d", true)]
    #[case("now() == 5m", false)] // datetimes and durations never compare equal
    fn datetime_and_duration_semantics(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches(filter), expected);
    }

    #[test]
    fn now_rejects_arguments_at_parse_time() {
        assert!(parse_error("now(1)").contains("does not accept any arguments"));
    }
}

mod failure_modes {
    use super::*;

    #[rstest]
    #[case("doc.pages >", "end of your filter expression")]
    #[case("&& true", "unexpected '&&'")]
    #[case("true && ", "end of your filter expression")]
    #[case("(true || false", "didn't find the closing ')'")]
    #[case("[1, 2", "didn't find the closing ']'")]
    #[case("a = b", "orphaned '='")]
    #[case("a & b", "orphaned '&'")]
    #[case("a | b", "orphaned '|'")]
    #[case("\"unterminated", "without finding the closing quote")]
    #[case("!", "end of your filter expression")]
    fn parse_errors_are_descriptive(#[case] filter: &str, #[case] message: &str) {
        let error = parse_error(filter);
        assert!(
            error.contains(message),
            "expected the error for '{filter}' to contain '{message}', got: {error}"
        );
    }

    #[test]
    fn errors_include_accurate_locations_across_lines() {
        let error = parse_error("doc.published &&\ndoc.pages =");
        assert!(
            error.contains("line 2, column 11"),
            "expected a line 2 location in: {error}"
        );
    }

    #[test]
    fn trailing_tokens_are_rejected() {
        assert!(parse_error("true false").contains("unexpected 'false'"));
        // String tokens are displayed with quotes in error messages.
        let error = parse_error(r#"true "oops""#);
        assert!(
            error.contains(r#"unexpected '"oops"'"#),
            "expected the quoted string in: {error}"
        );
    }

    #[test]
    fn malformed_numbers_are_rejected() {
        // "1.x" lexes as the number "1" followed by the property ".x".
        assert!(parse_error("1.x").contains("unexpected '.x'"));
    }

    #[test]
    fn errors_offer_remediation_advice() {
        let error = parse_error("a & b");
        assert!(
            error.contains("'&&' operator"),
            "expected remediation advice in: {error}"
        );
    }
}

mod formatting {
    use super::*;

    #[test]
    fn display_preserves_the_original_expression() {
        let raw = r#"doc.published && doc.title contains "rust""#;
        let filter = Filter::new(raw).expect("parse filter");
        assert_eq!(filter.to_string(), raw);
        assert_eq!(filter.raw(), raw);
    }

    #[test]
    fn debug_shows_the_parse_tree() {
        let filter = Filter::new("a || b && c").expect("parse filter");
        assert_eq!(
            format!("{filter:?}"),
            "(|| (property a) (&& (property b) (property c)))"
        );
    }
}
