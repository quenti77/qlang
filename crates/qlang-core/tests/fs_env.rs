//! `fs.read`/`fs.write` and `env.get`/`env.args`: the host owns the disk and the process
//! environment; a missing file or variable is never a crash.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};
use std::collections::HashMap;

/// Output of the program, or `CODE: message` (compile or run error).
fn exec(src: &str, mut host: MemoryHost) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
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
fn fs_read_needs_the_fs_capability() {
    let out = exec(r#"let r = fs.read("x.txt")"#, MemoryHost::default());
    assert!(out.starts_with("COMPILE: T250"), "{out}");
}

#[test]
fn fs_write_needs_the_fs_capability() {
    let out = exec(r#"let r = fs.write("x.txt", "hi")"#, MemoryHost::default());
    assert!(out.starts_with("COMPILE: T250"), "{out}");
}

#[test]
fn fs_read_returns_the_files_content() {
    let mut files = HashMap::new();
    files.insert("greeting.txt".to_string(), "hello".to_string());
    let host = MemoryHost {
        fs: Some(files),
        ..MemoryHost::default()
    };
    let out = exec(r#"print(fs.read("greeting.txt").unwrap())"#, host);
    assert_eq!(out, "hello\n");
}

#[test]
fn fs_read_of_a_missing_file_is_a_result_err_not_a_crash() {
    let host = MemoryHost {
        fs: Some(HashMap::new()),
        ..MemoryHost::default()
    };
    let out = exec(
        r#"
let r = fs.read("missing.txt")
print(r.is_err())
"#,
        host,
    );
    assert_eq!(out, "true\n");
}

#[test]
fn fs_write_then_read_round_trips() {
    let host = MemoryHost {
        fs: Some(HashMap::new()),
        ..MemoryHost::default()
    };
    let out = exec(
        r#"
fs.write("out.txt", "written").unwrap()
print(fs.read("out.txt").unwrap())
"#,
        host,
    );
    assert_eq!(out, "written\n");
}

#[test]
fn env_get_needs_the_env_capability() {
    let out = exec(r#"let r = env.get("HOME")"#, MemoryHost::default());
    assert!(out.starts_with("COMPILE: T250"), "{out}");
}

#[test]
fn env_get_returns_none_for_an_unset_variable() {
    let host = MemoryHost {
        env: Some(HashMap::new()),
        ..MemoryHost::default()
    };
    let out = exec(
        r#"
let v = env.get("NOPE")
print(v == none)
"#,
        host,
    );
    assert_eq!(out, "true\n");
}

#[test]
fn env_get_returns_a_set_variables_value() {
    let mut vars = HashMap::new();
    vars.insert("LANG".to_string(), "fr_FR".to_string());
    let host = MemoryHost {
        env: Some(vars),
        ..MemoryHost::default()
    };
    let out = exec(r#"print(env.get("LANG"))"#, host);
    assert_eq!(out, "fr_FR\n");
}

#[test]
fn env_args_returns_the_programs_own_arguments() {
    let host = MemoryHost {
        env: Some(HashMap::new()),
        cli_args: vec!["a".to_string(), "b".to_string()],
        ..MemoryHost::default()
    };
    let out = exec(r#"print(env.args())"#, host);
    assert_eq!(out, "[\"a\", \"b\"]\n");
}

#[test]
fn programs_that_never_use_fs_or_env_do_not_need_those_capabilities() {
    let out = exec(r#"print(1 + 1)"#, MemoryHost::default());
    assert_eq!(out, "2\n");
}

// ---- env.require / env.parse / env.optional

fn env_host(pairs: &[(&str, &str)]) -> MemoryHost {
    MemoryHost {
        env: Some(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        ),
        ..MemoryHost::default()
    }
}

#[test]
fn typed_env_reads_need_the_env_capability() {
    for src in [
        r#"let r = env.require<int>("X")"#,
        r#"let r = env.parse("X", 1)"#,
        r#"let r = env.optional<int>("X")"#,
    ] {
        let out = exec(src, MemoryHost::default());
        assert!(out.starts_with("COMPILE: T250"), "{src}: {out}");
    }
}

#[test]
fn require_reads_a_typed_value_or_says_what_is_wrong() {
    let host = env_host(&[("PORT", "5432"), ("BAD", "abc"), ("NAME", "db")]);
    let out = exec(
        r#"
print(env.require<int>("PORT").unwrap())
print(env.require<string>("NAME").unwrap())
match env.require<int>("NOPE")
    case Err(e) then print(e.message)
    else print("?")
end
match env.require<int>("BAD")
    case Err(e) then print("{e.name} {e.message}")
    else print("?")
end
"#,
        host,
    );
    assert_eq!(
        out,
        "5432\ndb\nthe environment variable NOPE is not set\n\
BAD the environment variable BAD has the value \"abc\", which is not valid\n"
    );
}

#[test]
fn parse_gives_the_default_only_when_the_variable_is_not_set() {
    let host = env_host(&[("PORT", "80"), ("BAD", "abc"), ("EMPTY", "")]);
    let out = exec(
        r#"
let a: int = env.parse("PORT", 3306).unwrap()
let b: int = env.parse("NOPE", 3306).unwrap()
print("{a} {b}")
print(env.parse("BAD", 1).is_err())
print(env.parse("EMPTY", 1).is_err())
"#,
        host,
    );
    assert_eq!(out, "80 3306\ntrue\ntrue\n");
}

#[test]
fn optional_is_none_when_not_set_and_an_error_when_not_valid() {
    let host = env_host(&[("SSL", "yes"), ("BAD", "maybe")]);
    let out = exec(
        r#"
let a: bool? = env.optional<bool>("SSL").unwrap()
let b: bool? = env.optional<bool>("NOPE").unwrap()
print("{a} {b}")
print(env.optional<bool>("BAD").is_err())
"#,
        host,
    );
    assert_eq!(out, "true none\ntrue\n");
}

#[test]
fn bool_values_are_the_documented_ones() {
    let host = env_host(&[
        ("A", "true"),
        ("B", " 1 "),
        ("C", "YES"),
        ("D", "On"),
        ("E", "false"),
        ("F", "0"),
        ("G", "No"),
        ("H", "OFF"),
        ("I", "2"),
        ("J", ""),
    ]);
    let out = exec(
        r#"
let names = ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J"]
for n in names do
    match env.optional<bool>(n)
        case Ok(v) then print("{n} {v}")
        case Err(e) then print("{n} invalid")
    end
end
"#,
        host,
    );
    assert_eq!(
        out,
        "A true\nB true\nC true\nD true\nE false\nF false\nG false\nH false\nI invalid\nJ invalid\n"
    );
}

#[test]
fn int_and_float_reject_an_empty_or_malformed_value() {
    let host = env_host(&[("I", "12"), ("F", "1.5"), ("E", ""), ("X", "1x")]);
    let out = exec(
        r#"
print(env.require<int>("I").unwrap())
print(env.require<float>("F").unwrap())
print(env.require<int>("E").is_err())
print(env.require<float>("X").is_err())
print(env.require<string>("E").unwrap() == "")
"#,
        host,
    );
    assert_eq!(out, "12\n1.5\ntrue\ntrue\ntrue\n");
}

#[test]
fn a_program_type_can_be_read_from_the_environment() {
    let host = env_host(&[("PORT", "8080"), ("LOW", "0")]);
    let out = exec(
        r#"
struct Port
    public n: int
end
impl ParsableFromString for Port
    public fun from_text(value: string) -> Self?
        let n = int.parse(value)
        if n == none or n < 1 or n > 65535 then
            return none
        end
        Port { n: n }
    end
end
print(env.require<Port>("PORT").unwrap().n)
print(env.require<Port>("LOW").is_err())
"#,
        host,
    );
    assert_eq!(out, "8080\ntrue\n");
}

#[test]
fn require_needs_its_type_but_parse_takes_it_from_the_default() {
    let out = exec(r#"let r = env.require("X")"#, env_host(&[]));
    assert!(out.starts_with("COMPILE: T171"), "{out}");
    let out = exec(r#"print(env.parse("X", 2.5).unwrap())"#, env_host(&[]));
    assert_eq!(out, "2.5\n");
}

#[test]
fn the_env_error_goes_through_question_mark_with_other_errors() {
    let host = env_host(&[("PORT", "80")]);
    let out = exec(
        r#"
fun config() -> Result<int, Error>
    let p = env.require<int>("PORT")?
    let q = env.require<int>("NOPE")?
    Ok(p + q)
end
match config()
    case Err(e) then print(e.message)
    else print("ok")
end
"#,
        host,
    );
    assert_eq!(out, "the environment variable NOPE is not set\n");
}
