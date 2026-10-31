//! `qlang dap`, driven like an editor would.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

fn read_message(from: &mut BufReader<ChildStdout>) -> Option<Value> {
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if from.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(v) = line.strip_prefix("Content-Length:") {
            length = v.trim().parse().ok()?;
        }
    }
    let mut body = vec![0u8; length];
    from.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

/// The canonical form the adapter reports (without Windows' `\\?\` prefix).
fn plain(path: std::path::PathBuf) -> std::path::PathBuf {
    let text = path.to_string_lossy().into_owned();
    match text.strip_prefix(r"\\?\") {
        Some(rest) => std::path::PathBuf::from(rest),
        None => path,
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

struct Client {
    child: Child,
    to: ChildStdin,
    from: std::sync::mpsc::Receiver<Value>,
    seq: i64,
    /// Events read while waiting for something else.
    events: Vec<Value>,
}

impl Client {
    fn start() -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_qlang"))
            .arg("dap")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let (send, from) = std::sync::mpsc::channel();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        // a thread reads the messages, so that a silent adapter fails the test instead of hanging it
        std::thread::spawn(move || {
            while let Some(m) = read_message(&mut stdout) {
                if send.send(m).is_err() {
                    break;
                }
            }
        });
        Client {
            to: child.stdin.take().unwrap(),
            from,
            child,
            seq: 0,
            events: Vec::new(),
        }
    }

    fn read(&mut self) -> Value {
        match self.from.recv_timeout(std::time::Duration::from_secs(30)) {
            Ok(m) => m,
            Err(e) => panic!(
                "the adapter said nothing for 30 s ({e}); read so far: {:?}",
                self.events
            ),
        }
    }

    fn request(&mut self, command: &str, arguments: Value) -> Value {
        self.seq += 1;
        let body =
            json!({"seq": self.seq, "type": "request", "command": command, "arguments": arguments})
                .to_string();
        write!(self.to, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        self.to.flush().unwrap();
        loop {
            let m = self.read();
            if m["type"] == "response" && m["request_seq"] == self.seq {
                assert_eq!(m["success"], true, "{command}: {m}");
                return m["body"].clone();
            }
            self.events.push(m);
        }
    }

    fn event(&mut self, name: &str) -> Value {
        loop {
            if let Some(i) = self.events.iter().position(|e| e["event"] == name) {
                return self.events.remove(i)["body"].clone();
            }
            let m = self.read();
            self.events.push(m);
        }
    }

    fn top(&mut self) -> (String, u64) {
        let b = self.request("stackTrace", json!({"threadId": 1}));
        let f = &b["stackFrames"][0];
        (
            f["name"].as_str().unwrap().to_string(),
            f["line"].as_u64().unwrap(),
        )
    }

    fn variables(&mut self, frame: u64) -> Vec<(String, String)> {
        let scopes = self.request("scopes", json!({"frameId": frame}));
        let r = scopes["scopes"][0]["variablesReference"].clone();
        let vars = self.request("variables", json!({"variablesReference": r}));
        vars["variables"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                (
                    v["name"].as_str().unwrap().to_string(),
                    v["value"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }
}

const PROGRAM: &str =
    "fun add(a: int, b: int) -> int\n  let sum = a + b\n  sum\nend\nlet x = add(1, 2)\nprint(x)\n";

#[test]
fn a_breakpoint_stops_the_program_and_the_steps_move_through_it() {
    let dir = std::env::temp_dir().join(format!("qlang-dap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.q");
    std::fs::write(&file, PROGRAM).unwrap();
    let file = file.canonicalize().map(plain).unwrap();

    let mut c = Client::start();
    let caps = c.request("initialize", json!({"adapterID": "qlang"}));
    assert_eq!(caps["supportsConfigurationDoneRequest"], true);
    c.event("initialized");
    c.request("launch", json!({"program": file.to_str().unwrap()}));
    let set = c.request(
        "setBreakpoints",
        json!({"source": {"path": file.to_str().unwrap()}, "breakpoints": [{"line": 2}]}),
    );
    assert_eq!(set["breakpoints"][0]["verified"], true);
    c.request("configurationDone", json!({}));

    // stopped inside `add`, before `sum` exists
    assert_eq!(c.event("stopped")["reason"], "breakpoint");
    assert_eq!(c.top(), ("add".to_string(), 2));
    let stack = c.request("stackTrace", json!({"threadId": 1}));
    assert_eq!(stack["stackFrames"][1]["name"], "(program)");
    assert_eq!(stack["stackFrames"][1]["line"], 5);
    assert_eq!(
        c.variables(0),
        [("a".into(), "1".into()), ("b".into(), "2".into())]
    );
    let value = c.request("evaluate", json!({"expression": "b", "frameId": 0}));
    assert_eq!(value["result"], "2");

    // one step: `sum` is there
    c.request("next", json!({"threadId": 1}));
    assert_eq!(c.event("stopped")["reason"], "step");
    assert_eq!(c.top().1, 3);
    assert!(c.variables(0).contains(&("sum".into(), "3".into())));

    // out of the function: back in the program, on the line after the call
    c.request("stepOut", json!({"threadId": 1}));
    c.event("stopped");
    assert_eq!(c.top(), ("(program)".to_string(), 6));

    c.request("continue", json!({"threadId": 1}));
    let out = c.event("output");
    assert_eq!(out["output"], "3\n");
    assert_eq!(c.event("exited")["exitCode"], 0);
    c.event("terminated");
    c.request("disconnect", json!({}));
    let _ = c.child.wait();
}

#[test]
fn stop_on_entry_and_step_in_follow_a_call() {
    let dir = std::env::temp_dir().join(format!("qlang-dap-in-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.q");
    std::fs::write(&file, PROGRAM).unwrap();
    let file = file.canonicalize().map(plain).unwrap();

    let mut c = Client::start();
    c.request("initialize", json!({}));
    c.event("initialized");
    c.request(
        "launch",
        json!({"program": file.to_str().unwrap(), "stopOnEntry": true}),
    );
    c.request("configurationDone", json!({}));
    c.event("stopped");
    assert_eq!(c.top(), ("(program)".to_string(), 5));
    c.request("stepIn", json!({"threadId": 1}));
    c.event("stopped");
    assert_eq!(c.top(), ("add".to_string(), 2));
    // leaving early ends the program quietly
    c.request("disconnect", json!({}));
    let _ = c.child.wait();
}

#[test]
fn a_program_that_fails_reports_it_and_ends() {
    let dir = std::env::temp_dir().join(format!("qlang-dap-err-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.q");
    std::fs::write(&file, "let a = [1]\nprint(a[3])\n").unwrap();
    let mut c = Client::start();
    c.request("initialize", json!({}));
    c.event("initialized");
    c.request("launch", json!({"program": file.to_str().unwrap()}));
    c.request("configurationDone", json!({}));
    let out = c.event("output");
    assert_eq!(out["category"], "stderr");
    assert_eq!(c.event("exited")["exitCode"], 2);
    c.event("terminated");
    c.request("disconnect", json!({}));
    let _ = c.child.wait();
}

#[test]
fn a_breakpoint_works_in_an_imported_file() {
    let dir = std::env::temp_dir().join(format!("qlang-dap-mod-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(
        dir.join("sub/lib.q"),
        "export twice\nfun twice(n: int) -> int\n  let r = n * 2\n  r\nend\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("main.q"),
        "import twice from \"sub/lib.q\"\nlet items = [1, 2]\nfor i in items do\n  print(twice(i))\nend\n",
    )
    .unwrap();
    let main = dir.join("main.q").canonicalize().map(plain).unwrap();
    let lib = dir.join("sub/lib.q").canonicalize().map(plain).unwrap();
    let mut c = Client::start();
    c.request("initialize", json!({}));
    c.event("initialized");
    c.request("launch", json!({"program": main.to_str().unwrap()}));
    c.request(
        "setBreakpoints",
        json!({"source": {"path": lib.to_str().unwrap()}, "breakpoints": [{"line": 4}]}),
    );
    c.request("configurationDone", json!({}));
    c.event("stopped");
    let stack = c.request("stackTrace", json!({"threadId": 1}));
    assert_eq!(stack["stackFrames"][0]["name"], "twice");
    assert_eq!(
        stack["stackFrames"][0]["source"]["path"],
        lib.to_str().unwrap()
    );
    assert_eq!(c.variables(0)[0], ("n".into(), "1".into()));
    // the second turn of the loop stops again, with the next item
    c.request("continue", json!({"threadId": 1}));
    c.event("stopped");
    assert_eq!(c.variables(0)[0], ("n".into(), "2".into()));
    let items = c.request("scopes", json!({"frameId": 1}));
    let locals = items["scopes"][0]["variablesReference"].clone();
    let vars = c.request("variables", json!({"variablesReference": locals}));
    let names: Vec<&str> = vars["variables"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"items"), "{names:?}");
    c.request("disconnect", json!({}));
    let _ = c.child.wait();
}
