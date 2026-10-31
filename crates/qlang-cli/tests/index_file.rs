//! The package index of the repository (`packages.json`) follows the rules of docs/registry.md.

#[test]
fn every_entry_of_the_index_is_listable() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages.json");
    let text = std::fs::read_to_string(path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let list = json.as_object().expect("the index is an object");
    assert!(!list.is_empty());
    for (name, e) in list {
        assert!(
            !name.is_empty()
                && name.len() <= 64
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
            "{name}: not a valid package name"
        );
        let git = e["git"]
            .as_str()
            .unwrap_or_else(|| panic!("{name}: no git"));
        assert!(
            git.starts_with("https://") && !git.ends_with(".git") && !git.contains('@'),
            "{name}: {git}"
        );
        for key in ["description", "license"] {
            assert!(
                e[key].as_str().is_some_and(|s| !s.trim().is_empty()),
                "{name}: no {key}"
            );
        }
        for key in e.as_object().unwrap().keys() {
            assert!(
                [
                    "git",
                    "description",
                    "license",
                    "keywords",
                    "yanked",
                    "removed"
                ]
                .contains(&key.as_str()),
                "{name}: unknown field {key}"
            );
        }
    }
}
