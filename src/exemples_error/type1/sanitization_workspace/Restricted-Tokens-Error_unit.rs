pub fn restricted_tokens_error_unit(input: Option<usize>) -> usize {
    let unsafe_value = input.unwrap();
    let _ = input.unwrap(); // [TEMP_ALLOWED]
    unsafe_value
}
