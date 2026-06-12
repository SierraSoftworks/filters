# Performance

This document describes the performance characteristics of the `filters`
crate, the optimizations applied on the `perf/zero-alloc-eval` branch, and
the opportunities which were identified but deliberately *not* implemented
(usually because they would break the public API).

All numbers were collected with the criterion suite in
`benches/filtering.rs` (`cargo bench`) on a single Windows machine; treat
them as relative indicators rather than absolute truths. "Before" is the
state of `main` immediately prior to these changes; "after" is the tip of
this branch. The change column is criterion's own statistical estimate of
the delta between the two runs.

## Evaluation-time allocation model

### Before

Every call to `Filter::matches` walked the AST with a visitor returning
*owned* `FilterValue`s, which meant:

1. **Every literal was cloned per evaluation** (`visit_literal` called
   `value.clone()`). For string and tuple literals this is one or more heap
   allocations each time the filter is evaluated, even though the literal
   never changes.
2. **Every property value was cloned twice** — `visit_property` called
   `self.target.get(name).clone()`, cloning the already-owned value returned
   by `Filterable::get` a second time. For a string property this doubled
   the allocation count; for a tuple of *n* strings it added *n + 1*
   redundant allocations.
3. **Every case-insensitive string operator allocated twice** —
   `FilterValue::contains`, `startswith`, and `endswith` called
   `str::to_lowercase()` on both operands, allocating two temporary
   `String`s per comparison.

A realistic compound filter such as

```text
repo.public && !repo.archived && (repo.language in ["rust", "go", "typescript"]
  || repo.stars >= 500) && repo.name startswith "git"
  && !(repo.topics contains "deprecated")
```

performed on the order of a dozen heap allocations *per object evaluated*.

### After

The (private) `ExprVisitor` trait now carries the AST lifetime, and the
interpreter returns `Cow<'_, FilterValue>`:

