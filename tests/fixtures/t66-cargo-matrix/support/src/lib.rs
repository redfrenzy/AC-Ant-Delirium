pub fn target_label() -> &'static str {
    if cfg!(target_arch = "wasm32") {
        "wasm"
    } else {
        "native"
    }
}
