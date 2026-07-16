#![no_main]

use filt_rs::Filter;
use libfuzzer_sys::fuzz_target;

mod common;

fuzz_target!(|input: common::FuzzInput| {
    let expression = input.expression();

    if let Ok(filter) = Filter::new(expression.as_str()) {
        assert_eq!(filter.raw(), expression);

        let cloned = filter.clone();
        assert_eq!(cloned.raw(), expression);
    }
});
