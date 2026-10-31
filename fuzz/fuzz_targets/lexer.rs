#![no_main]
use libfuzzer_sys::fuzz_target;

// Any text is either tokens or diagnostics, never a panic.
fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let mut diags = Vec::new();
        let _ = qlang_core::lexer::lex(text, 0, 0, &mut diags);
    }
});
