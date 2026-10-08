use qlang_core::check::compile;
use qlang_core::host::MemoryHost;

fn dir() -> std::path::PathBuf {
    std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples"))
}

#[test]
fn tour_checks() {
    let mut host = MemoryHost::new();
    {
        let f = "tour_math.q";
        host.files.insert(f.to_string(), std::fs::read_to_string(dir().join(f)).unwrap());
    }
    let src = std::fs::read_to_string(dir().join("tour.q")).unwrap();
    match compile("tour.q", &src, &mut host) {
        Ok(_) => {}
        Err(e) => panic!("\n{}", e.render()),
    }
}
