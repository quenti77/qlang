//! Math methods on numbers, and the `Base64`, `Hex`, `Url` and `Hash` namespaces.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};

fn exec(src: &str) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let mut host = MemoryHost::default();
            let prog = match compile("main.q", &src, &mut host) {
                Ok(p) => p,
                Err(e) => {
                    return format!(
                        "COMPILE: {}",
                        e.diagnostics
                            .iter()
                            .map(|d| format!("{}: {}", d.code, d.message))
                            .collect::<Vec<_>>()
                            .join("; ")
                    );
                }
            };
            let r = prog.run(&mut host, Limits::default());
            match r.error {
                Some(d) => format!("{}: {}", d.code, d.message),
                None => host.output,
            }
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn trigonometry_and_logarithms() {
    let out = exec(
        "print((float.PI / 2.0).sin())\nprint(0.cos())\nprint(1.0.atan2(1.0) * 4.0 == float.PI)\nprint(100.log10())\nprint(8.log2())\nprint(81.log(3))\nprint(1.ln())\nprint(2.exp().to_fixed(5))\nprint(180.to_radians() == float.PI)\nprint(float.PI.to_degrees())\nprint(1.0.asin().to_fixed(4))\nprint(1.acos())",
    );
    assert_eq!(
        out,
        "1.0\n1.0\ntrue\n2.0\n3.0\n4.0\n0.0\n7.38906\ntrue\n180.0\n1.5708\n0.0\n"
    );
}

#[test]
fn rounding_sign_and_clamp() {
    let out = exec(
        "print((-7.9).trunc())\nprint(7.9.trunc())\nprint((-7).signum())\nprint(0.signum())\nprint((-2.5).signum())\nprint(15.clamp(0, 10))\nprint((-3).clamp(0, 10))\nprint(0.7.clamp(0.0, 0.5))\nprint(float.NAN.is_nan())\nprint(1.5.is_nan())\nprint(float.INFINITY.is_infinite())\nprint(float.NAN == float.NAN)",
    );
    assert_eq!(
        out,
        "-7\n7\n-1\n0\n-1.0\n10\n0\n0.5\ntrue\nfalse\ntrue\nfalse\n"
    );
}

#[test]
fn domain_errors_stop_the_program() {
    for (src, needle) in [
        ("print(0.ln())", "domain of `ln`"),
        ("print((-1.0).log10())", "domain of `log10`"),
        ("print(2.0.asin())", "domain of `asin`"),
        ("print(8.log(1))", "R016"),
        ("print(5.clamp(10, 0))", "R016"),
    ] {
        let out = exec(src);
        assert!(out.contains(needle), "{src}: {out}");
    }
}

#[test]
fn math_methods_are_typed() {
    assert!(exec("print(\"a\".sin())").starts_with("COMPILE:"));
    assert!(exec("print(1.5.clamp(0, 1))").starts_with("COMPILE:"));
    assert!(exec("let n: int = 1.sin()").starts_with("COMPILE:"));
}

#[test]
fn hashes_and_hmac() {
    let out = exec(
        "print(Hash.sha256(\"abc\"))\nprint(Hash.sha1(\"abc\"))\nprint(Hash.md5(\"\"))\nprint(Hash.hmac_sha256(\"key\", \"The quick brown fox jumps over the lazy dog\"))",
    );
    assert_eq!(
        out,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\na9993e364706816aba3e25717850c26c9cd0d89d\nd41d8cd98f00b204e9800998ecf8427e\nf7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8\n"
    );
}

#[test]
fn encodings_round_trip_and_bad_input_is_none() {
    let out = exec(
        "print(Base64.encode(\"hello, wörld\"))\nprint(Base64.decode(Base64.encode(\"hello, wörld\")))\nprint(Base64.encode_url(\"é>\"))\nprint(Base64.decode(\"w6k-\"))\nprint(Base64.decode(\"!!!\"))\nprint(Hex.encode(\"AZ\"))\nprint(Hex.decode(\"415a\"))\nprint(Hex.decode(\"4\"))\nprint(Url.encode(\"a b&c=é\"))\nprint(Url.decode(\"a%20b+c\"))\nprint(Url.decode(\"%zz\"))\nprint(Hex.decode(\"ff\"))",
    );
    assert_eq!(
        out,
        "aGVsbG8sIHfDtnJsZA==\nhello, wörld\nw6k-\né>\nnone\n415a\nAZ\nnone\na%20b%26c%3D%C3%A9\na b c\nnone\nnone\n"
    );
}

#[test]
fn the_namespaces_do_not_get_in_the_way_of_variables() {
    let out = exec(
        "let url = \"http://x\"\nlet hash = Hash.sha256(url)\nlet hex = Hex.encode(url)\nprint(url.upper())\nprint(hash.len())\nprint(hex.len())",
    );
    assert_eq!(out, "HTTP://X\n64\n16\n");
}
