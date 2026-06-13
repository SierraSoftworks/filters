//! Proves the allocation behaviour of `Filter::matches` by counting every
//! heap allocation made while a filter is evaluated.
//!
//! This test installs a counting `#[global_allocator]`, which is safe to do
//! here because each integration test file is compiled as its own binary.
//! All assertions live in a single `#[test]` function so that no other test
//! threads can allocate concurrently and skew the counts.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use filt_rs::{Filter, FilterValue, Filterable};

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Runs the provided closure and returns the number of heap allocations it
/// performed.
fn allocations_during(f: impl FnOnce()) -> usize {
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    f();
    ALLOCATIONS.load(Ordering::Relaxed) - before
}

struct Server {
    hostname: &'static str,
    region: &'static str,
    port: u16,
    healthy: bool,
    tags: [&'static str; 3],
}

impl Filterable for Server {
    fn get(&self, key: &str) -> FilterValue {
        match key {
            // String and tuple values inherently allocate: `Filterable::get`
            // must return an owned `FilterValue` (1 allocation for the
            // String, 1 + len for the Tuple of strings).
            "hostname" => self.hostname.into(),
            "region" => self.region.into(),
            "tags" => self
                .tags
                .iter()
                .map(|&t| t.into())
                .collect::<Vec<FilterValue>>()
                .into(),
            // Numbers, booleans, and nulls are returned by value and never
            // touch the heap.
            "port" => self.port.into(),
            "healthy" => self.healthy.into(),
            _ => FilterValue::Null,
        }
    }
}

#[test]
fn evaluation_allocation_counts() {
    let server = Server {
        hostname: "web-01.example.com",
        region: "eu-west-1",
        port: 443,
        healthy: true,
        tags: ["web", "public", "production"],
    };

    // Each case lists the expected number of heap allocations for a single
    // `Filter::matches` call. The only allocations permitted are those made
    // by `Filterable::get` itself (owned FilterValues are part of its public
    // contract); the interpreter and the string operators must not allocate.
    let cases: &[(&str, usize, &str)] = &[
        ("true", 0, "a literal-only filter"),
        (
            r#""Hello World" contains "WORLD""#,
            0,
            "case-insensitive operators on string literals",
        ),
        (
            r#""choo" in ["one", "two", "choo"] && "Pre-Release" startswith "pre" && "v1.2.3" endswith ".3""#,
            0,
            "tuple and string literals with every string operator",
        ),
        (
            "port == 443 && healthy && !(port < 100)",
            0,
            "numeric and boolean properties (their FilterValues are stack-only)",
        ),
        (
            "missing == null",
            0,
            "an unknown property resolving to null",
        ),
        (
            r#"hostname startswith "WEB" && hostname contains "example" && !(hostname endswith ".org")"#,
            3,
            "three string-property resolutions (one String allocation each, in get)",
        ),
        (
            r#"region in ["eu-west-1", "eu-west-2"]"#,
            1,
            "a string property against a tuple literal (one String allocation in get)",
        ),
        (
            r#"tags contains "production""#,
            4,
            "a tuple property (one Vec plus three Strings, all allocated in get)",
        ),
    ];

    for &(expression, expected, description) in cases {
        let filter = Filter::new(expression).expect("the filter should parse");

        // Warm up once so that any one-off lazy allocations don't get
        // attributed to the measured call.
        filter.matches(&server).expect("the filter should evaluate");

        let allocations = allocations_during(|| {
            assert!(
                filter.matches(&server).expect("the filter should evaluate"),
                "the '{expression}' filter should match the fixture"
            );
        });

        assert_eq!(
            allocations, expected,
            "expected {expected} allocation(s) evaluating {description} ('{expression}'), observed {allocations}"
        );
    }
}
