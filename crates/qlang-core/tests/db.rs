//! `Db.connect`/`query`/`execute`/`close`: the host runs the statement; a connection or
//! query failure becomes a `Result::err`, never a crash.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost, SqlRow, SqlValue};
use std::collections::HashMap;

fn host_with(responses: HashMap<String, Result<Vec<SqlRow>, String>>) -> MemoryHost {
    MemoryHost { sql: Some(responses), ..MemoryHost::default() }
}

fn canned(sql: &str, result: Result<Vec<SqlRow>, String>) -> MemoryHost {
    let mut m = HashMap::new();
    m.insert(sql.to_string(), result);
    host_with(m)
}

fn row(columns: &[(&str, SqlValue)]) -> SqlRow {
    SqlRow { columns: columns.iter().map(|(n, v)| (n.to_string(), v.clone())).collect() }
}

/// Output of the program, or `CODE: message` (compile or run error).
fn exec(src: &str, mut host: MemoryHost) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let prog = match compile("main.q", &src, &mut host) {
                Ok(p) => p,
                Err(e) => return format!("COMPILE: {}", e.diagnostics.iter().map(|d| format!("{}: {}", d.code, d.message)).collect::<Vec<_>>().join("; ")),
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
fn db_connect_needs_the_sql_capability() {
    let out = exec(r#"let c = Db.connect("sqlite://x.db")"#, MemoryHost::default());
    assert!(out.starts_with("COMPILE: T250"), "{out}");
}

#[test]
fn query_returns_rows_as_maps_of_json() {
    let host = canned(
        "SELECT id, name FROM people",
        Ok(vec![row(&[("id", SqlValue::Int(1)), ("name", SqlValue::Text("Rex".to_string()))])]),
    );
    let out = exec(
        r#"
let db = Db.connect("sqlite://x.db").unwrap()
let rows = db.query("SELECT id, name FROM people", []).unwrap()
print(rows.len())
let r = rows[0]
print(r.get("id", Json.null()).as_number())
print(r.get("name", Json.null()).as_string())
"#,
        host,
    );
    assert_eq!(out, "1\n1.0\nRex\n");
}

#[test]
fn a_query_error_is_a_result_err_not_a_crash() {
    let host = canned("SELECT * FROM missing", Err("no such table: missing".to_string()));
    let out = exec(
        r#"
let db = Db.connect("sqlite://x.db").unwrap()
let r = db.query("SELECT * FROM missing", [])
if r.is_err() then
    print(r.unwrap_err().message)
end
"#,
        host,
    );
    assert_eq!(out, "no such table: missing\n");
}

#[test]
fn execute_returns_rows_affected() {
    let host = canned("DELETE FROM people", Ok(vec![row(&[]), row(&[])]));
    let out = exec(
        r#"
let db = Db.connect("sqlite://x.db").unwrap()
print(db.execute("DELETE FROM people", []).unwrap())
"#,
        host,
    );
    assert_eq!(out, "2\n");
}

#[test]
fn bind_parameters_are_converted_from_json_values() {
    let host = host_with(HashMap::new());
    let out = exec(
        r#"
let db = Db.connect("sqlite://x.db").unwrap()
print(db.query("SELECT 1", [Json.number(1.0), Json.string("x"), Json.boolean(true)]).unwrap().len())
"#,
        host,
    );
    assert_eq!(out, "0\n");
}

#[test]
fn begin_commit_and_rollback_send_the_matching_sql() {
    let mut m = HashMap::new();
    m.insert("BEGIN".to_string(), Ok(vec![]));
    m.insert("COMMIT".to_string(), Ok(vec![]));
    m.insert("ROLLBACK".to_string(), Ok(vec![]));
    let host = host_with(m);
    let out = exec(
        r#"
let db = Db.connect("sqlite://x.db").unwrap()
print(db.begin().unwrap())
print(db.commit().unwrap())
print(db.rollback().unwrap())
"#,
        host,
    );
    assert_eq!(out, "true\ntrue\ntrue\n");
}

#[test]
fn a_failed_commit_is_a_result_err_not_a_crash() {
    let mut m = HashMap::new();
    m.insert("BEGIN".to_string(), Ok(vec![]));
    m.insert("COMMIT".to_string(), Err("no transaction in progress".to_string()));
    let host = host_with(m);
    let out = exec(
        r#"
let db = Db.connect("sqlite://x.db").unwrap()
db.begin()
let r = db.commit()
if r.is_err() then
    print(r.unwrap_err().message)
end
"#,
        host,
    );
    assert_eq!(out, "no transaction in progress\n");
}

/// A regression guard: `begin`/`commit`/`rollback` are resolved natively (never written
/// as qlang calling `self.execute(...)` from inside the always-checked prelude), so a
/// program that never touches `Db` still compiles on a host without the `sql` capability.
/// Putting that call in the prelude's own source instead made every program need `sql`
/// to compile at all, since the prelude's bodies are checked unconditionally.
#[test]
fn programs_that_never_use_db_do_not_need_the_sql_capability() {
    let out = exec("print(1 + 1)\nlet j = Json.string(\"x\")\nprint(j.as_string())\n", MemoryHost::default());
    assert_eq!(out, "2\nx\n");
}

#[test]
fn a_query_after_close_is_a_result_err() {
    let host = host_with(HashMap::new());
    let out = exec(
        r#"
let db = Db.connect("sqlite://x.db").unwrap()
print(db.close().unwrap())
let r = db.query("SELECT 1", [])
if r.is_err() then
    print(r.unwrap_err().message)
end
"#,
        host,
    );
    assert_eq!(out, "true\nthis database connection is closed\n");
}
