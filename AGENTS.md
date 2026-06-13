# Working on `filt-rs`

Guidance for humans and coding agents contributing to this crate. `CLAUDE.md`
is a symlink to this file, so both names point at the same instructions.

`filt-rs` is a small, human-friendly filter-expression DSL: users write a filter
string, you implement the single-method `Filterable` trait on your type, and the
crate parses the expression once (`Filter::new`) and evaluates it cheaply against
any number of objects (`Filter::matches`).

## Design goals — keep these in mind for every change

1. **Evaluation should be allocation-free.** Parsing may allocate; evaluating a
   parsed filter against an object should not. The interpreter returns
   `Cow<'a, FilterValue<'a>>` and borrows literals straight out of the AST, and
   `FilterValue` carries its string data as a `Cow` so it can *borrow* rather
   than copy. `From<&str>` borrows (`Cow::Borrowed`), and `Filterable::get`
   returns a value tied to `&self` — so implementors should hand out borrows
   (`self.field.as_str().into()`) instead of allocating owned copies.
   - This is not aspirational: it is enforced by tests (see "Performance
     guardrails"). A change that adds an allocation to evaluation will fail CI.
2. **Parse once, evaluate many.** Anything expensive (lexing, parsing, compiling
   globs/regexes) happens in `Filter::new`. `Filter::matches` must stay cheap.
3. **Friendly errors.** Parse/eval errors carry the exact location and remedial
   advice (via the `human-errors` crate). Preserve that quality.
4. **Lightweight & optional.** One small required dependency; everything else
   (`chrono`, `regex`, `secrecy`, `serde`) is behind a default-off feature. Code
   must compile and pass tests in every feature combination.
5. **No `unsafe` in the public API surface.** The only `unsafe` is the internal
   pinned-string trick in `Filter`; don't add more.

## Before you push — reproduce CI locally

CI (`.github/workflows/ci.yml`) has three jobs. The most common red build is
**formatting**, so run `cargo fmt --all` before every commit. Run the full set
below and make sure it's clean before pushing:

```sh
cargo fmt --all                                       # then: git add the result
cargo fmt --all --check                               # Lint job, step 1
cargo clippy --all-targets --all-features -- -D warnings   # Lint job, step 2 (warnings are errors)
cargo test --no-fail-fast                             # Test job, default features
cargo test --all-features --no-fail-fast              # Test job, all features
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps   # Docs job (doc warnings are errors)
```

Notes:
- **`cargo fmt`** is non-negotiable — CI runs `--check` and fails on any diff.
  Editing code by hand (especially long generic/lifetime signatures or wrapped
  literals) frequently leaves formatting rustfmt would change; let the formatter
  fix it rather than guessing.
- **Clippy runs with `-D warnings`** — a warning fails the build.
- **Docs run with `RUSTDOCFLAGS=-D warnings`** — broken intra-doc links and
  other rustdoc warnings fail the build. If you add/rename a public item, update
  the doc links that reference it.
- Also run `cargo test --no-default-features` locally when you touch
  feature-gated (`#[cfg(feature = "...")]`) code — CI covers default and all
  features, but feature interactions are easy to break.
- A separate **Security Audit** workflow runs `cargo audit` on a schedule and
  when `Cargo.toml`/`Cargo.lock` change.

## Performance guardrails

Two tests assert the zero-allocation contract by installing a counting
`#[global_allocator]`:

- `tests/alloc_counter.rs` asserts the *exact* number of heap allocations for a
  range of `Filter::matches` calls. String/number/bool property resolution
  should allocate nothing; only a tuple property's backing `Vec` allocates.
- `tests/alloc.rs` (runs with `harness = false`, single-threaded) proves the
  `like` and `matches` operators add *zero* allocations over plain property
  resolution.

If you change evaluation and these counts move, that is a signal — investigate
whether you introduced an allocation. Only update the expected numbers when the
change is intentional and you can explain it. `benches/filtering.rs` (run with
`cargo bench`) tracks evaluation throughput, including an
`eval_borrowed_vs_owned` group that quantifies the cost of returning owned vs.
borrowed strings from `Filterable::get`.

## Repository layout

| Path | Purpose |
| --- | --- |
| `src/value.rs` | `FilterValue`, the `Filterable` trait, `From`/comparison impls |
| `src/interpreter.rs` | Evaluation — the `ExprVisitor` that walks the AST |
| `src/expr.rs` | AST node types (`Expr`) and the `ExprVisitor` trait |
| `src/lexer.rs`, `src/token.rs` | Tokenizer |
| `src/parser.rs` | Recursive-descent parser (precedence climbing) |
| `src/pattern.rs` | Compiled glob (`like`) and regex (`matches`) |
| `src/case_sensitivity.rs` | Allocation-free Unicode case-folding |
| `src/lib.rs` | Public API: `Filter`, re-exports, serde, top-level docs |
| `tests/` | `behaviour`, `patterns`, `secrecy`, `alloc`, `alloc_counter` |
| `benches/filtering.rs` | Criterion benchmarks (`parse`, `eval`, `eval_borrowed_vs_owned`) |

## Lifetimes & borrowing notes

`FilterValue<'a>` is covariant and borrows its string data. A few consequences
to keep in mind when editing:

- `Filterable::get(&self, ..) -> FilterValue<'_>` ties the returned value to
  `&self`. You **cannot** return a value that borrows from a temporary inside
  `get` (e.g. `Other::default().get(key)`); own the source first (store it in a
  field) so the borrow is tied to `&self`.
- The interpreter unifies the AST and target lifetimes into one during a single
  `matches` call. `FilterValue`'s comparison helpers take `&FilterValue<'a>`
  with the same lifetime as `self`, and `PartialEq`/`PartialOrd` are `Self`-based
  — covariance lets callers shrink longer-lived values to match.
- When a public item gains a lifetime, the surrounding ergonomic changes ripple:
  `let x: FilterValue` annotations become `FilterValue<'_>`, and
  `collect::<Vec<FilterValue>>()` becomes `collect::<Vec<FilterValue<'_>>>()`.

## Conventions

- **Edition 2024, MSRV `1.88`** (`Cargo.toml` `rust-version`). Don't use APIs
  newer than the MSRV.
- **Tests** use [`rstest`](https://docs.rs/rstest) for parameterized cases;
  follow the existing `#[case(...)]` style. Keep behavioural coverage thorough —
  add cases for new operators, edge cases, and failure modes.
- **Doc examples** are part of the test suite (`cargo test --doc`); every public
  item should have a runnable, accurate example.
- **Match the surrounding style** — comment density, naming, and idioms. The
  codebase favours explanatory comments on non-obvious decisions.
- **Commits** end with the trailer used across this repo:
  `Co-Authored-By: <author>` when pair-authored with an agent.
