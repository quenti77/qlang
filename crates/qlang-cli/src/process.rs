//! `Process.run`: starting a program on the machine, for the terminal host, when the user
//! allowed it (`--allow-process`). No shell is involved: the command and its arguments are
//! given to the system as they are. A program is not confined by `--root`.

use qlang_core::host::{HostError, HostValue};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The longest a program may be given: an hour.
const MAX_TIMEOUT_MS: i64 = 3_600_000;
/// What a program wrote is kept up to this many bytes on each stream.
const MAX_OUTPUT: u64 = 16 << 20;

fn text(v: Option<&HostValue>) -> Option<String> {
    match v {
        Some(HostValue::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

/// Stops the program and what it started: its whole process group on unix (`uv run python` starts
/// a second program), its tree on Windows.
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// `process_run(command, args, input, cwd, env, timeout_ms)` -> a map `{status, stdout, stderr}`.
pub fn run(args: &[HostValue]) -> Result<HostValue, HostError> {
    let command = text(args.first()).ok_or_else(|| HostError::Other("no command".into()))?;
    let argv: Vec<String> = match args.get(1) {
        Some(HostValue::Array(items)) => items.iter().filter_map(|a| text(Some(a))).collect(),
        _ => Vec::new(),
    };
    let input = text(args.get(2));
    let cwd = text(args.get(3));
    let env: Vec<(String, String)> = match args.get(4) {
        Some(HostValue::Map(pairs)) => pairs
            .iter()
            .filter_map(|(k, v)| Some((text(Some(k))?, text(Some(v))?)))
            .collect(),
        _ => Vec::new(),
    };
    let timeout_ms = match args.get(5) {
        Some(HostValue::Int(n)) => *n,
        _ => 60_000,
    };
    if !(1..=MAX_TIMEOUT_MS).contains(&timeout_ms) {
        return Err(HostError::Other(format!(
            "the delay of a program is from 1 to {MAX_TIMEOUT_MS} milliseconds (found {timeout_ms})"
        )));
    }
    let timeout = Duration::from_millis(timeout_ms as u64);
    let mut cmd = Command::new(&command);
    cmd.args(&argv).envs(env);
    if let Some(dir) = &cwd {
        cmd.current_dir(dir);
    }
    #[cfg(unix)]
    {
        // its own group, so that it can be stopped with what it started
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    }
    let mut child = cmd
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| HostError::Other(format!("cannot start `{command}`: {e}")))?;

    if let Some(text) = input {
        let mut stdin = child.stdin.take().expect("piped");
        // written from a thread: a program that fills its output before reading its input
        // must not block us
        std::thread::spawn(move || {
            let _ = stdin.write_all(text.as_bytes());
        });
    }
    let read = |mut stream: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = (&mut stream).take(MAX_OUTPUT).read_to_end(&mut buf);
            // keep draining, so the program is not stopped by a full pipe
            let _ = std::io::copy(&mut stream, &mut std::io::sink());
            buf
        })
    };
    let out = read(Box::new(child.stdout.take().expect("piped")));
    let err = read(Box::new(child.stderr.take().expect("piped")));

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if qlang_core::host::cancelled() => {
                kill_tree(&mut child);
                return Err(HostError::Other(format!(
                    "`{command}` was stopped: the task was cancelled"
                )));
            }
            Ok(None) if Instant::now() >= deadline => {
                kill_tree(&mut child);
                return Err(HostError::Other(format!(
                    "`{command}` was stopped after {timeout_ms} milliseconds"
                )));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => {
                return Err(HostError::Other(format!(
                    "cannot wait for `{command}`: {e}"
                )));
            }
        }
    };
    let stdout = String::from_utf8_lossy(&out.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&err.join().unwrap_or_default()).into_owned();
    Ok(HostValue::Map(vec![
        (
            HostValue::Str("status".into()),
            // -1 when a signal ended it
            HostValue::Int(status.code().map_or(-1, i64::from)),
        ),
        (HostValue::Str("stdout".into()), HostValue::Str(stdout)),
        (HostValue::Str("stderr".into()), HostValue::Str(stderr)),
    ]))
}
