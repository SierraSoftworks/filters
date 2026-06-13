//! Benchmarks for filter parsing and evaluation.
//!
//! Run with `cargo bench`. The sample windows are kept deliberately short
//! (1s warm-up, 2s measurement) so that the full suite completes quickly;
//! increase them locally if you need tighter confidence intervals.

use std::hint::black_box;
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use filt_rs::{Filter, FilterValue, Filterable};

/// A fixture resembling the repository metadata which this crate was
/// originally designed to filter (see the `github-backup` project).
struct Repo {
    name: &'static str,
    full_name: &'static str,
    language: &'static str,
    stars: u32,
    forks: u32,
    archived: bool,
    fork: bool,
    public: bool,
    topics: Vec<&'static str>,
}

impl Default for Repo {
    fn default() -> Self {
        Self {
            name: "git-tool",
            full_name: "SierraSoftworks/git-tool",
            language: "Rust",
            stars: 320,
            forks: 12,
            archived: false,
            fork: false,
            public: true,
            topics: vec!["git", "productivity", "cli"],
        }
    }
}

impl Filterable for Repo {
    fn get(&self, key: &str) -> FilterValue<'_> {
        match key {
            "repo.name" => self.name.into(),
            "repo.full_name" => self.full_name.into(),
            "repo.language" => self.language.into(),
            "repo.stars" => self.stars.into(),
            "repo.forks" => self.forks.into(),
            "repo.archived" => self.archived.into(),
            "repo.fork" => self.fork.into(),
            "repo.public" => self.public.into(),
            "repo.topics" => self
                .topics
                .iter()
                .map(|&t| t.into())
                .collect::<Vec<FilterValue<'_>>>()
                .into(),
            _ => FilterValue::Null,
        }
    }
}

/// The same fixture as [`Repo`], but its `Filterable::get` returns *owned*
/// strings (allocating a fresh `String` for every string property it resolves).
/// This models a client whose data isn't already available as `&str` and lets
/// us measure exactly what the borrowing `FilterValue` saves over the previous
/// always-allocating behaviour.
struct OwnedRepo {
    name: String,
    full_name: String,
    language: String,
    stars: u32,
    forks: u32,
    archived: bool,
    fork: bool,
    public: bool,
    topics: Vec<String>,
}

impl Default for OwnedRepo {
    fn default() -> Self {
        let borrowed = Repo::default();
        Self {
            name: borrowed.name.to_string(),
            full_name: borrowed.full_name.to_string(),
            language: borrowed.language.to_string(),
            stars: borrowed.stars,
            forks: borrowed.forks,
            archived: borrowed.archived,
            fork: borrowed.fork,
            public: borrowed.public,
            topics: borrowed.topics.iter().map(|t| t.to_string()).collect(),
        }
    }
}

impl Filterable for OwnedRepo {
    fn get(&self, key: &str) -> FilterValue<'_> {
        match key {
            // `.clone()` forces a heap allocation for every string property,
            // exactly as `Filterable::get` used to before strings could borrow.
            "repo.name" => self.name.clone().into(),
            "repo.full_name" => self.full_name.clone().into(),
            "repo.language" => self.language.clone().into(),
            "repo.stars" => self.stars.into(),
            "repo.forks" => self.forks.into(),
            "repo.archived" => self.archived.into(),
            "repo.fork" => self.fork.into(),
            "repo.public" => self.public.into(),
            "repo.topics" => self
                .topics
                .iter()
                .map(|t| t.clone().into())
                .collect::<Vec<FilterValue<'_>>>()
                .into(),
            _ => FilterValue::Null,
        }
    }
}

const SIMPLE_FILTER: &str = "repo.stars >= 50";
const COMPLEX_FILTER: &str = r#"repo.public && !repo.archived && (repo.language in ["rust", "go", "typescript"] || repo.stars >= 500) && repo.name startswith "git" && !(repo.topics contains "deprecated")"#;

fn bench_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse");
    group.measurement_time(Duration::from_secs(5));

    group.bench_function("simple", |b| {
        b.iter(|| Filter::new(black_box(SIMPLE_FILTER)).unwrap())
    });

    group.bench_function("complex", |b| {
        b.iter(|| Filter::new(black_box(COMPLEX_FILTER)).unwrap())
    });

    group.finish();
}

fn bench_evaluation(c: &mut Criterion) {
    let mut group = c.benchmark_group("eval");
    group.measurement_time(Duration::from_secs(5));

    let repo = Repo::default();

    let cases: &[(&str, &str, bool)] = &[
        // The interpreter overhead floor: no properties, no strings.
        ("literal_only", "true", true),
        // Numeric and boolean property comparisons (non-allocating property values).
        (
            "numeric_boolean",
            "repo.stars >= 50 && !repo.archived && repo.forks < 100",
            true,
        ),
        // String-heavy: equality, contains, startswith, and endswith.
        (
            "string_heavy",
            r#"repo.name startswith "git" && repo.full_name contains "sierra" && repo.language == "RUST" && !(repo.name endswith "-old")"#,
            true,
        ),
        // Tuple membership against a literal tuple.
        (
            "tuple_membership",
            r#"repo.language in ["rust", "go", "typescript"]"#,
            true,
        ),
        // Membership within a tuple-valued property.
        ("tuple_property", r#"repo.topics contains "cli""#, true),
        // A realistic compound filter combining all of the above.
        ("compound_realistic", COMPLEX_FILTER, true),
    ];

    for &(name, expression, expected) in cases {
        let filter = Filter::new(expression).unwrap();
        assert_eq!(
            filter.matches(&repo).unwrap(),
            expected,
            "the '{name}' benchmark filter should evaluate to {expected}"
        );

        group.bench_function(name, |b| {
            b.iter(|| filter.matches(black_box(&repo)).unwrap())
        });
    }

    group.finish();
}

/// Directly compares evaluating the same string-heavy filters against a fixture
/// that *borrows* its string data versus one that returns *owned* strings. The
/// gap between the two pairs is the allocation cost that lifetime-bounded
/// `FilterValue`s eliminate for implementors who can hand out borrows.
fn bench_borrowed_vs_owned(c: &mut Criterion) {
    let mut group = c.benchmark_group("eval_borrowed_vs_owned");
    group.measurement_time(Duration::from_secs(5));

    let borrowed = Repo::default();
    let owned = OwnedRepo::default();

    let cases: &[(&str, &str)] = &[
        (
            "string_heavy",
            r#"repo.name startswith "git" && repo.full_name contains "sierra" && repo.language == "RUST" && !(repo.name endswith "-old")"#,
        ),
        ("compound_realistic", COMPLEX_FILTER),
    ];

    for &(name, expression) in cases {
        let filter = Filter::new(expression).unwrap();
        assert_eq!(
            filter.matches(&borrowed).unwrap(),
            filter.matches(&owned).unwrap()
        );

        group.bench_function(format!("{name}/borrowed"), |b| {
            b.iter(|| filter.matches(black_box(&borrowed)).unwrap())
        });
        group.bench_function(format!("{name}/owned"), |b| {
            b.iter(|| filter.matches(black_box(&owned)).unwrap())
        });
    }

    group.finish();
}

fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(2))
}

criterion_group! {
    name = benches;
    config = config();
    targets = bench_parsing, bench_evaluation, bench_borrowed_vs_owned
}
criterion_main!(benches);
