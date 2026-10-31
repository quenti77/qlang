#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let mut diags = Vec::new();
        let _ = qlang_core::parser::parse_module(text, 0, &mut diags);
    }
});
