//! Behavioural tests for the `secrecy` feature, proving that secret values
//! behave exactly like strings in every filter operation while remaining
//! impossible to print in un-redacted form.

#![cfg(feature = "secrecy")]

use filt_rs::{Filter, FilterValue, Filterable};
use rstest::rstest;
use secrecy::SecretString;

/// A user record whose password (and API keys) must never leak into logs.
struct User {
    name: &'static str,
    password: SecretString,
    api_keys: Vec<SecretString>,
}

impl Default for User {
    fn default() -> Self {
        Self {
            name: "alice",
            password: SecretString::from("hunter2"),
            api_keys: vec![SecretString::from("key-1"), SecretString::from("key-2")],
        }
    }
}

impl Filterable for User {
    fn get(&self, key: &str) -> FilterValue {
        match key {
            "user.name" => self.name.into(),
            "user.password" => self.password.clone().into(),
            "user.api-keys" => self
                .api_keys
                .iter()
                .cloned()
                .map(FilterValue::from)
                .collect::<Vec<FilterValue>>()
                .into(),
            _ => FilterValue::Null,
        }
    }
}

fn matches(filter: &str) -> bool {
    Filter::new(filter)
        .expect("the filter should parse")
        .matches(&User::default())
        .expect("the filter should evaluate")
}

mod equality {
    use super::*;

    #[rstest]
    // Secret on the left of the operator...
    #[case(r#"user.password == "hunter2""#, true)]
    #[case(r#"user.password == "HUNTER2""#, true)] // equality ignores case
    #[case(r#"user.password == "swordfish""#, false)]
    #[case(r#"user.password != "hunter2""#, false)]
    #[case(r#"user.password != "swordfish""#, true)]
    // ...and on the right.
    #[case(r#""hunter2" == user.password"#, true)]
    #[case(r#""HUNTER2" == user.password"#, true)]
    #[case(r#""swordfish" == user.password"#, false)]
    #[case(r#""hunter2" != user.password"#, false)]
    #[case(r#""swordfish" != user.password"#, true)]
    // Secret vs secret.
    #[case("user.password == user.password", true)]
    #[case("user.password != user.password", false)]
    fn equality(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches(filter), expected);
    }

    #[rstest]
    #[case("user.password == null", false)]
    #[case("user.password != null", true)]
    #[case("null == user.password", false)]
    #[case("user.password == 42", false)]
    #[case("user.password == true", false)]
    fn mismatched_types_never_match(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches(filter), expected);
    }
}

mod substrings {
    use super::*;

    #[rstest]
    // Secret haystack, plain needle.
    #[case(r#"user.password contains "unte""#, true)]
    #[case(r#"user.password contains "UNTE""#, true)] // membership ignores case
    #[case(r#"user.password contains "xyz""#, false)]
    #[case(r#""unte" in user.password"#, true)]
    #[case(r#""xyz" in user.password"#, false)]
    // Plain haystack, secret needle.
    #[case(r#"user.password in "xxhunter2xx""#, true)]
    #[case(r#"user.password in "xxHUNTER2xx""#, true)]
    #[case(r#"user.password in "xyz""#, false)]
    #[case(r#""xxhunter2xx" contains user.password"#, true)]
    #[case(r#""xyz" contains user.password"#, false)]
    fn contains(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches(filter), expected);
    }

    #[rstest]
    #[case(r#"user.password startswith "hun""#, true)]
    #[case(r#"user.password startswith "HUN""#, true)]
    #[case(r#"user.password startswith "er2""#, false)]
    #[case(r#""hunter2-suffix" startswith user.password"#, true)]
    #[case(r#""prefix-hunter2" startswith user.password"#, false)]
    fn startswith(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches(filter), expected);
    }

    #[rstest]
    #[case(r#"user.password endswith "er2""#, true)]
    #[case(r#"user.password endswith "ER2""#, true)]
    #[case(r#"user.password endswith "hun""#, false)]
    #[case(r#""prefix-hunter2" endswith user.password"#, true)]
    #[case(r#""hunter2-suffix" endswith user.password"#, false)]
    fn endswith(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches(filter), expected);
    }
}

mod ordering {
    use super::*;

    #[rstest]
    // Secret on the left of the operator...
    #[case(r#"user.password > "abc""#, true)]
    #[case(r#"user.password < "zzz""#, true)]
    #[case(r#"user.password < "abc""#, false)]
    #[case(r#"user.password >= "hunter2""#, true)]
    #[case(r#"user.password <= "hunter2""#, true)]
    #[case(r#"user.password >= "zzz""#, false)]
    // ...and on the right.
    #[case(r#""abc" < user.password"#, true)]
    #[case(r#""zzz" > user.password"#, true)]
    #[case(r#""abc" > user.password"#, false)]
    #[case(r#""hunter2" <= user.password"#, true)]
    #[case(r#""hunter2" >= user.password"#, true)]
    #[case(r#""zzz" <= user.password"#, false)]
    // Ordering against non-strings never matches, just like strings.
    #[case("user.password > 5", false)]
    #[case("user.password < 5", false)]
    #[case("user.password >= null", false)]
    fn ordering(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches(filter), expected);
    }
}

