//! `fs.list`, `fs.is_dir`, `fs.glob` and the line readers (`fs.lines`, `each_line`, `read_lines`).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("qlang-files-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Dir(p)
    }

    fn write(&self, rel: &str, bytes: &[u8]) {
        let f = self.0.join(rel);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, bytes).unwrap();
    }

    fn run(&self, source: &str, extra: &[&str]) -> (String, String) {
        self.write("main.q", source.as_bytes());
        let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
            .arg("run")
            .args(extra)
            .arg("main.q")
            .current_dir(&self.0)
            .output()
            .unwrap();
        (
            String::from_utf8_lossy(&o.stdout).into_owned(),
            String::from_utf8_lossy(&o.stderr).into_owned(),
        )
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn list_is_dir_and_glob() {
    let d = Dir::new("glob");
    d.write("src/a.q", b"");
    d.write("src/b.txt", b"");
    d.write("src/deep/c.q", b"");
    d.write("docs/readme.md", b"");
    let (out, err) = d.run(
        r#"print(fs.list("src").unwrap().join(","))
print(fs.is_dir("src").unwrap())
print(fs.is_dir("src/a.q").unwrap())
print(fs.is_dir("nope").unwrap())
print(fs.list("nope").is_err())
print(fs.glob("src/*.q").unwrap().join(","))
print(fs.glob("src/**/*.q").unwrap().join(","))
print(fs.glob("**/*.md").unwrap().join(","))
print(fs.glob("src/?.txt").unwrap().join(","))
print(fs.glob("missing/*.q").unwrap().len())
print(fs.glob("./src/*.q").unwrap().join(","))
"#,
        &[],
    );
    assert_eq!(
        out,
        "a.q,b.txt,deep\ntrue\nfalse\nfalse\ntrue\nsrc/a.q\nsrc/a.q,src/deep/c.q\ndocs/readme.md\nsrc/b.txt\n0\n./src/a.q\n",
        "{err}"
    );
}

#[test]
fn lines_are_read_one_at_a_time() {
    let d = Dir::new("lines");
    d.write("f.txt", b"one\r\ntwo\n\nlast without break");
    d.write("empty.txt", b"");
    let (out, err) = d.run(
        r#"let r = fs.lines("f.txt").unwrap()
while true do
  let next = r.next().unwrap()
  if next == none then
    break
  end
  print("[{next}]")
end
print(r.next().is_err())
print(fs.read_lines("f.txt").unwrap().len())
let n = 0
print(fs.each_line("f.txt", fun(line: string) n += line.len() end).unwrap())
print(n)
print(fs.each_line("empty.txt", fun(line: string) print("never") end).unwrap())
print(fs.lines("nope.txt").is_err())
let early = fs.lines("f.txt").unwrap()
print(early.next().unwrap() == "one")
early.close()
print(early.next().is_err())
"#,
        &[],
    );
    assert_eq!(
        out, "[one]\n[two]\n[]\n[last without break]\ntrue\n4\n4\n24\n0\ntrue\ntrue\ntrue\n",
        "{err}"
    );
}

#[test]
fn bad_lines_and_limits() {
    let d = Dir::new("bad");
    d.write("bin.txt", b"ok\n\xff\xfe\nafter\n");
    d.write("a.txt", b"x\n");
    let (out, err) = d.run(
        r#"let r = fs.lines("bin.txt").unwrap()
print(r.next().unwrap() == "ok")
let bad = r.next()
print(bad.is_err())
print(bad.unwrap_err().message)
-- the file stays open after a bad line: the next one can be read
print(r.next().unwrap() == "after")
let held: array<Lines> = []
for i in 0..63 do
  held.push(fs.lines("a.txt").unwrap())
end
let over = fs.lines("a.txt")
print(over.is_err())
print(over.unwrap_err().message.contains("too many files"))
held[0].close()
print(fs.lines("a.txt").is_ok())
print(fs.lines(".").is_err())
"#,
        &[],
    );
    assert_eq!(
        out, "true\ntrue\nline 2 is not valid UTF-8\ntrue\ntrue\ntrue\ntrue\ntrue\n",
        "{err}"
    );
}

#[test]
fn root_confines_listing_and_reading() {
    let base = Dir::new("root");
    base.write("outside.txt", b"out\n");
    base.write("inside/a.txt", b"in\n");
    base.write(
        "inside/main.q",
        br#"print(fs.list(".").is_ok())
print(fs.list("..").is_err())
print(fs.lines("a.txt").is_ok())
print(fs.lines("../outside.txt").is_err())
print(fs.is_dir("..").is_err())
print(fs.glob("**/*.txt").unwrap().join(","))
"#,
    );
    let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .args(["run", "--root", ".", "main.q"])
        .current_dir(base.0.join("inside"))
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&o.stdout),
        "true\ntrue\ntrue\ntrue\ntrue\na.txt\n",
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
}

#[test]
fn transactions_nest_and_db_transaction_commits_or_rolls_back() {
    let d = Dir::new("tx");
    let (out, err) = d.run(
        r#"let db = Db.connect("sqlite://:memory:").unwrap()
db.execute("CREATE TABLE t (a INT)", []).unwrap()
fun total(db: Db) -> int
  db.query("SELECT a FROM t", []).unwrap().len()
end
print(total(db))
db.begin().unwrap()
db.execute("INSERT INTO t VALUES (1)", []).unwrap()
db.begin().unwrap()
db.execute("INSERT INTO t VALUES (2)", []).unwrap()
db.rollback().unwrap()
print(total(db))
db.commit().unwrap()
print(total(db))
let ok = db.transaction(fun() -> Result<string, Error>
  db.execute("INSERT INTO t VALUES (3)", []).unwrap()
  Result<string, Error>.Ok("kept")
end)
print(ok.unwrap())
print(total(db))
let failed = db.transaction(fun() -> Result<string, Error>
  db.execute("INSERT INTO t VALUES (4)", []).unwrap()
  Result<string, Error>.Err(Error { message: "nope" })
end)
print(failed.unwrap_err().message)
print(total(db))
-- inside another transaction only the inner part is undone
db.begin().unwrap()
db.execute("INSERT INTO t VALUES (5)", []).unwrap()
db.transaction(fun() -> Result<string, Error>
  db.execute("INSERT INTO t VALUES (6)", []).unwrap()
  Result<string, Error>.Err(Error { message: "inner" })
end)
db.commit().unwrap()
print(total(db))
"#,
        &[],
    );
    assert_eq!(out, "0\n1\n1\nkept\n2\nnope\n2\n3\n", "{err}");
}
