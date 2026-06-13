//! Proof that the `like` and `matches` operators perform no pattern-related
//! heap allocation at evaluation time.
//!
//! This test binary installs a counting `#[global_allocator]` (each file under
//! `tests/` is its own binary, so this affects nothing else) and runs without
//! the default libtest harness: the measurements need a single-threaded
//! process, since allocations made concurrently by other tests would pollute
//! the global counter.
//!
//! What is (and isn't) proven here:
//!
//! - We measure a baseline filter which only resolves the property
//!   (`branch.name`) and assert that evaluating `branch.name like "..."` (or
//!   `... matches "..."`) performs *exactly* the same number of allocations —
//!   i.e. the pattern matching itself contributes zero. (Since `FilterValue`
//!   borrows its string data, resolving `branch.name` is itself allocation
//!   free here, so this baseline happens to be zero — but the invariant under
//!   test is the *difference*, which holds regardless.)
//! - For non-string properties no allocation happens at all, which we assert
//!   as an absolute zero-allocation case.
//! - Regex matching is *amortized* allocation-free: the regex engine lazily
//!   allocates per-thread scratch space the first time a pattern is used, so
//!   the regex measurement warms the filter up first.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

use filters::{Filter, FilterValue, Filterable};

struct CountingAllocator;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn count_allocations(f: impl FnOnce()) -> u64 {
    let before = ALLOCATIONS.load(Ordering::SeqCst);
    f();
    ALLOCATIONS.load(Ordering::SeqCst) - before
}

struct Branch(&'static str);

impl Filterable for Branch {
    fn get(&self, key: &str) -> FilterValue<'_> {
        match key {
            "branch.name" => self.0.into(),
            "branch.protected" => true.into(),
            _ => FilterValue::Null,
        }
    }
}

const ITERATIONS: u64 = 1_000;

/// Evaluates the filter `ITERATIONS` times, asserting the expected outcome,
/// and returns the number of heap allocations performed.
fn allocations_for(filter: &Filter, target: &Branch, expected: bool) -> u64 {
    count_allocations(|| {
        for _ in 0..ITERATIONS {
            assert_eq!(
                filter.matches(target).expect("the filter should evaluate"),
                expected
            );
        }
    })
}

fn like_adds_no_allocations_over_property_resolution() {
    let baseline = Filter::new("branch.name").expect("parse the baseline filter");
    let hit = Filter::new(r#"branch.name like "feat/*""#).expect("parse the like filter");
    let miss = Filter::new(r#"branch.name like "*a*ab""#).expect("parse the like filter");

    let target = Branch("feat/login");

    // Warm up so that any one-off allocations (e.g. lazy thread-local state)
    // don't skew the measurement.
    assert!(baseline.matches(&target).expect("evaluate the baseline"));
    assert!(hit.matches(&target).expect("evaluate the hit filter"));
    assert!(!miss.matches(&target).expect("evaluate the miss filter"));

    let baseline_allocs = allocations_for(&baseline, &target, true);
    let hit_allocs = allocations_for(&hit, &target, true);
    let miss_allocs = allocations_for(&miss, &target, false);

    assert_eq!(
        hit_allocs, baseline_allocs,
        "a matching 'like' evaluation should allocate exactly as much as resolving the property"
    );
    assert_eq!(
        miss_allocs, baseline_allocs,
        "a backtracking 'like' miss should allocate exactly as much as resolving the property"
    );
}

fn like_against_non_string_properties_is_absolutely_allocation_free() {
    // Boolean property resolution doesn't allocate at all, so this proves the
    // entire evaluation pipeline (context setup, expression walking, and the
    // 'like' operator's type dispatch) is allocation-free end to end.
    let filter = Filter::new(r#"branch.protected like "*""#).expect("parse the filter");
    let target = Branch("feat/login");

    // Non-string values never match a pattern, even the match-all glob.
    assert!(!filter.matches(&target).expect("evaluate the filter"));

    let allocs = allocations_for(&filter, &target, false);
    assert_eq!(allocs, 0, "expected zero allocations, found {allocs}");
}

#[cfg(feature = "regex")]
fn matches_adds_no_allocations_over_property_resolution_after_warmup() {
    let baseline = Filter::new("branch.name").expect("parse the baseline filter");
    let hit = Filter::new(r#"branch.name matches r"^release/v\d+(\.\d+){2}$""#)
        .expect("parse the matches filter");
    let miss =
        Filter::new(r#"branch.name matches r"^feat/.+$""#).expect("parse the matches filter");

    let target = Branch("release/v1.2.3");

    // The regex engine allocates per-thread scratch space lazily on first
    // use, so matching is only *amortized* allocation-free: warm each filter
    // up before measuring.
    for _ in 0..16 {
        assert!(baseline.matches(&target).expect("evaluate the baseline"));
        assert!(hit.matches(&target).expect("evaluate the hit filter"));
        assert!(!miss.matches(&target).expect("evaluate the miss filter"));
    }

    let baseline_allocs = allocations_for(&baseline, &target, true);
    let hit_allocs = allocations_for(&hit, &target, true);
    let miss_allocs = allocations_for(&miss, &target, false);

    assert_eq!(
        hit_allocs, baseline_allocs,
        "a matching 'matches' evaluation should allocate exactly as much as resolving the property"
    );
    assert_eq!(
        miss_allocs, baseline_allocs,
        "a missing 'matches' evaluation should allocate exactly as much as resolving the property"
    );
}

fn main() {
    like_adds_no_allocations_over_property_resolution();
    println!("ok - like adds no allocations over property resolution");

    like_against_non_string_properties_is_absolutely_allocation_free();
    println!("ok - like against non-string properties is absolutely allocation-free");

    #[cfg(feature = "regex")]
    {
        matches_adds_no_allocations_over_property_resolution_after_warmup();
        println!("ok - matches adds no allocations over property resolution (after warmup)");
    }
}
