#![no_main]

use filt_rs::{Filter, FilterValue, Filterable};
use libfuzzer_sys::fuzz_target;

mod common;

const MAX_TUPLE_LENGTH: usize = 8;

struct FuzzObject<'a> {
    text: &'a str,
    number: f64,
    boolean: bool,
    tuple: Vec<&'a str>,
}

impl Filterable for FuzzObject<'_> {
    fn get(&self, key: &str) -> FilterValue<'_> {
        match key {
            "text" | "doc.title" => self.text.into(),
            "number" | "doc.pages" => self.number.into(),
            "boolean" | "doc.published" => self.boolean.into(),
            "tuple" | "doc.tags" => self
                .tuple
                .iter()
                .map(|value| (*value).into())
                .collect::<Vec<FilterValue<'_>>>()
                .into(),
            _ => FilterValue::Null,
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let input = common::bounded_text(data);
    let mut fields = input.lines();
    let expression = fields.next().unwrap_or_default();
    let text = fields.next().unwrap_or_default();
    let tuple = fields.take(MAX_TUPLE_LENGTH).collect();

    let mut number_bytes = [0; size_of::<f64>()];
    let byte_count = data.len().min(number_bytes.len());
    number_bytes[..byte_count].copy_from_slice(&data[..byte_count]);

    let target = FuzzObject {
        text,
        number: f64::from_le_bytes(number_bytes),
        boolean: data.first().is_some_and(|byte| byte & 1 != 0),
        tuple,
    };

    if let Ok(filter) = Filter::new(expression) {
        let matched = filter
            .matches(&target)
            .expect("a parsed filter using built-in functions to evaluate");

        if !expression.contains("now") && !expression.contains("ago") {
            assert_eq!(
                filter
                    .matches(&target)
                    .expect("a parsed deterministic filter to evaluate again"),
                matched
            );
        }
    }
});
