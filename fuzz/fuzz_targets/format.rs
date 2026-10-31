#![no_main]
use libfuzzer_sys::fuzz_target;

// The formatter keeps the tokens (it checks itself) and laying out its own output changes nothing.
fuzz_target!(|data: &[u8]| {
    if data.len() > 8192 {
        return;
    }
    if let Ok(text) = std::str::from_utf8(data) {
        if let Ok(once) = qlang_core::fmt::format(text) {
            let twice = qlang_core::fmt::format(&once).expect("formatted text formats again");
            assert_eq!(once, twice, "formatting is not stable on:\n{text}");
        }
    }
});
