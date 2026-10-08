//! How much native stack does a deep (but allowed) recursion need?

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn run_on_stack(bytes: usize, depth: usize) -> bool {
    std::thread::Builder::new()
        .stack_size(bytes)
        .spawn(move || {
            let src = format!("fun d(n: int) -> int\nif n == 0 then return 0 end\n1 + d(n - 1)\nend\nprint(d({depth}))");
            let mut host = MemoryHost::new();
            let prog = compile("main.q", &src, &mut host).ok().unwrap();
            let r = prog.run(&mut host, Limits::default());
            r.error.is_none() && host.output.trim() == depth.to_string()
        })
        .unwrap()
        .join()
        .is_ok_and(|ok| ok)
}

#[test]
fn default_depth_fits_in_a_normal_thread_stack() {
    // The default limit is 1000 nested calls. A plain 8 MiB thread stack must hold them
    // in release builds; in debug builds frames are much larger, so only check a big stack.
    if cfg!(debug_assertions) {
        assert!(run_on_stack(256 << 20, 990));
    } else {
        assert!(run_on_stack(8 << 20, 990), "990 nested calls need more than 8 MiB of stack");
    }
}
