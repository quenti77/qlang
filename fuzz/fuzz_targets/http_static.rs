#![no_main]
use libfuzzer_sys::fuzz_target;
use std::path::PathBuf;
use std::sync::OnceLock;

// `--static DIR` must never hand out a file outside DIR, whatever the path.
fuzz_target!(|data: &[u8]| {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    let dir = DIR.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("qlang-fuzz-static-{}", std::process::id()));
        std::fs::create_dir_all(root.join("public/sub")).unwrap();
        std::fs::write(root.join("public/index.html"), "<p>hi</p>").unwrap();
        std::fs::write(root.join("public/sub/a.txt"), "a").unwrap();
        std::fs::write(root.join("secret.txt"), "secret").unwrap();
        root.join("public").canonicalize().unwrap()
    });
    if let Ok(path) = std::str::from_utf8(data) {
        assert!(qlang_cli::serve::fuzz_static_path(dir, path), "escaped with {path:?}");
    }
});
