<div align="center">
  <img src="assets/logo.svg" alt="filt-rs" width="440">

  <p><strong>A human-friendly filter expression language for matching your objects against user-provided queries.</strong></p>

  <p>
    <a href="https://github.com/SierraSoftworks/filters/actions/workflows/ci.yml"><img src="https://github.com/SierraSoftworks/filters/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
    <a href="https://crates.io/crates/filt-rs"><img src="https://img.shields.io/crates/v/filt-rs.svg" alt="crates.io"></a>
    <a href="https://docs.rs/filt-rs"><img src="https://img.shields.io/docsrs/filt-rs" alt="docs.rs"></a>
    <a href="LICENSE"><img src="https://img.shields.io/github/license/SierraSoftworks/filters" alt="MIT License"></a>
  </p>
</div>

---

`filt-rs` gives your users a small, safe, and friendly expression language for
describing *which* of your objects a tool should operate on — which repositories
to back up, which emails to restore, which releases to download. You implement
a single-method trait to expose your object's properties, and your users write
filters like this:

```text
repo.public && !repo.fork && repo.name in ["git-tool", "grey"]
```

This crate was extracted from the Sierra Softworks
[github-backup](https://github.com/SierraSoftworks/github-backup) and
[mail-backup](https://github.com/SierraSoftworks/mail-backup) projects, where
it powers their backup policy filtering.

## Features

- **Friendly syntax** — reads like plain English, with `&&`/`||`/`!`,
  comparisons, and string operators like `contains`, `startswith`, and `in`.
- **Pattern matching** — glob-style matching with `like` (built in, zero
  allocations at evaluation time) and full regular expressions with `matches`
  (behind the optional `regex` feature).
- **Helpful errors** — parse and evaluation errors include the exact line and
  column of the problem along with advice on how to fix it (powered by
  [human-errors](https://crates.io/crates/human-errors)).
- **Parse once, evaluate cheaply** — filters are compiled to an AST up front
  and can then be evaluated against any number of objects.
- **Bring your own objects** — implement the single-method `Filterable` trait;
  no derives, reflection, or serialization required.
- **Lightweight** — a single small dependency, no async, no unsafe API surface.
- **Optional datetime support** — filter on timestamps with relative-time
  expressions like `event.timestamp > now() - 5m` using the `chrono` feature.
- **Optional serde support** — deserialize filters directly out of your
  configuration files with the `serde` feature.
- **Optional secret values** — compare passwords and tokens in filters without
  ever being able to print them, with the `secrecy` feature.
- **Optional visitor API** — walk and transform the parsed expression tree with
  your own `ExprVisitor` via `Filter::visit`, behind the `visitor` feature.

## Usage

```shell
cargo add filt-rs
```

Implement `Filterable` for your type, then parse and evaluate filters:

```rust
use filt_rs::{Filter, FilterValue, Filterable};

struct Repo {
    name: &'static str,
    public: bool,
    stars: u32,
}

impl Filterable for Repo {
    fn get(&self, key: &str) -> FilterValue<'_> {
        match key {
            "repo.name" => self.name.into(),
            "repo.public" => self.public.into(),
            "repo.stars" => self.stars.into(),
            _ => FilterValue::Null,
        }
    }
}

fn main() -> Result<(), filt_rs::Error> {
    let filter = Filter::new("repo.public && repo.stars >= 50")?;

    let repo = Repo { name: "git-tool", public: true, stars: 87 };
    assert!(filter.matches(&repo)?);

    let repo = Repo { name: "top-secret", public: false, stars: 3 };
    assert!(!filter.matches(&repo)?);

    Ok(())
}
```

## Filter syntax

A filter is a single logical expression which is evaluated against each object,
matching whenever the expression evaluates to a truthy value (`null`, `false`,
`0`, `""`, and `[]` are falsy; everything else is truthy).

### Literals

| Literal    | Example                | Notes                                            |
| ---------- | ---------------------- | ------------------------------------------------ |
| Null       | `null`                 | Also returned for properties which aren't found. |
| Boolean    | `true`, `false`        |                                                  |
| Number     | `123`, `123.45`        | All numbers are 64-bit floats internally.        |
| String     | `"hello"`              | Escape embedded quotes with `\"`.                |
| Raw string | `r"^v\d+$"`            | No escape processing; cannot contain `"`.        |
| Tuple      | `["a", "b"]`           | A list of literal values.                        |
| Duration   | `5m`, `1h30m`, `500ms` | Requires the `chrono` feature.                   |

### Properties

Any other identifier — including `.` and `-` separated names like
`release.prerelease` or `asset.source-code` — is treated as a property
reference and resolved by calling `Filterable::get` on the target object.
Operator keywords (`in`, `contains`, `startswith`, `endswith`, `like`,
`matches`, and their `_cs` variants) are reserved and cannot be used as
property names.

### Operators

In order of increasing precedence:

| Operator                 | Meaning                                                       |
| ------------------------ | ------------------------------------------------------------- |
| `\|\|`                   | Logical OR (short-circuiting).                                |
| `&&`                     | Logical AND (short-circuiting).                               |
| `==`, `!=`               | Equality (strings are compared case-insensitively).           |
| `>`, `>=`, `<`, `<=`     | Ordering comparisons.                                         |
| `contains`               | String contains a substring, or tuple contains a value.       |
| `in`                     | Inverse of `contains` (`a in b` ≡ `b contains a`).            |
| `startswith`, `endswith` | String prefix/suffix tests (case-insensitive).                |
| `like`                   | Case-insensitive glob match (`*` and `?` wildcards).          |
| `matches`                | Regular expression match (requires the `regex` feature).      |
| `+`, `-`                 | Addition and subtraction (numbers, datetimes, and durations). |
| `!`                      | Logical NOT (unary).                                          |
| `(...)`                  | Grouping.                                                     |

Arithmetic binds tighter than comparisons (so `a + b > c` reads as
`(a + b) > c`), and unsupported operand combinations evaluate to `null` rather
than failing. There is no unary minus — write `0 - 5` for a negative value —
and a `-` *inside* a property name remains part of that name, so
`asset.source-code` keeps working as a single property.

### Functions

Filters may call built-in functions using the familiar `name(args...)` syntax.
Unknown function names and incorrect argument counts are rejected when the
filter is parsed, with an error listing the supported functions.

| Function | Result                                                                   |
| -------- | ------------------------------------------------------------------------ |
| `now()`  | The current UTC time, evaluated afresh on every `Filter::matches` call. Requires the `chrono` feature. |

### Case sensitivity

The string operators compare case-insensitively by default, folding both
operands with the language's Unicode case-folding rules. Each of them (except
`matches`, where the pattern author controls casing with `(?i)`) has a
case-sensitive variant with a `_cs` suffix which compares strings exactly as
written: `contains_cs`, `in_cs`, `startswith_cs`, `endswith_cs`, and
`like_cs`. Tuple membership through `contains_cs` and `in_cs` compares the
tuple's elements case-sensitively too.

```text
branch.name startswith_cs "Feat/" && "Alice" in_cs branch.reviewers
```

### Pattern matching

The `like` operator matches a string against a glob pattern, where `*` matches
any sequence of characters (including none), `?` matches exactly one
character, and a backslash makes the following character literal (`\*`, `\?`,
`\\`). Character classes like `[a-z]` are not supported. As with the rest of
the language, matching is case-insensitive, using the same Unicode
case-folding rules as `==`, `contains`, `startswith`, and `endswith` —
including multi-character folds, so `"groß" like "*ss"` holds (note that `?`
counts folded characters, so `ß` counts as two):

```text
branch.name like "feat/*"
repo.name like "*-backup"
version like "v?.?.?"
```

With the optional `regex` feature enabled, the `matches` operator tests a
string against a regular expression (powered by the
[regex](https://docs.rs/regex) crate). Raw strings (`r"..."`) avoid having to
escape backslashes. Unlike the rest of the language, regular expressions are
case-sensitive as written (use `(?i)` to ignore case) and unanchored (use `^`
and `$` to anchor the match):

```text
branch.name matches r"^release/v\d+(\.\d+){2}$"
commit.message matches "(?i)breaking change"
```

Both operators require their pattern to be a string literal: patterns are
compiled once when the filter is parsed (invalid regular expressions are
reported as friendly parse errors) and evaluation performs no
pattern-related heap allocation. Only string values can match a pattern;
tuples match when any of their string elements match, while `null`, booleans,
and numbers never match.

```shell
cargo add filt-rs --features regex
```

### Examples

```text
!repo.fork && repo.name contains "awesome"
!release.prerelease && !asset.source-code
size > 1024 && (archived || disabled)
"backup" in tags
branch.name like "feat/*"
branch.name matches r"^release/v\d+(\.\d+){2}$"
event.timestamp > now() - 5m
```

## Datetime support

Enable the `chrono` feature to filter on timestamps with relative-time
expressions:

```shell
cargo add filt-rs --features chrono
```

Expose a `FilterValue::DateTime` from your `Filterable` implementation (any
`chrono::DateTime<Utc>` or `std::time::SystemTime` converts with `.into()`),
and your users can write filters like `event.timestamp > now() - 5m`:

```rust,ignore
use filt_rs::{Filter, FilterValue, Filterable};

struct Event {
    timestamp: chrono::DateTime<chrono::Utc>,
}

impl Filterable for Event {
    fn get(&self, key: &str) -> FilterValue<'_> {
        match key {
            "event.timestamp" => self.timestamp.into(),
            _ => FilterValue::Null,
        }
    }
}

let filter = Filter::new("event.timestamp > now() - 5m")?;
assert!(filter.matches(&Event { timestamp: chrono::Utc::now() })?);
```

Durations are written as a number immediately followed by a unit — `ms`, `s`,
`m` (minutes), `h`, `d`, or `w` — and several segments can be chained together
(`1h30m`). Datetimes and durations compare against values of the same type and
support `+`/`-` arithmetic: `DateTime ± Duration → DateTime`,
`DateTime - DateTime → Duration`, and `Duration ± Duration → Duration`.

## Serde support

Enable the `serde` feature to deserialize filters directly from your
configuration files:

```shell
cargo add filt-rs --features serde
```

```rust,ignore
#[derive(serde::Deserialize)]
struct BackupPolicy {
    kind: String,
    from: String,
    #[serde(default)]
    filter: filt_rs::Filter,
}
```

Missing or `null` filter fields deserialize to the match-everything filter
`true`, so optional filters work out of the box.

## Inspecting filters

Enable the `visitor` feature to expose the parsed expression tree and walk it
with your own visitor — useful for validating which properties a filter
references, estimating its cost, or translating it into another query language:

```shell
cargo add filt-rs --features visitor
```

Implement the `ExprVisitor` trait and pass it to `Filter::visit`, which returns
whatever your visitor produces. Each `visit_*` method is handed the relevant
child nodes (and, for the binary/logical/unary cases, a `BinaryOperator`,
`LogicalOperator`, or `UnaryOperator` so there's no ambiguity about which
operators can appear where), so you control how the tree is traversed:

```rust,ignore
use std::collections::BTreeSet;
use filt_rs::{BinaryOperator, Expr, ExprVisitor, Filter, FilterValue, Glob, LogicalOperator, UnaryOperator};

#[derive(Default)]
struct PropertyCollector<'a> {
    properties: BTreeSet<&'a str>,
}

impl<'a> ExprVisitor<'a, ()> for PropertyCollector<'a> {
    fn visit_property(&mut self, name: &'a str) {
        self.properties.insert(name);
    }
    fn visit_binary(&mut self, l: &'a Expr<'a>, _op: BinaryOperator, r: &'a Expr<'a>) {
        self.visit_expr(l);
        self.visit_expr(r);
    }
    // ...and the other node kinds, recursing with `self.visit_expr(child)`.
#   fn visit_literal(&mut self, _v: &'a FilterValue<'a>) {}
#   fn visit_function_call(&mut self, _n: &'a str, args: &'a [Expr<'a>]) { for a in args { self.visit_expr(a); } }
#   fn visit_logical(&mut self, l: &'a Expr<'a>, _op: LogicalOperator, r: &'a Expr<'a>) { self.visit_expr(l); self.visit_expr(r); }
#   fn visit_unary(&mut self, _op: UnaryOperator, r: &'a Expr<'a>) { self.visit_expr(r); }
#   fn visit_like(&mut self, l: &'a Expr<'a>, _g: &'a Glob) { self.visit_expr(l); }
}

let filter = Filter::new("repo.public && repo.stars >= 50")?;
let mut collector = PropertyCollector::default();
filter.visit(&mut collector);
assert!(collector.properties.contains("repo.stars"));
```

See [`examples/property_collector.rs`](examples/property_collector.rs) for the
complete, runnable version:

```shell
cargo run --example property_collector --features visitor
```

## Performance

Filters are parsed once and may then be evaluated against any number of
objects. Evaluation is allocation-free except for the owned `FilterValue`s
your `Filterable::get` implementation returns.

## Secret values

Enable the `secrecy` feature to expose sensitive properties (passwords, API
tokens, and the like) as [secrecy](https://crates.io/crates/secrecy)-backed
secrets. Secret values behave exactly like strings in every filter operation,
but are always redacted when formatted — so they can never leak into your logs:

```shell
cargo add filt-rs --features secrecy
```

```rust,ignore
use filt_rs::{Filter, FilterValue, Filterable};

struct User {
    password: secrecy::SecretString,
}

impl Filterable for User {
    fn get(&self, key: &str) -> FilterValue<'_> {
        match key {
            "user.password" => self.password.clone().into(),
            _ => FilterValue::Null,
        }
    }
}

let user = User { password: "hunter2".into() };

// Secrets compare exactly like strings within filter expressions...
let filter = Filter::new(r#"user.password == "Hunter2""#)?;
assert!(filter.matches(&user)?);

// ...but they are always redacted when formatted.
println!("{}", user.get("user.password")); // prints: [REDACTED]
```

Note that, as with all of the filter language's comparisons, secret comparisons
are not constant-time — don't rely on them to defend against timing attacks.

## Error messages

Errors are designed to be shown directly to the people writing the filters:

```text
Oops! Filter included an orphaned '&' at line 1, column 13 which is not a valid operator.

To try and fix this, you can:
 - Ensure that you are using the '&&' operator to implement a logical AND within your filter.
```

## License

Licensed under the [MIT License](LICENSE).

Copyright © Sierra Softworks.
