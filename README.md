<div align="center">
  <img src="assets/logo.svg" alt="filters" width="440">

  <p><strong>A human-friendly filter expression language for matching your objects against user-provided queries.</strong></p>

  <p>
    <a href="https://github.com/SierraSoftworks/filters/actions/workflows/ci.yml"><img src="https://github.com/SierraSoftworks/filters/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
    <a href="https://crates.io/crates/filters"><img src="https://img.shields.io/crates/v/filters.svg" alt="crates.io"></a>
    <a href="https://docs.rs/filters"><img src="https://img.shields.io/docsrs/filters" alt="docs.rs"></a>
    <a href="LICENSE"><img src="https://img.shields.io/github/license/SierraSoftworks/filters" alt="MIT License"></a>
  </p>
</div>

---

`filters` gives your users a small, safe, and friendly expression language for
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
- **Optional serde support** — deserialize filters directly out of your
  configuration files with the `serde` feature.

## Usage

```shell
cargo add filters
```

Implement `Filterable` for your type, then parse and evaluate filters:

```rust
use filters::{Filter, FilterValue, Filterable};

struct Repo {
    name: &'static str,
    public: bool,
    stars: u32,
}

impl Filterable for Repo {
    fn get(&self, key: &str) -> FilterValue {
        match key {
            "repo.name" => self.name.into(),
            "repo.public" => self.public.into(),
            "repo.stars" => self.stars.into(),
            _ => FilterValue::Null,
        }
    }
}

fn main() -> Result<(), filters::Error> {
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

| Literal    | Example         | Notes                                            |
| ---------- | --------------- | ------------------------------------------------ |
| Null       | `null`          | Also returned for properties which aren't found. |
| Boolean    | `true`, `false` |                                                  |
| Number     | `123`, `123.45` | All numbers are 64-bit floats internally.        |
| String     | `"hello"`       | Escape embedded quotes with `\"`.                |
| Raw string | `r"^v\d+$"`     | No escape processing; cannot contain `"`.        |
| Tuple      | `["a", "b"]`    | A list of literal values.                        |

### Properties

Any other identifier — including `.` and `-` separated names like
`release.prerelease` or `asset.source-code` — is treated as a property
reference and resolved by calling `Filterable::get` on the target object.
Operator keywords (`in`, `contains`, `startswith`, `endswith`, `like`,
`matches`) are reserved and cannot be used as property names.

### Operators

In order of increasing precedence:

| Operator                 | Meaning                                                 |
| ------------------------ | ------------------------------------------------------- |
| `\|\|`                   | Logical OR (short-circuiting).                          |
| `&&`                     | Logical AND (short-circuiting).                         |
| `==`, `!=`               | Equality (strings are compared case-insensitively).     |
| `>`, `>=`, `<`, `<=`     | Ordering comparisons.                                   |
| `contains`               | String contains a substring, or tuple contains a value. |
| `in`                     | Inverse of `contains` (`a in b` ≡ `b contains a`).      |
| `startswith`, `endswith` | String prefix/suffix tests (case-insensitive).          |
| `like`                   | Case-insensitive glob match (`*` and `?` wildcards).    |
| `matches`                | Regular expression match (requires the `regex` feature). |
| `!`                      | Logical NOT (unary).                                    |
| `(...)`                  | Grouping.                                               |

### Pattern matching

The `like` operator matches a string against a glob pattern, where `*` matches
any sequence of characters (including none), `?` matches exactly one
character, and a backslash makes the following character literal (`\*`, `\?`,
`\\`). Character classes like `[a-z]` are not supported. As with the rest of
the language, matching is case-insensitive, using the same character-folding
rules as `contains`, `startswith`, and `endswith`:

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
cargo add filters --features regex
```

### Examples

```text
!repo.fork && repo.name contains "awesome"
!release.prerelease && !asset.source-code
size > 1024 && (archived || disabled)
"backup" in tags
branch.name like "feat/*"
branch.name matches r"^release/v\d+(\.\d+){2}$"
```

## Serde support

Enable the `serde` feature to deserialize filters directly from your
configuration files:

```shell
cargo add filters --features serde
```

```rust,ignore
#[derive(serde::Deserialize)]
struct BackupPolicy {
    kind: String,
    from: String,
    #[serde(default)]
    filter: filters::Filter,
}
```

Missing or `null` filter fields deserialize to the match-everything filter
`true`, so optional filters work out of the box.

## Performance

Filters are parsed once and may then be evaluated against any number of
objects. Evaluation is allocation-free except for the owned `FilterValue`s
your `Filterable::get` implementation returns.

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
