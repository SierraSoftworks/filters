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

#[test]
fn selectors_with_the_generator_bit_decode_generated_afl_inputs() {
    // Bit 0 set (structured family) plus bit 1 set (recursive grammar) routes to
    // the recursive `GeneratedFilter` generator rather than the flat one.
    let input = FuzzInput::decode(&[0b11, 1, 2, 3, 4, 5, 6, 7, 8]);

    assert!(matches!(input, FuzzInput::Generated(_)));
}

#[test]
fn generated_expressions_always_parse_and_evaluate() {
    // Drive the recursive generator with a spread of deterministic byte seeds and
    // assert that every expression it *does* produce a parse for also evaluates
    // without error, and that deterministic filters are stable across runs. This
    // is the same contract the `evaluate` fuzz target asserts, checked here so a
    // regression in the generator surfaces without a full fuzzing run.
    let mut checked = 0;
    let mut state: u64 = 0x9E3779B97F4A7C15;
    for seed in 0..4_000u64 {
        // A small xorshift keeps the seeds varied yet reproducible without any
        // dev-dependency on an RNG crate.
        state ^= seed.wrapping_mul(0x2545F4914F6CDD1D);
        let mut bytes = Vec::with_capacity(129);
        bytes.push(0b11); // force the recursive generator
        for i in 0..128u64 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            bytes.push((state >> (i % 56)) as u8);
        }

        let input = FuzzInput::decode(&bytes);
        let FuzzInput::Generated(filter) = &input else {
            continue;
        };

        let expression = filter.expression();
        let Ok(parsed) = Filter::new(&expression) else {
            continue;
        };

        checked += 1;
        let matched = parsed
            .matches(&Target)
            .unwrap_or_else(|error| panic!("failed to evaluate {expression:?}: {error}"));

        if filter.is_deterministic() {
            let again = parsed
                .matches(&Target)
                .expect("a deterministic generated filter to evaluate again");
            assert_eq!(
                again, matched,
                "non-deterministic result for {expression:?}"
            );
        }
    }

    assert!(
        checked > 0,
        "expected the generator to produce at least one parseable expression"
    );
}
