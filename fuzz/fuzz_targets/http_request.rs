#![no_main]
use libfuzzer_sys::fuzz_target;

// Whatever a client sends to `qlang serve`: request line, headers, bodies, chunks.
fuzz_target!(|data: &[u8]| {
    if data.len() <= 65536 {
        qlang_cli::serve::fuzz_request(data);
    }
});
