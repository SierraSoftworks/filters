#![no_main]

use filt_rs::Filter;
use libfuzzer_sys::fuzz_target;

mod common;

fuzz_target!(|data: &[u8]| {
    let input = common::bounded_text(data);

    if let Ok(filter) = Filter::new(input.as_str()) {
        assert_eq!(filter.raw(), input);

        let cloned = filter.clone();
        assert_eq!(cloned.raw(), input);
    }
});
