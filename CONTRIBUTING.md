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
cargo install cargo-fuzz --version 0.13.2 --locked
```

Commands in this guide select nightly with `+nightly`, so your default
toolchain can remain stable.

### Native dependencies

`cargo-fuzz` uses libFuzzer and LLVM sanitizer instrumentation. Install a C++11
compiler and the platform's AddressSanitizer support:

| Platform | Setup |
| --- | --- |
| Linux | Install a C++ toolchain, such as `sudo apt-get install build-essential clang` on Ubuntu/Debian. The CI fuzzers use `x86_64-unknown-linux-gnu` with dynamically linked GNU libc. |
| macOS | Install the Xcode Command Line Tools with `xcode-select --install`. Both x86-64 and Apple Silicon macOS are supported. |
| Windows | Install Visual Studio 2022 with **MSVC v143 - VS 2022 C++ x64/x86 build tools** and **C++ AddressSanitizer** through Visual Studio Installer. |

On Windows, run fuzzers from **Developer PowerShell for VS 2022** or initialize
an existing PowerShell session before running Cargo:

```powershell
Import-Module 'C:\Program Files\Microsoft Visual Studio\2022\Community\Common7\Tools\Microsoft.VisualStudio.DevShell.dll'
Enter-VsDevShell `
  -VsInstallPath 'C:\Program Files\Microsoft Visual Studio\2022\Community' `
  -SkipAutomaticLocation `
  -DevCmdArguments '-arch=x64 -host_arch=x64'
```

Visual Studio editions or installation paths may differ. The initialized shell
must be able to find the MSVC linker and AddressSanitizer libraries. The
optional Windows debugger path is
`C:\Program Files (x86)\Windows Kits\10\Debuggers\x64`.

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
```

### Running fuzzers

Run either target for five minutes against its curated corpus:

```sh
cargo +nightly fuzz run parse fuzz/corpus/parse -- -max_total_time=300 -timeout=10
cargo +nightly fuzz run evaluate fuzz/corpus/evaluate -- -max_total_time=300 -timeout=10
```

Omit `-max_total_time` to run until interrupted. `-runs=1000` is useful for a
quick local smoke test. libFuzzer adds discovered inputs to the corpus directory
while it runs; do not commit hash-named generated inputs unless they provide
durable coverage that the named seeds do not.

### Failures and CI

Crashing inputs are written under `fuzz/artifacts/<target>/`. Reproduce one by
passing its path to the target, then add a focused regression test before
fixing the defect:

```sh
cargo +nightly fuzz run parse fuzz/artifacts/parse/crash-<hash>
```

Pull requests and `main` run each target for 60 seconds. Scheduled and manually
dispatched workflows run each target for five minutes, retain logs and crash
artifacts, and create or update a target-specific issue on failure. See the
[Rust Fuzz Book](https://rust-fuzz.github.io/book/cargo-fuzz.html) for advanced
corpus minimization, sanitizer, and debugging workflows.