//! `Process.run`: starting a program on the machine, for the terminal host, when the user
//! allowed it (`--allow-process`). No shell is involved: the command and its arguments are
//! given to the system as they are. A program is not confined by `--root`.

use qlang_core::host::{HostError, HostValue};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A program that runs longer than this is stopped.
const TIMEOUT: Duration = Duration::from_secs(60);
/// What a program wrote is kept up to this many bytes on each stream.
const MAX_OUTPUT: u64 = 16 << 20;

fn text(v: Option<&HostValue>) -> Option<String> {
    match v {
        Some(HostValue::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

/// `process_run(command, args, input)` -> a map `{status, stdout, stderr}`.
pub fn run(args: &[HostValue]) -> Result<HostValue, HostError> {
    let command = text(args.first()).ok_or_else(|| HostError::Other("no command".into()))?;
    let argv: Vec<String> = match args.get(1) {
        Some(HostValue::Array(items)) => items.iter().filter_map(|a| text(Some(a))).collect(),
        _ => Vec::new(),
    };
    let input = text(args.get(2));
    let mut child = Command::new(&command)
        .args(&argv)
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

    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(HostError::Other(format!(
                    "`{command}` was stopped after {} seconds",
                    TIMEOUT.as_secs()
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
