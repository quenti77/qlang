//! `Redis` against a server. Set `QLANG_TEST_REDIS` to a URL such as `redis://127.0.0.1:6379/9`
//! (CI starts one; on a machine without any the test is skipped). The database is emptied first.

use std::process::Command;

fn run(name: &str, source: &str) -> (String, String, bool) {
    let dir = std::env::temp_dir().join(format!("qlang-redis-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.q");
    std::fs::write(&file, source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .arg("run")
        .arg(&file)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

#[test]
fn commands_lists_sorted_sets_locks_scripts_and_subscriptions() {
    let Ok(url) = std::env::var("QLANG_TEST_REDIS") else {
        eprintln!("QLANG_TEST_REDIS is not set: skipped");
        return;
    };
    let source = format!(
        r#"let r = Redis.connect("{url}").unwrap()
r.command(["FLUSHDB"]).unwrap()
print(r.set("k", "v").unwrap())
print(r.get("k").unwrap())
print(r.get("nope").unwrap() == none)
print(r.incr("n").unwrap() + r.incr("n").unwrap())
r.rpush("l", "a").unwrap()
r.rpush("l", "b").unwrap()
print(r.lrange("l", 0, -1).unwrap().join(","))
print(r.lpop("l").unwrap() ?? "none")
r.zadd("z", 5.0, "five").unwrap()
r.zadd("z", 1.0, "one").unwrap()
print(r.zrange_by_score("z", 0.0, 3.0, 10).unwrap().join(","))
print(r.set_if_absent("lock", "me", 10).unwrap())
print(r.set_if_absent("lock", "you", 10).unwrap())
let popped = r.blpop(["l"], 1).unwrap()
print(popped != none)
print(r.blpop(["empty"], 1).unwrap() == none)
print(r.eval("return redis.call('GET', KEYS[1]) .. ARGV[1]", ["k"], ["!"]).unwrap().as_string())
print(r.command(["LPUSH", "k", "x"]).is_err())
let sub = Redis.subscriber("{url}", ["news"]).unwrap()
print(sub.next(0).unwrap() == none)
print(r.publish("news", "hello").unwrap())
let m = sub.next(1000).unwrap()
if m != none then
  print(m.channel + " " + m.payload)
end
sub.psubscribe(["w.*"]).unwrap()
r.publish("w.x", "p").unwrap()
let p = sub.next(1000).unwrap()
if p != none then
  print((p.pattern ?? "?") + " " + p.channel + " " + p.payload)
end
print(sub.next(20).unwrap() == none)
r.command(["FLUSHDB"]).unwrap()
"#
    );
    let (out, err, ok) = run("all", &source);
    assert!(ok, "{err}");
    assert_eq!(
        out,
        "true\nv\ntrue\n3\na,b\na\none\ntrue\nfalse\ntrue\ntrue\nv!\ntrue\ntrue\n1\nnews hello\nw.* w.x p\ntrue\n"
    );
}

#[test]
fn a_remote_server_in_clear_is_refused_and_a_bad_url_too() {
    let (out, err, ok) = run(
        "refused",
        "let a = Redis.connect(\"redis://example.invalid\")
print(a.unwrap_err().message.contains(\"not on this machine\"))
let b = Redis.connect(\"http://x\")
print(b.is_err())
",
    );
    assert!(ok, "{err}");
    assert_eq!(out, "true\ntrue\n");
}
