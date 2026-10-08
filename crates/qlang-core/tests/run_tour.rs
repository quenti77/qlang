use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn dir() -> std::path::PathBuf {
    std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples"))
}

#[test]
fn tour_runs() {
    let handle = std::thread::Builder::new().stack_size(256 << 20).spawn(|| {
        let mut host = MemoryHost::new();
        host.files.insert("tour_math.q".into(), std::fs::read_to_string(dir().join("tour_math.q")).unwrap());
        let src = std::fs::read_to_string(dir().join("tour.q")).unwrap();
        let prog = match compile("tour.q", &src, &mut host) {
            Ok(p) => p,
            Err(e) => panic!("\n{}", e.render()),
        };
        let r = prog.run(&mut host, Limits::default());
        println!("{}", host.output);
        if let Some(e) = r.error {
            panic!("\n{}", e.render(&prog.sources));
        }
    });
    handle.unwrap().join().unwrap();
}
