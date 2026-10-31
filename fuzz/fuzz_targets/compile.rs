#![no_main]
use libfuzzer_sys::fuzz_target;
use qlang_core::host::MemoryHost;

// The whole front end. `X999` is the compiler's answer to a bug of its own: it must never come out.
fuzz_target!(|data: &[u8]| {
    if data.len() > 8192 {
        return;
    }
    if let Ok(text) = std::str::from_utf8(data) {
        let mut host = MemoryHost::new();
        if let Err(e) = qlang_core::check::compile("main.q", text, &mut host) {
            assert!(
                e.diagnostics.iter().all(|d| d.code != "X999"),
                "internal error on:\n{text}\n{}",
                e.render()
            );
        }
    }
});
