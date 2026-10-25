//! `Log.info(message, context)`: the context, a value of any type, is written as JSON.

use std::process::Command;

#[test]
fn the_context_is_written_as_json_and_the_levels_follow_psr_3() {
    let dir = std::env::temp_dir().join(format!("qlang-log-ctx-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.q");
    std::fs::write(
        &file,
        "struct Order\n  public id: int\nend\nLog.info(\"plain\")\nLog.warning(\"slow\", {\"ms\": 900})\nLog.error(\"failed\", Order { id: 42 })\nLog.notice(\"seen\")\nLog.critical(\"down\", [1, 2])\n",
    )
    .unwrap();
    let run = |extra: &[&str]| {
        let o = Command::new(env!("CARGO_BIN_EXE_qlang"))
            .arg("run")
            .arg(&file)
            .args(extra)
            .output()
            .unwrap();
        String::from_utf8_lossy(&o.stderr).into_owned()
    };
    let text = run(&[]);
    assert!(text.contains("INFO  plain\n"), "{text}");
    assert!(text.contains("WARN  slow context={\"ms\":900}"), "{text}");
    assert!(text.contains("ERROR failed context={\"id\":42}"), "{text}");
    assert!(text.contains("CRITICAL down context=[1,2]"), "{text}");
    let json = run(&["--log-format", "json", "--log-level", "warning"]);
    assert!(!json.contains("plain") && !json.contains("seen"), "{json}");
    assert!(
        json.contains("\"context\":{\"ms\":900}") && json.contains("\"level\":\"warning\""),
        "{json}"
    );
    assert!(json.contains("\"context\":[1,2]"), "{json}");
}
