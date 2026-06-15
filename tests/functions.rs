//! Integration tests for the user-extensible function API: the [`Function`]
//! trait and [`Filter::with_functions`].

use std::borrow::Cow;
use std::sync::Arc;

use filt_rs::{Filter, FilterValue, Filterable, Function};

/// An `upper(string)` function which upper-cases its string argument. Used to
/// prove a custom function actually runs (via the case-*sensitive* operators,
/// since the language's `==`/`contains` ignore case).
struct Upper;

impl Function for Upper {
    fn name(&self) -> &str {
        "upper"
    }

    fn arity(&self) -> usize {
        1
    }

    fn call<'a>(&self, args: &[Cow<'a, FilterValue<'a>>]) -> Cow<'a, FilterValue<'a>> {
        match args[0].as_ref() {
            FilterValue::String(s) => Cow::Owned(FilterValue::String(s.to_uppercase().into())),
            _ => Cow::Owned(FilterValue::Null),
        }
    }
}

struct Repo {
    name: &'static str,
}

impl Filterable for Repo {
    fn get(&self, key: &str) -> FilterValue<'_> {
        match key {
            "repo.name" => self.name.into(),
            _ => FilterValue::Null,
        }
    }
}

fn custom_functions() -> Vec<Arc<dyn Function>> {
    vec![Arc::new(Upper)]
}

#[test]
fn custom_functions_can_be_called() {
    let filter =
        Filter::with_functions(r#"upper(repo.name) contains_cs "GIT""#, custom_functions())
            .expect("parse filter");

    // upper("git-tool") == "GIT-TOOL", which case-sensitively contains "GIT"...
    assert!(filter.matches(&Repo { name: "git-tool" }).unwrap());

    // ...whereas the raw (lower-case) property does not.
    let raw = Filter::new(r#"repo.name contains_cs "GIT""#).expect("parse filter");
    assert!(!raw.matches(&Repo { name: "git-tool" }).unwrap());
}

#[test]
fn base_functions_remain_available_alongside_custom_ones() {
    let filter = Filter::with_functions(r#"trim(repo.name) == "trim me""#, custom_functions())
        .expect("parse filter");
    assert!(
        filter
            .matches(&Repo {
                name: "  trim me  "
            })
            .unwrap()
    );
}

#[test]
fn custom_and_base_functions_compose() {
    // Nested calls: trim() of the upper() result.
    let filter = Filter::with_functions(
        r#"trim(upper(repo.name)) contains_cs "GIT""#,
        custom_functions(),
    )
    .expect("parse filter");
    assert!(
        filter
            .matches(&Repo {
                name: "  git-tool  "
            })
            .unwrap()
    );
}

#[test]
fn custom_function_arity_is_validated_at_parse_time() {
    let error =
        Filter::with_functions("upper()", custom_functions()).expect_err("the filter should fail");
    assert!(
        error
            .to_string()
            .contains("expects 1 argument, but your filter provided 0"),
        "unexpected error: {error}"
    );
}

#[test]
fn unknown_functions_list_the_available_functions() {
    let error =
        Filter::with_functions("nope()", custom_functions()).expect_err("the filter should fail");
    let message = error.to_string();
    assert!(
        message.contains("unknown function 'nope()'"),
        "unexpected error: {message}"
    );
    // Both the base set and the registered custom function are listed.
    assert!(message.contains("trim()"), "unexpected error: {message}");
    assert!(message.contains("upper()"), "unexpected error: {message}");
}

#[test]
fn custom_functions_are_unavailable_without_registration() {
    // `upper` only exists once registered via `with_functions`; a plain filter
    // rejects it as unknown.
    assert!(Filter::new("upper(repo.name)").is_err());
}

#[test]
fn cloning_preserves_custom_functions() {
    let filter =
        Filter::with_functions(r#"upper(repo.name) contains_cs "GIT""#, custom_functions())
            .expect("parse filter");

    // Cloning re-parses against the stored function set, so the clone still
    // resolves `upper` and behaves identically.
    let clone = filter.clone();
    assert_eq!(clone, filter);
    assert!(clone.matches(&Repo { name: "git-tool" }).unwrap());
}
