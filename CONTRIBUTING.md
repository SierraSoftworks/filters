# Contributing to `filt-rs`

Thanks for helping improve `filt-rs`. Changes should preserve cheap,
allocation-free evaluation, friendly errors, and compatibility with every
feature combination. Open an issue before undertaking a large API or language
change so that its design can be agreed before implementation.

## Development setup

### Rust toolchains

The crate uses Rust edition 2024 and has an MSRV of 1.88. Install stable Rust
for normal development and nightly Rust for fuzzing:

```sh
rustup toolchain install stable --component rustfmt clippy
rustup toolchain install nightly
cargo install cargo-afl --version 0.18.2 --locked
```

Commands in this guide select nightly with `+nightly`, so your default
toolchain can remain stable.

### Native dependencies

`cargo-afl` bundles AFL++ and uses LLVM instrumentation. Install a C compiler
and `make`:

| Platform | Setup |
| --- | --- |
| Linux | Install a C toolchain and `make`, such as `sudo apt-get install build-essential clang` on Ubuntu/Debian. |
| macOS | Install the Xcode Command Line Tools with `xcode-select --install`. Both x86-64 and Apple Silicon macOS are supported. |
| Windows | Run AFL++ inside WSL2 or Docker; `afl.rs` does not support native Windows. |

The official `aflplusplus/aflplusplus` container can be used when a native
Linux or macOS environment is unavailable.

### Repository conventions

- Add behavior-focused tests for core use cases and failure modes. Existing
  parameterized tests use `rstest` and should be extended when appropriate.
- Keep optional dependencies behind default-off features and avoid allocations
  in `Filter::matches`. Return borrowed property values from `Filterable::get`
  whenever possible.
- Public APIs need rustdoc and runnable examples. Keep changes focused and use
  the surrounding naming, error, and comment style.

## Test and quality checks

### Fast feedback

Run a focused test while iterating, then format the repository:

```sh
cargo test lexer::tests::test_name
cargo test --test behaviour test_name
cargo fmt --all
```

Examples and doc tests are ordinary Cargo test targets. A failed allocation
test usually indicates a performance regression rather than an expected test
fixture update.

### Full validation

Reproduce the main CI workflow before pushing:

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --no-fail-fast
cargo test --all-features --no-fail-fast
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps
```

In PowerShell, set rustdoc flags for the current session before building:

```powershell
$env:RUSTDOCFLAGS = '-D warnings'
cargo doc --all-features --no-deps
```

### Feature and performance checks

Run `cargo test --no-default-features` when changing feature-gated code. The
allocation guardrails are available directly when investigating evaluation
costs:

```sh
cargo test --test alloc
cargo test --test alloc_counter
cargo bench --bench filtering
```

Only change allocation expectations when the increase is intentional and
explained. Benchmark results are diagnostic and are not a required CI gate.

## Fuzzing

### Targets and build checks

The standalone package in `fuzz/` contains two targets:

| Target | Coverage |
| --- | --- |
| `parse` | Arbitrary filter text, parse errors, AST construction, and cloning parsed filters. |
| `evaluate` | Parsing plus repeated evaluation against borrowed strings, numbers, booleans, nulls, and tuples. |

Compile and lint both targets without starting a fuzzing campaign:

```sh
cargo +nightly check --manifest-path fuzz/Cargo.toml --bins
cargo +nightly clippy --manifest-path fuzz/Cargo.toml --bins -- -D warnings
cargo +nightly afl build --manifest-path fuzz/Cargo.toml --release --bins
```

### Running fuzzers

Run either target for five minutes against its curated corpus:

```sh
cargo +nightly afl fuzz -i fuzz/corpus/parse -o fuzz/output/parse -V 300 -t 10000 -- fuzz/target/release/parse
cargo +nightly afl fuzz -i fuzz/corpus/evaluate -o fuzz/output/evaluate -V 300 -t 10000 -- fuzz/target/release/evaluate
```

Omit `-V 300` to run until interrupted. AFL++ keeps generated inputs and state
under `fuzz/output/`, leaving the curated seed corpus unchanged.

### Failures and CI

Crashing and hanging inputs are written under
`fuzz/output/<target>/default/{crashes,hangs}/`. Reproduce one through standard
input, then add a focused regression test before fixing the defect:

```sh
cargo +nightly afl run --manifest-path fuzz/Cargo.toml --bin parse < fuzz/output/parse/default/crashes/id:<case>
```

Pull requests and `main` run each target for 60 seconds. Scheduled and manually
dispatched workflows run each target for five minutes, retain logs and crash
artifacts, and create or update a target-specific issue on failure. See the
[Rust Fuzz Book](https://rust-fuzz.github.io/book/afl.html) and
[AFL++ documentation](https://github.com/AFLplusplus/AFLplusplus/tree/stable/docs)
for advanced campaign, corpus minimization, and debugging workflows.