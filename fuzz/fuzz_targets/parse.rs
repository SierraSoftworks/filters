use filt_rs::Filter;

mod common;

fn main() {
    afl::fuzz!(|data: &[u8]| {
        let expression = common::FuzzInput::decode(data).expression();

        if let Ok(filter) = Filter::new(expression.as_str()) {
            assert_eq!(filter.raw(), expression);

            let cloned = filter.clone();
            assert_eq!(cloned.raw(), expression);
        }
    });
}
