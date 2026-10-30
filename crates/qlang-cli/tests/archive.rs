//! `Gzip`, `Zip`, `Tar` and `Zone` from a program.

use std::process::{Command, Stdio};

fn run(source: &str) -> (String, String) {
    let name = std::thread::current().name().unwrap_or("t").to_string();
    let dir = std::env::temp_dir().join(format!("qlang-archive-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.q"), source).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
        .args(["run", "main.q"])
        .current_dir(&dir)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

#[test]
fn archives_round_trip_in_memory() {
    let (out, err) = run(r#"
let packed = Gzip.compress("hello hello hello".to_bytes())
print(Gzip.decompress(packed).unwrap().to_string())
print(Gzip.decompress_max(packed, 5).is_err())
let files = [ArchiveEntry { name: "a/b.txt", data: "hi".to_bytes() }, ArchiveEntry { name: "c.txt", data: "yo".to_bytes() }]
for e in Zip.read(Zip.write(files).unwrap()).unwrap() do
  print("zip {e.name} {e.data.len()}")
end
for e in Tar.read(Tar.write_gz(files).unwrap()).unwrap() do
  print("tar {e.name} {e.data.to_string()}")
end
print(Zip.write([ArchiveEntry { name: "../x", data: "z".to_bytes() }]).is_err())
print(Zip.read("not a zip".to_bytes()).is_err())
"#);
    assert_eq!(
        out,
        "hello hello hello\ntrue\nzip a/b.txt 2\nzip c.txt 2\ntar a/b.txt hi\ntar c.txt yo\ntrue\ntrue\n",
        "{err}"
    );
}

#[test]
fn zones_know_summer_time_and_the_hours_that_do_not_exist() {
    let (out, err) = run(r#"
let paris = Zone.of("Europe/Paris").unwrap()
let summer = datetime.of(2026, 7, 1, 12, 0, 0)
let winter = datetime.of(2026, 1, 1, 12, 0, 0)
print("{paris.format(summer, "HH:mm")} {paris.abbreviation(summer)} {paris.offset_seconds(summer)}")
print("{paris.format(winter, "HH:mm")} {paris.abbreviation(winter)}")
print(paris.local(2026, 10, 11, 9, 0, 0))
print(paris.local(2026, 3, 29, 2, 30, 0))
print(paris.local(2026, 10, 25, 2, 30, 0))
print(Zone.of("Mars/Base").is_err())
print(Zone.of("America/New_York").unwrap().format(summer, "HH:mm"))
"#);
    assert_eq!(
        out,
        "14:00 CEST 7200\n13:00 CET\n2026-10-11T07:00:00Z\n2026-03-29T01:30:00Z\n2026-10-25T00:30:00Z\ntrue\n08:00\n",
        "{err}"
    );
}
