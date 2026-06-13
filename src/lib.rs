//! A human-friendly filter expression language for matching your objects against
//! user-provided queries.
//!
//! This crate provides a small, dependency-light filtering DSL designed for
//! situations where your users need to describe *which* items a tool should
//! operate on — for example which repositories to back up, which emails to
//! restore, or which releases to download. It was originally developed for
//! (and extracted from) the Sierra Softworks
//! [`github-backup`](https://github.com/SierraSoftworks/github-backup) and
//! [`mail-backup`](https://github.com/SierraSoftworks/mail-backup) projects.
//!
//! # Quick start
//!
//! Implement the [`Filterable`] trait on your type to expose the properties
//! which may be referenced in a filter expression, then parse a [`Filter`]
//! and evaluate it against your objects.
//!
//! ```
//! use filt_rs::{Filter, FilterValue, Filterable};
//!
//! struct Repo {
//!     name: &'static str,
//!     public: bool,
//!     stars: u32,
//! }
//!
//! impl Filterable for Repo {
//!     fn get(&self, key: &str) -> FilterValue {
//!         match key {
//!             "repo.name" => self.name.into(),
//!             "repo.public" => self.public.into(),
//!             "repo.stars" => self.stars.into(),
//!             _ => FilterValue::Null,
//!         }
//!     }
//! }
//!
//! # fn main() -> Result<(), filt_rs::Error> {
//! let filter = Filter::new("repo.public && repo.stars >= 50")?;
//!
//! let repo = Repo { name: "git-tool", public: true, stars: 87 };
//! assert!(filter.matches(&repo)?);
//!
//! let repo = Repo { name: "top-secret", public: false, stars: 3 };
//! assert!(!filter.matches(&repo)?);
//! # Ok(())
//! # }
//! ```
//!
//! # Filter syntax
//!
//! A filter is a single logical expression which is evaluated against each
//! object, matching the object whenever the expression is
//! [truthy](FilterValue::is_truthy).
//!
//! ```text
//! repo.public && !repo.fork && repo.name in ["git-tool", "grey"]
//! ```
//!
//! ## Literals
//!
//! | Literal    | Example                | Notes                                            |
//! |------------|------------------------|--------------------------------------------------|
//! | Null       | `null`                 | Also returned for properties which aren't found. |
//! | Boolean    | `true`, `false`        |                                                  |
//! | Number     | `123`, `123.45`        | All numbers are 64-bit floats internally.        |
//! | String     | `"hello"`              | Escape embedded quotes with `\"`.                |
//! | Raw string | `r"^v\d+$"`            | No escape processing; cannot contain `"` (the `r#"..."#` form is not supported). |
//! | Tuple      | `["a", "b"]`           | A list of literal values.                        |
//! | Duration   | `5m`, `1h30m`, `500ms` | Requires the **`chrono`** crate feature.         |
//!
//! ## Properties
//!
//! Any other identifier (including `.` and `-` separated names like
//! `release.prerelease` or `asset.source-code`) is treated as a property
//! reference, and is resolved by calling [`Filterable::get`] on the target
//! object. Note that the operator keywords below (`in`, `contains`, `like`,
//! `matches`, etc.) are reserved and cannot be used as property names.
//!
//! ## Operators
//!
//! In order of increasing precedence:
//!
//! | Operator                 | Meaning                                                            |
//! |--------------------------|--------------------------------------------------------------------|
//! | `\|\|`                   | Logical OR (short-circuiting).                                     |
//! | `&&`                     | Logical AND (short-circuiting).                                    |
//! | `==`, `!=`               | Equality (strings are compared case-insensitively).                |
//! | `>`, `>=`, `<`, `<=`     | Ordering comparisons.                                              |
//! | `contains`               | String contains a substring, or tuple contains a value.            |
//! | `in`                     | Inverse of `contains` (i.e. `a in b` ≡ `b contains a`).            |
//! | `startswith`, `endswith` | String prefix/suffix tests (case-insensitive).                     |
//! | `like`                   | Case-insensitive glob match (`*` and `?` wildcards).               |
//! | `matches`                | Regular expression match (requires the **`regex`** crate feature). |
//! | `+`, `-`                 | Addition and subtraction (numbers, datetimes, and durations).      |
//! | `!`                      | Logical NOT (unary).                                               |
//! | `(...)`                  | Grouping.                                                          |
//!
//! ## Case sensitivity
//!
//! The string operators above compare case-insensitively, folding both
//! operands with the language's Unicode case-folding rules. Each of them
//! (except `matches`, where the pattern author controls casing with `(?i)`)
//! has a case-*sensitive* variant with a `_cs` suffix which compares strings
//! exactly as written: `contains_cs`, `in_cs`, `startswith_cs`,
//! `endswith_cs`, and `like_cs`. They sit at the same precedence as their
//! case-insensitive counterparts, and tuple membership through `contains_cs`
//! and `in_cs` compares the tuple's elements case-sensitively too.
//!
//! ```text
//! branch.name startswith_cs "Feat/" && "Alice" in_cs branch.reviewers
//! ```
//!
//! ## Pattern matching
//!
//! The `like` operator matches a string against a glob pattern. `*` matches
//! any sequence of characters (including none), `?` matches exactly one
//! character, and a backslash makes the following character literal (`\*`,
//! `\?`, `\\`); character classes like `[a-z]` are **not** supported. Like
//! the rest of the language, matching is case-insensitive: both the pattern
//! and the input are folded using the language's Unicode case-folding rules,
//! including multi-character folds (`"groß" like "*ss"` holds, and `?`
//! counts folded characters, so `ß` counts as two). The `like_cs` variant
//! matches case-sensitively instead, with no folding at all.
//!
//! ```
//! use filt_rs::{Filter, FilterValue, Filterable};
//!
//! struct Branch(&'static str);
//!
//! impl Filterable for Branch {
//!     fn get(&self, key: &str) -> FilterValue {
//!         match key {
//!             "branch.name" => self.0.into(),
//!             _ => FilterValue::Null,
//!         }
//!     }
//! }
//!
//! # fn main() -> Result<(), filt_rs::Error> {
//! let filter = Filter::new(r#"branch.name like "feat/*""#)?;
//! assert!(filter.matches(&Branch("feat/login"))?);
//! assert!(filter.matches(&Branch("FEAT/LOGIN"))?);
//! assert!(!filter.matches(&Branch("fix/typo"))?);
//! # Ok(())
//! # }
//! ```
//!
//! With the **`regex`** crate feature enabled, the `matches` operator tests a
//! string against a regular expression (as implemented by the
//! [regex](https://docs.rs/regex) crate). Raw strings (`r"..."`) are the most
//! convenient way to write these, since they perform no escape processing.
//! Unlike the rest of the language, regular expressions are case-sensitive as
//! written (use `(?i)` to ignore case) and unanchored (use `^` and `$` to
//! anchor the match).
//!
//! ```
//! # use filt_rs::{Filter, FilterValue, Filterable};
//! # struct Branch(&'static str);
//! # impl Filterable for Branch {
//! #     fn get(&self, key: &str) -> FilterValue {
//! #         match key {
//! #             "branch.name" => self.0.into(),
//! #             _ => FilterValue::Null,
//! #         }
//! #     }
//! # }
//! # fn main() -> Result<(), filt_rs::Error> {
//! # #[cfg(feature = "regex")]
//! # {
//! let filter = Filter::new(r#"branch.name matches r"^release/v\d+(\.\d+){2}$""#)?;
//! assert!(filter.matches(&Branch("release/v1.2.3"))?);
//! assert!(!filter.matches(&Branch("release/v1.2"))?);
//! # }
//! # Ok(())
//! # }
//! ```
//!
//! Both operators require their pattern to be a string literal: the pattern
//! is compiled once when the filter is parsed (with invalid regular
//! expressions reported as friendly [`Filter::new`] errors), and evaluation
//! performs no pattern-related heap allocation. Glob evaluation is fully
//! allocation-free, while regex evaluation is *amortized* allocation-free
//! (the regex engine lazily allocates per-thread scratch space on first use
//! and reuses it thereafter). Only string values can match a pattern: tuples
//! match when any of their string elements match, while `null`, booleans, and
//! numbers never match — even against `like "*"`.
//!
//! ## Arithmetic
//!
//! The `+` and `-` operators bind tighter than comparisons, so
//! `a + b > c` is read as `(a + b) > c`. Numbers may be added to and
//! subtracted from one another, while any unsupported combination of operand
//! types evaluates to `null` (consistent with the language's lenient
//! comparison semantics). There is no unary minus: write `0 - 5` to produce a
//! negative value.
//!
//! ```
//! # use filt_rs::Filter;
//! # struct Nothing;
//! # impl filt_rs::Filterable for Nothing {
//! #     fn get(&self, _key: &str) -> filt_rs::FilterValue { filt_rs::FilterValue::Null }
//! # }
//! # fn main() -> Result<(), filt_rs::Error> {
//! let filter = Filter::new("1 + 2 - 4 < 0")?;
//! assert!(filter.matches(&Nothing)?);
//! # Ok(())
//! # }
//! ```
//!
//! Note that a `-` *inside* a property name remains part of that name (so
//! `asset.source-code` is a single property), while a `-` which starts a new
//! token is the subtraction operator: `asset.size - 5` subtracts, but
//! `asset.size-5` references a property named `asset.size-5`.
//!
//! ## Functions
//!
//! Filters may call built-in functions using the familiar `name(args...)`
//! syntax. Function names and argument counts are validated when the filter
//! is parsed, so typos fail fast with a friendly error rather than at
//! evaluation time.
//!
//! | Function | Result                                                                            |
//! |----------|-----------------------------------------------------------------------------------|
//! | `now()`  | The current UTC time, evaluated at each [`Filter::matches`] call. Requires **`chrono`**. |
//!
//! ## Datetimes and durations
//!
//! With the **`chrono`** crate feature enabled, filters can work with points
//! in time and spans of time:
//!
//! - Duration literals are written as a number immediately followed by a
//!   unit — `ms` (milliseconds), `s` (seconds), `m` (minutes), `h` (hours),
//!   `d` (days), or `w` (weeks) — and may chain several segments together:
//!   `90s`, `5m`, `1h30m`, `500ms`.
//! - [`Filterable::get`] implementations can return
//!   [`FilterValue::DateTime`](FilterValue) values (e.g. from
//!   [`chrono::DateTime<Utc>`](https://docs.rs/chrono/latest/chrono/struct.DateTime.html)
//!   or [`std::time::SystemTime`]).
//! - Datetimes and durations support ordering comparisons against values of
//!   the same type, and arithmetic via `+` and `-`:
//!   `DateTime ± Duration → DateTime`, `DateTime - DateTime → Duration`, and
//!   `Duration ± Duration → Duration`.
//! - Datetimes are always truthy, while durations are truthy if (and only
//!   if) they are non-zero.
//!
//! This makes relative-time filters pleasantly concise:
//!
//! ```text
//! event.timestamp > now() - 5m
//! ```
//!
//! Without the `chrono` feature, duration literals and `now()` are still
//! recognised by the parser but produce a friendly error explaining that the
//! feature must be enabled.
//!
//! # Crate features
//!
//! - **`regex`** — enables the `matches` regular expression operator (adds a
//!   dependency on the [regex](https://docs.rs/regex) crate). Without this
//!   feature, filters using `matches` fail to parse with an error explaining
//!   how to enable it.
//! - **`chrono`** — adds datetime and duration support: the
//!   [`FilterValue::DateTime`](FilterValue) and
//!   [`FilterValue::Duration`](FilterValue) variants, duration literals such
//!   as `5m` and `1h30m`, the `now()` function, and temporal arithmetic and
//!   comparisons (see [Datetimes and durations](#datetimes-and-durations)).
//!
//! - **`secrecy`** — adds a `FilterValue::Secret` variant backed by the
//!   [`secrecy`](https://docs.rs/secrecy) crate's `SecretString`. Secret values
//!   behave exactly like strings in every comparison operation, but are always
//!   formatted as `[REDACTED]`, making it impossible to leak them through
//!   logging. See `FilterValue::secret` for details.
//!
//!   ```
//!   # #[cfg(feature = "secrecy")] {
//!   use filt_rs::{Filter, FilterValue, Filterable};
//!
//!   struct Credentials {
//!       password: secrecy::SecretString,
//!   }
//!
//!   impl Filterable for Credentials {
//!       fn get(&self, key: &str) -> FilterValue {
//!           match key {
//!               "password" => self.password.clone().into(),
//!               _ => FilterValue::Null,
//!           }
//!       }
//!   }
//!
//!   let creds = Credentials { password: "hunter2".into() };
//!
//!   // Secrets compare exactly like strings within filter expressions...
//!   let filter = Filter::new(r#"password == "Hunter2""#).unwrap();
//!   assert!(filter.matches(&creds).unwrap());
//!
//!   // ...but they are always redacted when formatted.
//!   assert_eq!(creds.get("password").to_string(), "[REDACTED]");
//!   # }
//!   ```
//!
//! - **`serde`** — implements [`serde::Deserialize`] for [`Filter`], allowing
//!   filters to be parsed directly out of configuration files (a missing or
//!   `null` value deserializes to the match-everything `true` filter).
//!
//! [`serde::Deserialize`]: https://docs.rs/serde/latest/serde/trait.Deserialize.html

