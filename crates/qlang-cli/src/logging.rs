//! `Log.debug/info/warn/error` of a program: one line per call on the standard error, as text
//! or as JSON, from a level up. Set with `--log-level` and `--log-format`, or the environment
//! variables `QLANG_LOG` and `QLANG_LOG_FORMAT`.

use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

static LEVEL: AtomicU8 = AtomicU8::new(1); // info
static JSON: AtomicBool = AtomicBool::new(false);

/// From the lowest; `warn` is another name for `warning`.
const LEVELS: [&str; 8] = [
    "debug",
    "info",
    "notice",
    "warning",
    "error",
    "critical",
    "alert",
    "emergency",
];

fn level_of(name: &str) -> Option<u8> {
    let name = if name == "warn" { "warning" } else { name };
    LEVELS.iter().position(|l| *l == name).map(|i| i as u8)
}

/// Reads the environment, then takes `--log-level X` and `--log-format X` out of `args`.
pub fn configure(args: &mut Vec<String>) -> Result<(), String> {
    if let Ok(v) = std::env::var("QLANG_LOG") {
        LEVEL.store(
            level_of(&v).ok_or(format!("QLANG_LOG: unknown level `{v}`"))?,
            Ordering::SeqCst,
        );
    }
    if let Ok(v) = std::env::var("QLANG_LOG_FORMAT") {
        set_format(&v)?;
    }
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--log-level" | "--log-format" => {
                let flag = args.remove(i);
                if i >= args.len() {
                    return Err(format!("{flag} needs a value"));
                }
                let value = args.remove(i);
                if flag == "--log-level" {
                    let n = level_of(&value).ok_or(format!(
                        "--log-level: `{value}` is not one of {}",
                        LEVELS.join(", ")
                    ))?;
                    LEVEL.store(n, Ordering::SeqCst);
                } else {
                    set_format(&value)?;
                }
            }
            // what follows `--` belongs to the program
            "--" => break,
            _ => i += 1,
        }
    }
    Ok(())
}

fn set_format(value: &str) -> Result<(), String> {
    match value {
        "text" => JSON.store(false, Ordering::SeqCst),
        "json" => JSON.store(true, Ordering::SeqCst),
        other => return Err(format!("log format: `{other}` is not `text` or `json`")),
    }
    Ok(())
}

/// Writes one line, if `level` is wanted. The `context` field, when there is one, holds JSON. `fields` are the extra `name=value` pairs.
pub fn write(time_ms: Option<i64>, level: &str, message: &str, fields: &[(String, String)]) {
    let n = level_of(level).unwrap_or(1);
    if n < LEVEL.load(Ordering::SeqCst) {
        return;
    }
    let when = time_ms.map(qlang_core::datetime::to_iso);
    let line = if JSON.load(Ordering::SeqCst) {
        let mut obj = serde_json::Map::new();
        if let Some(t) = &when {
            obj.insert("time".into(), t.clone().into());
        }
        obj.insert("level".into(), level.into());
        obj.insert("message".into(), message.into());
        for (k, v) in fields {
            // the `context` of `Log.info(message, context)` is JSON already
            let value = match (k.as_str(), serde_json::from_str::<serde_json::Value>(v)) {
                ("context", Ok(json)) => json,
                _ => v.clone().into(),
            };
            obj.entry(k.clone()).or_insert(value);
        }
        serde_json::Value::Object(obj).to_string()
    } else {
        let mut line = String::new();
        if let Some(t) = &when {
            line.push_str(t);
            line.push(' ');
        }
        let shown = if level == "warning" { "warn" } else { level };
        line.push_str(&format!("{:<5} {message}", shown.to_ascii_uppercase()));
        for (k, v) in fields {
            if k == "context" {
                line.push_str(&format!(" context={v}"));
            } else if v.contains(' ') || v.is_empty() {
                line.push_str(&format!(" {k}={v:?}"));
            } else {
                line.push_str(&format!(" {k}={v}"));
            }
        }
        line
    };
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{line}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_are_taken_out_and_checked() {
        let mut args: Vec<String> = ["run", "--log-level", "warn", "a.q", "--", "--log-level"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        configure(&mut args).unwrap();
        assert_eq!(args, ["run", "a.q", "--", "--log-level"]);
        assert_eq!(LEVEL.load(Ordering::SeqCst), 3);
        let mut bad: Vec<String> = vec!["--log-level".into(), "loud".into()];
        assert!(configure(&mut bad).is_err());
        LEVEL.store(1, Ordering::SeqCst);
    }
}
