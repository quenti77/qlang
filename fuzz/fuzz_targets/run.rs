#![no_main]
use libfuzzer_sys::fuzz_target;
use qlang_core::host::{Limits, MemoryHost};

// A program that compiles runs to its end, to an error of its own or to a limit: never to a bug of the
// interpreter (`X999`). Small limits keep every run short.
fuzz_target!(|data: &[u8]| {
    if data.len() > 4096 {
        return;
    }
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let mut host = MemoryHost::new();
    let Ok(program) = qlang_core::check::compile("main.q", text, &mut host) else {
        return;
    };
    let limits = Limits {
        max_steps: 50_000,
        max_depth: 64,
        max_output: 1 << 16,
        max_alloc: 1 << 16,
    };
    let result = program.run(&mut host, limits);
    if let Some(d) = result.error {
        assert!(d.code != "X999", "internal error on:\n{text}\n{}", d.render(&program.sources));
    }
});
