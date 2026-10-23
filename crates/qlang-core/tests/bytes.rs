//! `bytes`: a sequence of bytes that never changes.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn run_with(host: &mut MemoryHost, src: &str) -> String {
    match compile("main.q", src, host) {
        Ok(p) => {
            let r = p.run(host, Limits::default());
            match r.error {
                Some(d) => format!("RUNTIME {}", d.code),
                None => std::mem::take(&mut host.output),
            }
        }
        Err(e) => {
            let mut codes: Vec<String> = e.diagnostics.iter().map(|d| d.code.to_string()).collect();
            codes.sort();
            codes.join(",")
        }
    }
}

fn run(src: &str) -> String {
    let mut host = MemoryHost {
        seed: Some(7),
        ..MemoryHost::default()
    };
    run_with(&mut host, src)
}

const SOME: &str = "fun some(b: bytes?) -> bytes
  if b == none then
    return bytes.empty()
  end
  b
end
";

#[test]
fn text_becomes_bytes_and_back() {
    let out = run("let b = \"héllo\".to_bytes()
print(b.len())
print(b.hex())
print(b.to_string())
print(b.slice(0, 3))
print(b.at(1))
print(b.at(99) == none)
print(b.find(\"llo\".to_bytes()))
print(b.contains(\"zz\".to_bytes()))
print(b.starts_with(\"hé\".to_bytes()))
print(b == \"héllo\".to_bytes())
print(b.concat(b).len())
");
    assert_eq!(
        out,
        "6\n68c3a96c6c6f\nhéllo\nbytes[68 c3 a9]\n195\ntrue\n3\nfalse\ntrue\ntrue\n12\n"
    );
}

#[test]
fn bytes_that_are_not_text_have_no_string() {
    let out = run(&format!(
        "{SOME}let b = some(bytes.from_array([255, 254]))
print(b.to_string() == none)
print(b.to_array())
print(bytes.from_array([256]) == none)
print(bytes.from_array([-1]) == none)
"
    ));
    assert_eq!(out, "true\n[255, 254]\ntrue\ntrue\n");
}

#[test]
fn encodings_and_hashes_work_on_bytes() {
    let out = run(&format!(
        "{SOME}let b = some(Base64.decode_bytes(\"AQL/\"))
print(b.to_array())
print(Base64.encode_bytes(b))
print(Base64.encode_url_bytes(b))
print(Hex.encode_bytes(b))
print(some(Hex.decode_bytes(\"0102ff\")) == b)
print(Url.encode_bytes(b))
print(Hash.sha256_bytes(\"abc\".to_bytes()) == Hash.sha256(\"abc\"))
print(Hash.sha256_raw(\"abc\".to_bytes()).len())
print(Hash.hmac_sha256_bytes(\"k\".to_bytes(), \"m\".to_bytes()) == Hash.hmac_sha256(\"k\", \"m\"))
print(Hash.hmac_sha256_raw(\"k\".to_bytes(), \"m\".to_bytes()).hex() == Hash.hmac_sha256(\"k\", \"m\"))
print(Base64.decode_bytes(\"***\") == none)
"
    ));
    assert_eq!(
        out,
        "[1, 2, 255]\nAQL/\nAQL_\n0102ff\ntrue\n%01%02%FF\ntrue\n32\ntrue\ntrue\ntrue\n"
    );
}

#[test]
fn random_bytes_follow_the_seed_and_are_limited() {
    let a = run("print(random.bytes(8).hex())\n");
    let b = run("print(random.bytes(8).hex())\n");
    assert_eq!(a, b, "same seed, same bytes");
    assert_eq!(a.trim().len(), 16);
    assert_eq!(run("print(random.bytes(-1).len())\n"), "RUNTIME R017");
    assert_eq!(run("print(random.bytes(2000000).len())\n"), "RUNTIME R017");
}

#[test]
fn json_carries_bytes_as_base64() {
    let out = run(&format!(
        "{SOME}struct Doc
  public name: string
  public data: bytes
end
let d = Doc {{ name: \"a\", data: \"hi\".to_bytes() }}
let j = Json.of(d)
print(j.to_text())
let back = Json.to<Doc>(j).unwrap()
print(back.data == d.data)
"
    ));
    assert_eq!(out, "{\"name\":\"a\",\"data\":\"aGk=\"}\ntrue\n");
}

#[test]
fn files_can_be_read_and_written_as_bytes() {
    let mut files = std::collections::HashMap::new();
    files.insert("a.txt".to_string(), "héllo".to_string());
    let mut host = MemoryHost {
        fs: Some(files),
        ..MemoryHost::default()
    };
    let out = run_with(
        &mut host,
        "fun some(b: bytes?) -> bytes
  if b == none then
    return bytes.empty()
  end
  b
end
let r = fs.read_bytes(\"a.txt\")
print(r.is_ok())
print(r.unwrap().len())
print(fs.write_bytes(\"b.txt\", \"ok\".to_bytes()).is_ok())
print(fs.write_bytes(\"c.bin\", some(bytes.from_array([255]))).is_err())
",
    );
    assert_eq!(out, "true\n6\ntrue\ntrue\n");
    assert_eq!(
        host.fs
            .as_ref()
            .and_then(|f| f.get("b.txt"))
            .map(String::as_str),
        Some("ok")
    );
}

#[test]
fn bytes_mistakes_are_reported() {
    assert_eq!(run("let b: bytes = \"x\"\n"), "T002");
    assert_eq!(run("let a = \"x\".to_bytes()\nprint(a < a)\n"), "T005");
    assert_eq!(run("print(\"x\".to_bytes().nope())\n"), "T161");
    assert_eq!(run("let bytes = 3\nprint(bytes)\n"), "3\n");
}
