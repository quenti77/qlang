//! `docs/errors.md` lists every diagnostic code the sources can emit, and only those.

use std::collections::BTreeSet;
use std::path::Path;

fn codes_in(text: &str, quoted: bool) -> BTreeSet<String> {
    let bytes = text.as_bytes();
    let mut out = BTreeSet::new();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        let w = &bytes[i..i + 4];
        let before_ok = if quoted {
            i > 0 && bytes[i - 1] == b'"' && bytes.get(i + 4) == Some(&b'"')
        } else {
            i > 0 && bytes[i - 1] == b'`' && bytes.get(i + 4) == Some(&b'`')
        };
        if before_ok && b"PTRLWQXJ".contains(&w[0]) && w[1..].iter().all(u8::is_ascii_digit) {
            out.insert(String::from_utf8_lossy(w).to_string());
        }
        i += 1;
    }
    out
}

fn walk(dir: &Path, out: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.extend(codes_in(&std::fs::read_to_string(&path).unwrap(), true));
        }
    }
}

#[test]
fn every_code_the_sources_emit_is_in_the_error_list_and_the_list_has_no_other() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut emitted = BTreeSet::new();
    for krate in ["qlang-core", "qlang-cli"] {
        walk(&root.join("crates").join(krate).join("src"), &mut emitted);
    }
    let doc = std::fs::read_to_string(root.join("docs/errors.md")).unwrap();
    let listed = codes_in(&doc, false);
    let missing: Vec<_> = emitted.difference(&listed).collect();
    let unknown: Vec<_> = listed.difference(&emitted).collect();
    assert!(
        missing.is_empty(),
        "emitted but not in docs/errors.md: {missing:?}"
    );
    assert!(
        unknown.is_empty(),
        "in docs/errors.md but never emitted: {unknown:?}"
    );
}