#![warn(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/SierraSoftworks/filters/main/assets/icon.svg",
    html_favicon_url = "https://raw.githubusercontent.com/SierraSoftworks/filters/main/assets/icon.svg"
)]

mod case_sensitivity;
mod expr;
mod interpreter;
mod lexer;
mod location;
mod parser;
mod pattern;
mod token;
mod value;

use std::{fmt::Display, pin::Pin, ptr::NonNull};

use expr::{Expr, ExprVisitor};
use interpreter::FilterContext;

pub use human_errors::Error;
pub use value::{FilterValue, Filterable};

/// A parsed filter expression which can be evaluated against [`Filterable`] objects.
///
/// A `Filter` is constructed from a textual filter expression using
/// [`Filter::new`], which tokenizes and parses the expression up-front so that
/// it can be cheaply evaluated against any number of objects using
/// [`Filter::matches`].
///
/// ```
/// use filt_rs::{Filter, FilterValue, Filterable};
///
/// struct Server {
///     hostname: &'static str,
///     port: u16,
/// }
///
/// impl Filterable for Server {
///     fn get(&self, key: &str) -> FilterValue {
///         match key {
///             "hostname" => self.hostname.into(),
///             "port" => self.port.into(),
///             _ => FilterValue::Null,
///         }
///     }
/// }
///
/// # fn main() -> Result<(), filt_rs::Error> {
/// let filter = Filter::new(r#"hostname startswith "web" && port == 443"#)?;
///
/// assert!(filter.matches(&Server { hostname: "web-01", port: 443 })?);
/// assert!(!filter.matches(&Server { hostname: "db-01", port: 5432 })?);
/// # Ok(())
/// # }
/// ```
///
/// The default filter is the expression `true`, which matches every object:
///
/// ```
/// # use filt_rs::{Filter, FilterValue, Filterable};
/// # struct Anything;
/// # impl Filterable for Anything {
/// #     fn get(&self, _key: &str) -> FilterValue { FilterValue::Null }
/// # }
/// let filter = Filter::default();
/// assert_eq!(filter.raw(), "true");
/// assert!(filter.matches(&Anything).unwrap());
/// ```
pub struct Filter {
    #[allow(clippy::box_collection)]
    filter: Pin<Box<String>>,
    ast: Expr<'static>,
}