mod tuples {
    use super::*;

    #[rstest]
    // A secret needle in a tuple of plain strings...
    #[case(r#"user.password in ["hunter2", "letmein"]"#, true)]
    #[case(r#"user.password in ["LETMEIN", "HUNTER2"]"#, true)]
    #[case(r#"user.password in ["swordfish", "letmein"]"#, false)]
    #[case(r#"["hunter2", "letmein"] contains user.password"#, true)]
    // ...and a plain needle in a tuple of secrets.
    #[case(r#""key-1" in user.api-keys"#, true)]
    #[case(r#""KEY-2" in user.api-keys"#, true)]
    #[case(r#""key-3" in user.api-keys"#, false)]
    #[case(r#"user.api-keys contains "key-1""#, true)]
    #[case(r#"user.api-keys contains "key-3""#, false)]
    fn membership(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(matches(filter), expected);
    }
}

mod truthiness {
    use super::*;

    #[test]
    fn non_empty_secrets_are_truthy() {
        assert!(matches("user.password"));
        assert!(!matches("!user.password"));
        assert!(matches("user.password && user.name == \"alice\""));
    }

    #[test]
    fn empty_secrets_are_falsy() {
        let user = User {
            password: SecretString::from(""),
            ..User::default()
        };
        let filter = Filter::new("user.password").expect("parse filter");
        assert!(!filter.matches(&user).unwrap());

        let filter = Filter::new("!user.password").expect("parse filter");
        assert!(filter.matches(&user).unwrap());
    }
}

mod redaction {
    use super::*;

    #[rstest]
    #[case(FilterValue::secret("hunter2"))]
    #[case(SecretString::from("hunter2").into())]
    #[case(User::default().get("user.password"))]
    #[case(FilterValue::Tuple(vec!["a".into(), FilterValue::secret("hunter2")]))]
    #[case(User::default().get("user.api-keys"))]
    fn formatted_values_never_contain_the_secret(#[case] value: FilterValue) {
        for formatted in [value.to_string(), format!("{value:?}")] {
            assert!(
                !formatted.contains("hunter2") && !formatted.contains("key-1"),
                "the secret leaked into the formatted output: {formatted}"
            );
            assert!(
                formatted.contains("[REDACTED]"),
                "expected the redaction marker in: {formatted}"
            );
        }
    }

    #[test]
    fn secrets_are_redacted_recursively_within_tuples() {
        let value = FilterValue::Tuple(vec![
            "visible".into(),
            FilterValue::secret("hunter2"),
            1.into(),
        ]);
        assert_eq!(value.to_string(), r#"["visible", [REDACTED], 1]"#);
        assert_eq!(format!("{value:?}"), r#"["visible", [REDACTED], 1]"#);
    }
}

mod equivalence {
    use super::*;

    /// Evaluates the same filter against a user whose password property is a
    /// secret, and one where it is a plain string, asserting both agree.
    fn behaves_like_a_string(filter: &str) {
        struct PlainUser;

        impl Filterable for PlainUser {
            fn get(&self, key: &str) -> FilterValue {
                match key {
                    "user.password" => "hunter2".into(),
                    key => User::default().get(key),
                }
            }
        }

        let filter = Filter::new(filter).expect("the filter should parse");
        assert_eq!(
            filter
                .matches(&User::default())
                .expect("the filter should evaluate against the secret"),
            filter
                .matches(&PlainUser)
                .expect("the filter should evaluate against the string"),
            "'{filter}' behaved differently for a secret and a string"
        );
    }

    #[rstest]
    #[case(r#"user.password {op} "hunter2""#)]
    #[case(r#"user.password {op} "HUNTER2""#)]
    #[case(r#"user.password {op} "swordfish""#)]
    #[case(r#"user.password {op} "hun""#)]
    #[case(r#"user.password {op} "zzz""#)]
    #[case(r#"user.password {op} """#)]
    #[case(r#""hunter2" {op} user.password"#)]
    #[case(r#""xxhunter2xx" {op} user.password"#)]
    #[case(r#""abc" {op} user.password"#)]
    #[case(r#"user.password {op} null"#)]
    #[case(r#"user.password {op} 42"#)]
    #[case(r#"user.password {op} ["hunter2", "letmein"]"#)]
    fn secrets_behave_exactly_like_strings(#[case] template: &str) {
        for op in [
            "==",
            "!=",
            "contains",
            "in",
            "startswith",
            "endswith",
            ">",
            "<",
            ">=",
            "<=",
        ] {
            behaves_like_a_string(&template.replace("{op}", op));
        }
    }
}
