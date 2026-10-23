//! `Db` on MySQL / MariaDB. Needs a server: set `QLANG_TEST_MYSQL` to a URL such as
//! `mysql://root:pw@127.0.0.1:3306/app` (CI and most machines have none: the test is then skipped).

use std::process::Command;

#[test]
fn mysql_rows_come_back_typed_and_transactions_roll_back() {
    let Ok(url) = std::env::var("QLANG_TEST_MYSQL") else {
        eprintln!("QLANG_TEST_MYSQL is not set: skipped");
        return;
    };
    let dir = std::env::temp_dir().join(format!("qlang-mysql-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.q");
    std::fs::write(
        &file,
        format!(
            "let db = Db.connect(\"{url}\").unwrap()
db.execute(\"DROP TABLE IF EXISTS qlang_people\", [])
db.execute(\"CREATE TABLE qlang_people (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(50), age INT, score DOUBLE, born DATE, at DATETIME)\", [])
print(db.execute(\"INSERT INTO qlang_people (name, age, score, born, at) VALUES (?, ?, ?, '2000-01-02', '2026-10-23 14:05:09')\", [Json.string(\"Ada\"), Json.number(36.0), Json.number(1.5)]).unwrap())
db.begin()
db.execute(\"INSERT INTO qlang_people (name) VALUES (?)\", [Json.string(\"Bob\")])
db.rollback()
let rows = db.query(\"SELECT name, age, score, born, at FROM qlang_people\", []).unwrap()
print(rows.len())
let r = rows[0]
print(r.get(\"name\", Json.null()).as_string())
print(r.get(\"age\", Json.null()).as_number())
print(r.get(\"score\", Json.null()).as_number())
print(r.get(\"born\", Json.null()).as_string())
print(r.get(\"at\", Json.null()).as_string())
print(db.query(\"SELEKT 1\", []).is_err())
db.execute(\"DROP TABLE qlang_people\", [])
db.close()
"
        ),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .arg("run")
        .arg(&file)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        text,
        "1\n1\nAda\n36.0\n1.5\n2000-01-02\n2026-10-23 14:05:09\ntrue\n"
    );
}
