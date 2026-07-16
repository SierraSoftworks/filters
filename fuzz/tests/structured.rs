use filt_rs::{Filter, FilterValue, Filterable};

#[path = "../fuzz_targets/common.rs"]
mod common;

use common::{BuiltInFunction, FuzzInput, Operand, Operator, StructuredFilter};

struct Target;

impl Filterable for Target {
    fn get(&self, key: &str) -> FilterValue<'_> {
        match key {
            "text" => "  2026-03-12T12:00:00Z fuzz  ".into(),
            "number" => 42.5.into(),
            "boolean" => true.into(),
            "tuple" => vec!["fuzz".into(), "other".into()].into(),
            _ => FilterValue::Null,
        }
    }
}

#[test]
fn every_operator_and_function_combination_parses_and_evaluates() {
    for operator in Operator::ALL {
        for function in BuiltInFunction::ALL {
            let structured = StructuredFilter {
                operator,
                function,
                operand: Operand::StringLiteral,
                text: String::new(),
                number: 0.0,
                boolean: false,
                tuple: Vec::new(),
            };
            let expression = structured.expression();
            let filter = Filter::new(&expression)
                .unwrap_or_else(|error| panic!("failed to parse {expression:?}: {error}"));

            filter
                .matches(&Target)
                .unwrap_or_else(|error| panic!("failed to evaluate {expression:?}: {error}"));
        }
    }
}

#[test]
fn even_selectors_preserve_raw_afl_inputs() {
    let input = FuzzInput::decode(b"doc.title == true");

    assert_eq!(input.expression(), "doc.title == true");
}

#[test]
fn odd_selectors_decode_structured_afl_inputs() {
    let input = FuzzInput::decode(&[1, 0, 0, 0]);

    assert!(matches!(input, FuzzInput::Structured(_)));
}
