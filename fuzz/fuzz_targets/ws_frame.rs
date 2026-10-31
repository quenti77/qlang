#![no_main]
use libfuzzer_sys::fuzz_target;

// WebSocket frames from the network.
fuzz_target!(|data: &[u8]| {
    let mut input = data;
    let _ = qlang_cli::ws::read_frame(&mut input);
});
