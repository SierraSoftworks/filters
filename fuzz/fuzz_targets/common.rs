#![allow(dead_code)]

const MAX_INPUT_LENGTH: usize = 16 * 1024;

pub fn bounded_bytes(data: &[u8]) -> &[u8] {
    &data[..data.len().min(MAX_INPUT_LENGTH)]
}

pub fn bounded_text(data: &[u8]) -> String {
    String::from_utf8_lossy(bounded_bytes(data)).into_owned()
}