impl Filter {
    /// Parses the provided filter expression, returning a reusable `Filter`.
    ///
    /// The expression is tokenized and parsed eagerly, so any syntax errors
    /// are reported here rather than at evaluation time. Errors include the
    /// location of the problem and guidance on how to correct it.
    ///
    /// ```
    /// use filt_rs::Filter;
    ///
    /// let filter = Filter::new("size > 100 && !archived").unwrap();
    /// assert_eq!(filter.raw(), "size > 100 && !archived");
    ///
    /// let error = Filter::new("size >").unwrap_err();
    /// assert!(error.to_string().contains("end of your filter expression"));
    /// ```
    pub fn new<S: Into<String>>(filter: S) -> Result<Self, Error> {
        // The AST borrows string slices from the filter expression itself. Pinning
        // the boxed string keeps those borrows valid for the lifetime of this
        // struct without re-allocating the lexemes.
        let filter = Box::new(filter.into());
        let filter_ptr = NonNull::from(&filter);
        let pinned = Box::into_pin(filter);

        let tokens = lexer::Scanner::new(unsafe { filter_ptr.as_ref() });
        let ast = parser::Parser::parse(tokens.into_iter())?;
        Ok(Self {
            filter: pinned,
            ast,
        })
    }

    /// Evaluates this filter against the provided object, returning whether it matched.
    ///
    /// The object's properties are resolved through its [`Filterable::get`]
    /// implementation, and the filter matches when the expression evaluates to
    /// a [truthy](FilterValue::is_truthy) value.
    ///
    /// ```
    /// use filt_rs::{Filter, FilterValue, Filterable};
    ///
    /// struct Message(&'static str);
    ///
    /// impl Filterable for Message {
    ///     fn get(&self, key: &str) -> FilterValue {
    ///         match key {
    ///             "subject" => self.0.into(),
    ///             _ => FilterValue::Null,
    ///         }
    ///     }
    /// }
    ///
    /// # fn main() -> Result<(), filt_rs::Error> {
    /// let filter = Filter::new(r#"subject contains "invoice""#)?;
    /// assert!(filter.matches(&Message("Invoice #123"))?);
    /// assert!(!filter.matches(&Message("Weekly newsletter"))?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn matches<T: Filterable>(&self, target: &T) -> Result<bool, Error> {
        Ok(FilterContext::new(target).visit_expr(&self.ast).is_truthy())
    }

    /// Gets the raw filter expression which was used to construct this filter.
    ///
    /// ```
    /// use filt_rs::Filter;
    ///
    /// let filter = Filter::new("name == \"demo\"").unwrap();
    /// assert_eq!(filter.raw(), "name == \"demo\"");
    /// ```
    pub fn raw(&self) -> &str {
        &self.filter
    }
}