- Literals are returned as `Cow::Borrowed` — never cloned.
- Property values are `Cow::Owned`, allocated exactly once (inside the
  user's `Filterable::get`), with the redundant second clone removed.
- Operator results are `Cow::Owned(FilterValue::Bool(..))`, which is
  stack-only.
- The case-insensitive string operators compare the operands as case-folded
  character streams (`chars().flat_map(char::to_lowercase)`) without
  allocating at all.

The result: **`Filter::matches` performs zero heap allocations** unless the
filter resolves a string- or tuple-valued property, in which case the only
allocations are the ones `Filterable::get` itself makes to build its owned
return value. This is pinned down by `tests/alloc_counter.rs`, which counts
allocations with a custom `#[global_allocator]` and asserts exact counts:

| Filter shape                                              | Allocations per `matches()` |
| --------------------------------------------------------- | --------------------------- |
| Literal-only (incl. string/tuple literals + operators)    | 0                           |
| Numeric / boolean / null properties                       | 0                           |
| String property (`hostname startswith "WEB"`)             | 1 (the `String` in `get`)   |
| Tuple property of 3 strings (`tags contains "production"`)| 4 (`Vec` + 3 `String`s in `get`) |

## Benchmark results

Median times from criterion (1s warm-up, 2s measurement per benchmark).

### Evaluation (`Filter::matches`)

| Benchmark                 | Before    | After     | Change (criterion estimate) |
| ------------------------- | --------- | --------- | --------------------------- |
| `eval/literal_only`       | 5.35 ns   | 6.27 ns   | **+17%** (see note below)   |
| `eval/numeric_boolean`    | 61.2 ns   | 55.0 ns   | −5.8%                       |
| `eval/string_heavy`       | 863.9 ns  | 425.2 ns  | **−53.3%**                  |
| `eval/tuple_membership`   | 274.2 ns  | 68.6 ns   | **−75.1%**                  |
| `eval/tuple_property`     | 466.9 ns  | 186.1 ns  | **−59.2%**                  |
| `eval/compound_realistic` | 1.091 µs  | 429.9 ns  | **−62.2%**                  |

> **Note on `eval/literal_only`:** the `Cow` wrapper costs roughly one
> nanosecond on the trivial `true` filter (a 5 ns floor dominated by call
> overhead). Every non-trivial filter shape comes out ahead — substantially
> so for anything involving strings or tuples — so this trade-off was
> accepted.

### Parsing (`Filter::new`)

| Benchmark       | Before    | After     | Change (criterion estimate) |
| --------------- | --------- | --------- | --------------------------- |
| `parse/simple`  | 424.6 ns  | 428.7 ns  | −3.7% (within noise)        |
| `parse/complex` | 2.791 µs  | 2.735 µs  | −5.8%                       |

Parsing was not the focus of this work; the small gains come from the
escape-free string fast path and the O(1) decimal-point lookahead described
below.

## Optimizations implemented

### 1. Borrowed literals during evaluation (`src/expr.rs`, `src/interpreter.rs`)

The private `ExprVisitor<T>` trait became `ExprVisitor<'a, T>`, tying
visited nodes to the AST's lifetime so that visitors can return values
which borrow from the tree. The interpreter's result type changed from
`FilterValue` to `Cow<'a, FilterValue>`: literals are `Cow::Borrowed`,
property resolutions and operator results are `Cow::Owned`.

Comparisons are routed through `as_ref()` so that `FilterValue`'s bespoke
`lt`/`le`/`gt`/`ge` implementations are used rather than `Cow`'s defaults
(which would re-derive them from `partial_cmp` and change behaviour, e.g.
for `null < null` or tuple ordering).

This is the main driver of the `tuple_membership` (−75%) and
`compound_realistic` (−62%) improvements.

### 2. Redundant property clone removed (`src/interpreter.rs`)

`visit_property` previously did `self.target.get(name).clone()` — the
`.clone()` re-allocated the already-owned result of `Filterable::get`.
It is now consumed directly. This halved the allocation cost of every
string-property access and is a large part of the `tuple_property` (−59%)
improvement.

### 3. Allocation-free case-insensitive string operators (`src/value.rs`)

`FilterValue::{contains, startswith, endswith}` no longer call
`str::to_lowercase()` on both operands (two `String` allocations per
comparison). They now compare the operands as case-folded character
streams; `endswith` walks both strings in reverse using
`DoubleEndedIterator`, and `contains` performs the substring search over
the folded stream directly. Main driver of `string_heavy` (−53%).

**Behavioural caveat — Greek final sigma:** `str::to_lowercase` applies a
context-sensitive rule mapping `Σ` to `ς` at the end of a word and `σ`
elsewhere; a character-by-character fold cannot see that context. Rather
than diverge inconsistently, all sigma forms (`Σ`, `σ`, `ς`) are now
explicitly folded together (mirroring Unicode simple case folding), making
sigma comparisons position-independent and strictly *more* permissive than
before. Example: `"ΛΟΓΟΣ" endswith "Σ"` previously evaluated to `false`
(the operands lowercased to `…ς` and `σ` respectively) and now evaluates
to `true`; `"ΛΟΓΟΣ" endswith "ς"` matches under both implementations. This
is pinned by `test_greek_sigma_forms_are_equivalent` (unit) and
`greek_sigma_forms_are_interchangeable` (behaviour), and documented on the
public methods. All other Unicode behaviour — including multi-character
expansions such as `İ` → `i` + combining dot, and matches beginning
mid-expansion — is preserved exactly (also pinned by tests). String
*equality* remains ASCII-only case-insensitive, unchanged.

### 4. Parse-time: escape-free string fast path (`src/parser.rs`)

String literals were unconditionally run through two `str::replace` passes
(unescaping `\"` and `\\`), allocating two `String`s per literal. Literals
without a backslash — the common case — now skip both passes and allocate
only the single owned `String` the AST needs.

### 5. Parse-time: O(1) decimal-point lookahead (`src/lexer.rs`)

`read_number` peeked at the character after a `.` using
`source.chars().nth(byte_index + 1)`, which is O(n) in the filter length
*and* subtly wrong: `nth` counts characters while the index is a byte
offset, so a multi-byte character earlier in the filter would shift the
lookahead. It now slices the source at the byte offset and reads the next
char directly — O(1) and always correct.

## Opportunities identified but NOT implemented

These are documented for future review; most require an API break and
would belong in a 2.0.

1. **Borrowed property values (`Filterable::get` returning a borrowed
   `FilterValue<'a>` or a `get_ref` API).** The remaining evaluation-time
   allocations all come from `Filterable::get` returning an *owned*
   `FilterValue`. A lifetime-parameterized `FilterValue<'a>` (with
   `Cow<'a, str>` / borrowed tuple variants), or an additional
   `get_ref(&self, key) -> Option<FilterValueRef<'_>>` method, would let
   implementations hand out `&str` views with zero allocations. Both
   options change the public `Filterable` contract or the public
   `FilterValue` enum and are therefore API breaks.

2. **Arena allocation / flattening of the AST.** `Expr` boxes each child
   (`Binary(Box<Expr>, Token, Box<Expr>)`), so parsing a filter performs
   one allocation per node and evaluation chases pointers. Allocating all
   nodes into a single arena (or a flat `Vec<Expr>` with index-based
   children) would improve parse cost and evaluation cache locality. This
   is entirely private to the crate, but is complicated by the
   self-referential `Pin<Box<String>>` design in `Filter` — the arena
   would need to live alongside the pinned source string. Worth doing if
   parse throughput ever matters; evaluation is already pointer-stable.

3. **`SmallVec` for tuple literals and tuple property values.** Most
   tuples in real filters are short (2–5 elements); an inline small-vector
   would eliminate the `Vec` allocation for tuple *literals* at parse time
   and shrink `Filterable::get`'s cost for tuple properties. Blocked for
   property values by `FilterValue::Tuple(Vec<FilterValue>)` being public;
   parse-time literals could adopt it privately only by adding a private
   parallel value type, which wasn't judged worth the duplication.

4. **String interning / pre-lowering of literals.** String literals could
   be case-folded once at parse time and cached alongside the AST, turning
   repeated case-insensitive comparisons into direct memcmp-style
   comparisons against the pre-folded needle. This is purely internal and
   non-breaking, but the char-stream comparison is already allocation-free
   and the measured string benchmarks are dominated by the haystack scan
   (which cannot be precomputed), so the added complexity wasn't justified
   yet. Revisit if filters with many string literals become a hot path.

5. **Returning `bool` from logical/binary visitors.** The interpreter
   could evaluate boolean-only subtrees into plain `bool`s and avoid
   constructing `Cow::Owned(FilterValue::Bool(..))` intermediates. The
   filter language deliberately lets `&&`/`||` return the deciding
   *operand's value* (like JavaScript), so this would need a two-typed
   evaluation path for marginal gains — `FilterValue::Bool` is stack-only
   anyway. Not worth the complexity; the ~1 ns `literal_only` regression
   is the entire cost this would claw back.

6. **Sub-match short-circuit data layout.** `visit_binary` evaluates both
   operands before dispatching on the operator; for `==`/`!=` against
   `Null` literals etc. there is no cheaper path, and logical operators
   already short-circuit. No action needed.

## Running the benchmarks

```shell
cargo bench                      # full suite
cargo bench -- eval              # evaluation benchmarks only
cargo test --test alloc_counter  # exact allocation counts per filter shape
```

Criterion stores its baselines under `target/criterion/`; run the suite
once before making changes and it will report the deltas on subsequent
runs automatically.
