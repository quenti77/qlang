//! `qlang doc --prelude`: the reference of what the language provides.

use serde_json::Value;
use std::process::Command;

fn reference() -> Value {
    let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .args(["doc", "--prelude"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).unwrap()
}

fn find<'a>(list: &'a Value, name: &str) -> &'a Value {
    list.as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .unwrap_or_else(|| panic!("no {name}"))
}

#[test]
fn it_lists_functions_builtin_methods_namespaces_and_prelude_types() {
    let r = reference();
    assert_eq!(r["version"], env!("CARGO_PKG_VERSION"));
    let print = find(&r["functions"], "print");
    assert_eq!(print["signature"], "print(value)");
    assert!(print["doc"].as_str().unwrap().contains("new line"));

    let array = find(&r["builtin"], "array");
    let pop = find(&array["methods"], "pop");
    assert_eq!(pop["signature"], "pop() -> T?");
    assert!(
        find(&r["builtin"], "string")["methods"]
            .as_array()
            .unwrap()
            .len()
            > 30
    );

    let hash = find(&r["namespaces"], "Hash");
    assert!(
        hash["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["name"] == "sha256")
    );

    let result = find(&r["types"], "Result");
    assert_eq!(result["kind"], "enum");
    assert!(result["variants"].as_array().unwrap().len() >= 2);
    assert!(
        find(&result["methods"], "is_ok")["doc"]
            .as_str()
            .unwrap()
            .len()
            > 5
    );
}

#[test]
fn every_builtin_method_and_function_is_explained() {
    let r = reference();
    for f in r["functions"].as_array().unwrap() {
        assert!(
            !f["doc"].as_str().unwrap().is_empty(),
            "function {}",
            f["name"]
        );
    }
    for t in r["builtin"].as_array().unwrap() {
        for m in t["methods"].as_array().unwrap() {
            assert!(
                !m["doc"].as_str().unwrap().is_empty(),
                "{}.{}",
                t["name"],
                m["name"]
            );
        }
    }
}