impl Default for Filter {
    /// Returns the match-everything filter `true`.
    fn default() -> Self {
        Self {
            filter: Box::pin("true".to_string()),
            ast: Expr::Literal(FilterValue::Bool(true)),
        }
    }
}

impl std::fmt::Debug for Filter {
    /// Formats the filter as its parsed expression tree, which can be useful
    /// when debugging operator precedence issues.
    ///
    /// ```
    /// use filt_rs::Filter;
    ///
    /// let filter = Filter::new("a || b && c").unwrap();
    /// assert_eq!(format!("{filter:?}"), "(|| (property a) (&& (property b) (property c)))");
    /// ```
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.ast)
    }
}

impl Display for Filter {
    /// Formats the filter as its original raw expression.
    ///
    /// ```
    /// use filt_rs::Filter;
    ///
    /// let filter = Filter::new("a || b").unwrap();
    /// assert_eq!(filter.to_string(), "a || b");
    /// ```
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.raw())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Filter {
    /// Deserializes a `Filter` from a string containing a filter expression.
    ///
    /// Missing or `null` values are deserialized as the match-everything
    /// filter `true`, making it easy to use optional filter fields within
    /// your configuration structures.
    ///
    /// ```
    /// use filt_rs::Filter;
    ///
    /// #[derive(serde::Deserialize)]
    /// struct Config {
    ///     #[serde(default)]
    ///     filter: Filter,
    /// }
    ///
    /// let config: Config = serde_json::from_str(r#"{"filter": "!repo.fork"}"#).unwrap();
    /// assert_eq!(config.filter.raw(), "!repo.fork");
    ///
    /// let config: Config = serde_json::from_str("{}").unwrap();
    /// assert_eq!(config.filter.raw(), "true");
    /// ```
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct FilterVisitor;

        impl<'de> serde::de::Visitor<'de> for FilterVisitor {
            type Value = Filter;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a valid filter expression")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Filter::new(v).map_err(serde::de::Error::custom)
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Filter::new("true").map_err(serde::de::Error::custom)
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                deserializer.deserialize_str(self)
            }
        }

        deserializer.deserialize_option(FilterVisitor)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    struct TestObject {
        name: String,
        age: i32,
        alive: bool,
        tags: Vec<&'static str>,
    }

    impl Default for TestObject {
        fn default() -> Self {
            Self {
                name: "John Doe".to_string(),
                age: 30,
                alive: true,
                tags: vec!["red", "black"],
            }
        }
    }

    impl Filterable for TestObject {
        fn get(&self, property: &str) -> FilterValue {
            match property {
                "name" => self.name.clone().into(),
                "age" => self.age.into(),
                "alive" => self.alive.into(),
                "tags" => self
                    .tags
                    .iter()
                    .cloned()
                    .map(|v| v.into())
                    .collect::<Vec<FilterValue>>()
                    .into(),
                _ => FilterValue::Null,
            }
        }
    }

    #[rstest]
    #[case("name == \"John Doe\"", true)]
    #[case("name != \"John Doe\"", false)]
    #[case("name == \"Jane Doe\"", false)]
    #[case("name != \"Jane Doe\"", true)]
    #[case("name startswith \"John\"", true)]
    #[case("name startswith \"Jane\"", false)]
    #[case("name endswith \"Doe\"", true)]
    #[case("name endswith \"Smith\"", false)]
    #[case("age == 30", true)]
    #[case("age != 30", false)]
    #[case("age == 31", false)]
    #[case("age != 31", true)]
    #[case("age > 31", false)]
    #[case("age < 31", true)]
    #[case("age >= 30", true)]
    #[case("age <= 30", true)]
    #[case("tags == [\"red\",\"black\"]", true)]
    #[case("tags != [\"red\",\"black\"]", false)]
    #[case("tags == [\"blue\"]", false)]
    #[case("tags contains \"red\"", true)]
    #[case("tags contains \"blue\"", false)]
    #[case("\"red\" in tags", true)]
    #[case("\"blue\" in tags", false)]
    fn case_sensitive_filtering(#[case] filter: &str, #[case] matches: bool) {
        let obj = TestObject::default();

        assert_eq!(
            Filter::new(filter)
                .expect("parse filter")
                .matches(&obj)
                .expect("run filter"),
            matches
        );
    }

    #[rstest]
    #[case("name == \"john doe\"", true)]
    #[case("name != \"john doe\"", false)]
    #[case("name == \"jane doe\"", false)]
    #[case("name != \"jane doe\"", true)]
    #[case("name startswith \"john\"", true)]
    #[case("name startswith \"jane\"", false)]
    #[case("name endswith \"doe\"", true)]
    #[case("name endswith \"smith\"", false)]
    #[case("\"RED\" in tags", true)]
    #[case("\"BLUE\" in tags", false)]
    fn case_insensitive_filtering(#[case] filter: &str, #[case] matches: bool) {
        let obj = TestObject::default();

        assert_eq!(
            Filter::new(filter)
                .expect("parse filter")
                .matches(&obj)
                .expect("run filter"),
            matches
        );
    }

    #[rstest]
    #[case("name == \"John Doe\" && age == 30", true)]
    #[case("name == \"John Doe\" && age == 31", false)]
    #[case("name == \"Jane Doe\" && age == 30", false)]
    #[case("name == \"John Doe\" || age == 30", true)]
    #[case("name == \"John Doe\" || age == 31", true)]
    #[case("name == \"Jane Doe\" || age == 30", true)]
    #[case("name == \"Jane Doe\" || age == 31", false)]
    fn binary_operator_filtering(#[case] filter: &str, #[case] matches: bool) {
        let obj = TestObject::default();

        assert_eq!(
            Filter::new(filter)
                .expect("parse filter")
                .matches(&obj)
                .expect("run filter"),
            matches
        );
    }

    #[rstest]
    #[case("alive", true)]
    #[case("!alive", false)]
    #[case("name && age", true)]
    #[case("name && !age", false)]
    fn logical_operator_filtering(#[case] filter: &str, #[case] matches: bool) {
        let obj = TestObject::default();

        assert_eq!(
            Filter::new(filter)
                .expect("parse filter")
                .matches(&obj)
                .expect("run filter"),
            matches
        );
    }

    #[test]
    fn default_filter_matches_everything() {
        let filter = Filter::default();
        assert_eq!(filter.raw(), "true");
        assert!(filter.matches(&TestObject::default()).expect("run filter"));
    }

    #[test]
    fn display_round_trips_the_raw_expression() {
        let filter = Filter::new("age >= 30 && alive").expect("parse filter");
        assert_eq!(filter.to_string(), "age >= 30 && alive");
        assert_eq!(filter.raw(), "age >= 30 && alive");
    }

    #[rstest]
    #[case("age >")]
    #[case("(alive")]
    #[case("name = \"John\"")]
    #[case("\"unterminated")]
    fn invalid_filters_report_errors(#[case] filter: &str) {
        assert!(Filter::new(filter).is_err());
    }

    #[cfg(feature = "serde")]
    mod serde_tests {
        use super::*;

        #[derive(serde::Deserialize)]
        struct Config {
            #[serde(default)]
            filter: Filter,
        }

        #[test]
        fn deserializes_a_filter_expression() {
            let config: Config =
                serde_json::from_str(r#"{"filter": "age > 21 && alive"}"#).expect("deserialize");
            assert_eq!(config.filter.raw(), "age > 21 && alive");
            assert!(
                config
                    .filter
                    .matches(&TestObject::default())
                    .expect("run filter")
            );
        }

        #[test]
        fn missing_filters_match_everything() {
            let config: Config = serde_json::from_str("{}").expect("deserialize");
            assert_eq!(config.filter.raw(), "true");
        }

        #[test]
        fn null_filters_match_everything() {
            let config: Config = serde_json::from_str(r#"{"filter": null}"#).expect("deserialize");
            assert_eq!(config.filter.raw(), "true");
        }

        #[test]
        fn invalid_filters_fail_to_deserialize() {
            let result: Result<Config, _> = serde_json::from_str(r#"{"filter": "age >"}"#);
            assert!(result.is_err());
        }
    }
}
