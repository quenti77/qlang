//! `qlang lsp`: the language server, spoken to like an editor would.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

struct Client {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Value>,
    notes: Vec<Value>,
    next: i64,
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn uri(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    // `C:/dir` on Windows
    let slash = if s.starts_with('/') { "" } else { "/" };
    format!("file://{slash}{s}")
}

impl Client {
    fn start(root: &Path) -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_qlang"))
            .arg("lsp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let mut out = BufReader::new(child.stdout.take().unwrap());
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            loop {
                let mut n = 0usize;
                loop {
                    let mut line = String::new();
                    if out.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    let line = line.trim_end();
                    if line.is_empty() {
                        break;
                    }
                    if let Some(v) = line.strip_prefix("Content-Length:") {
                        n = v.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0u8; n];
                if out.read_exact(&mut body).is_err() {
                    return;
                }
                if tx.send(serde_json::from_slice(&body).unwrap()).is_err() {
                    return;
                }
            }
        });
        let mut c = Client {
            child,
            stdin,
            rx,
            notes: Vec::new(),
            next: 1,
        };
        let init = c.request(
            "initialize",
            json!({ "rootUri": uri(root), "capabilities": {} }),
        );
        assert_eq!(init["capabilities"]["hoverProvider"], json!(true));
        c.notify("initialized", json!({}));
        c
    }

    fn write(&mut self, msg: Value) {
        let body = msg.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        self.stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.write(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    /// The whole response (`result` or `error`).
    fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.write(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let m = self
                .rx
                .recv_timeout(Duration::from_secs(20))
                .expect("no answer from the server");
            if m["id"] == json!(id) {
                return m;
            }
            self.notes.push(m);
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let r = self.call(method, params);
        assert!(r.get("error").is_none(), "{method}: {r}");
        r["result"].clone()
    }

    /// The diagnostics last published for `file`, waiting for them if needed.
    fn diagnostics(&mut self, file: &str) -> Vec<Value> {
        loop {
            let found = self
                .notes
                .iter()
                .rposition(|n| {
                    n["method"] == "textDocument/publishDiagnostics"
                        && n["params"]["uri"].as_str().unwrap().ends_with(file)
                })
                .map(|i| self.notes.remove(i));
            if let Some(n) = found {
                self.notes.retain(|m| {
                    !(m["method"] == "textDocument/publishDiagnostics"
                        && m["params"]["uri"] == n["params"]["uri"])
                });
                return n["params"]["diagnostics"].as_array().unwrap().clone();
            }
            let m = self
                .rx
                .recv_timeout(Duration::from_secs(20))
                .expect("no diagnostics");
            self.notes.push(m);
        }
    }

    fn open(&mut self, path: &Path, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": {
                "uri": uri(path), "languageId": "qlang", "version": 1, "text": text,
            } }),
        );
    }

    fn at(&self, path: &Path, line: u64, character: u64) -> Value {
        json!({ "textDocument": { "uri": uri(path) }, "position": { "line": line, "character": character } })
    }
}

fn project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qlang-lsp-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const MAIN: &str = "--\" Adds one. --\"\nfun add(a: int) -> int\n  a + 1\nend\nstruct P\n  public x: int\nend\nlet p = P { x: 1 }\nlet v = add(p.x)\nprint(add(v))\n";

#[test]
fn hover_definition_references_and_rename() {
    let dir = project("nav");
    let main = dir.join("main.q");
    std::fs::write(&main, MAIN).unwrap();
    let mut c = Client::start(&dir);
    c.open(&main, MAIN);
    assert!(c.diagnostics("main.q").is_empty());

    // hover on a call: the signature and the documentation comment
    let h = c.request("textDocument/hover", c.at(&main, 8, 9));
    let text = h["contents"]["value"].as_str().unwrap();
    assert!(text.contains("fun add(a: int) -> int"), "{text}");
    assert!(text.contains("Adds one."), "{text}");

    // go to the definition of `add`
    let d = c.request("textDocument/definition", c.at(&main, 8, 9));
    assert_eq!(d["range"]["start"], json!({ "line": 1, "character": 4 }));

    // and of the field `x`
    let d = c.request("textDocument/definition", c.at(&main, 8, 15));
    assert_eq!(d["range"]["start"], json!({ "line": 5, "character": 9 }));

    // every use of `add`, the declaration included
    let mut params = c.at(&main, 1, 5);
    params["context"] = json!({ "includeDeclaration": true });
    let refs = c.request("textDocument/references", params);
    assert_eq!(refs.as_array().unwrap().len(), 3, "{refs}");

    // renaming it changes the three places
    let mut params = c.at(&main, 8, 9);
    params["newName"] = json!("plus");
    let edit = c.request("textDocument/rename", params);
    let edits = edit["changes"][uri(&main)].as_array().unwrap().clone();
    assert_eq!(edits.len(), 3, "{edit}");
    assert!(edits.iter().all(|e| e["newText"] == "plus"));

    // a keyword is not a name
    let mut params = c.at(&main, 8, 9);
    params["newName"] = json!("end");
    assert!(c.call("textDocument/rename", params).get("error").is_some());

    // the symbols of the file
    let syms = c.request(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri(&main) } }),
    );
    let names: Vec<&str> = syms
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"add") && names.contains(&"P"), "{names:?}");
}

#[test]
fn errors_follow_the_text_and_completion_knows_members() {
    let dir = project("edit");
    let main = dir.join("main.q");
    std::fs::write(&main, MAIN).unwrap();
    let mut c = Client::start(&dir);
    c.open(&main, MAIN);
    assert!(c.diagnostics("main.q").is_empty());

    let broken = MAIN.replace("add(p.x)", "add(\"no\")");
    c.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri(&main), "version": 2 },
                "contentChanges": [{ "text": broken }] }),
    );
    let diags = c.diagnostics("main.q");
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0]["severity"], json!(1));
    assert_eq!(diags[0]["range"]["start"]["line"], json!(8));

    // `p.` offers the fields of `P`
    let typing = MAIN.replace("add(p.x)", "add(p.)");
    c.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri(&main), "version": 3 },
                "contentChanges": [{ "text": typing }] }),
    );
    let items = c.request("textDocument/completion", c.at(&main, 8, 14));
    let labels: Vec<&str> = items
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap())
        .collect();
    assert!(labels.contains(&"x"), "{items}");

    // and a call shows its parameters
    let call = MAIN.replace("add(p.x)", "add(");
    c.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri(&main), "version": 4 },
                "contentChanges": [{ "text": call }] }),
    );
    let sig = c.request("textDocument/signatureHelp", c.at(&main, 8, 12));
    assert_eq!(sig["signatures"][0]["label"], json!("add(a: int) -> int"));
    assert_eq!(
        sig["signatures"][0]["parameters"][0]["label"],
        json!("a: int")
    );
}

#[test]
fn names_are_followed_across_files() {
    let dir = project("files");
    let lib = dir.join("lib.q");
    let main = dir.join("main.q");
    let lib_src = "fun double(n: int) -> int\n  n * 2\nend\nexport double\n";
    let main_src = "import double from \"lib.q\"\nprint(double(4))\n";
    std::fs::write(&lib, lib_src).unwrap();
    std::fs::write(&main, main_src).unwrap();
    let mut c = Client::start(&dir);
    c.open(&main, main_src);
    assert!(c.diagnostics("main.q").is_empty());

    let d = c.request("textDocument/definition", c.at(&main, 1, 8));
    assert_eq!(d["uri"], json!(uri(&lib)));
    assert_eq!(d["range"]["start"], json!({ "line": 0, "character": 4 }));

    // renaming from the definition reaches the other file (not open in the editor)
    let mut params = c.at(&lib, 0, 5);
    params["newName"] = json!("twice");
    let edit = c.request("textDocument/rename", params);
    let changes = edit["changes"].as_object().unwrap();
    assert!(changes.contains_key(&uri(&lib)), "{edit}");
    assert!(changes.contains_key(&uri(&main)), "{edit}");

    c.request("shutdown", Value::Null);
}

#[test]
fn rename_reaches_every_kind_of_use() {
    let dir = project("kinds");
    let main = dir.join("main.q");
    let src = "struct Point\n  public x: int\n  public y: int\nend\nimpl Point\n  public fun len(self) -> int\n    self.x + self.y\n  end\nend\nenum Color\n  Red\n  Green(n: int)\nend\nfun show(c: Color, p: Point) -> int\n  let total = p.len()\n  match c\n    case Red then total\n    case Green(n) then total + n\n  end\nend\nlet q: Point = Point { x: 1, y: 2 }\nprint(show(Color.Green(3), q))\nprint(show(Color.Red, q))\n";
    std::fs::write(&main, src).unwrap();
    let mut c = Client::start(&dir);
    c.open(&main, src);
    let d = c.diagnostics("main.q");
    assert!(d.is_empty(), "{d:?}");
    let count = |c: &mut Client, line: u64, col: u64| {
        let mut params = c.at(&main, line, col);
        params["newName"] = json!("renamed");
        let r = c.request("textDocument/rename", params);
        r["changes"][uri(&main)].as_array().map_or(0, |v| v.len())
    };
    assert_eq!(count(&mut c, 0, 8), 5, "the struct");
    assert_eq!(count(&mut c, 1, 10), 3, "the field");
    assert_eq!(count(&mut c, 5, 14), 2, "the method");
    assert_eq!(count(&mut c, 10, 3), 3, "the variant");
    assert_eq!(count(&mut c, 14, 7), 3, "the variable");
    assert_eq!(count(&mut c, 13, 9), 2, "the parameter");
    assert_eq!(count(&mut c, 9, 6), 4, "the enumeration");
}

#[test]
fn rename_refuses_the_shorthand_that_means_two_things() {
    let dir = project("shorthand");
    let main = dir.join("main.q");
    let src = "struct P\n  public x: int\nend\nlet x = 1\nlet p = P { x }\nprint(p.x + x)\n";
    std::fs::write(&main, src).unwrap();
    let mut c = Client::start(&dir);
    c.open(&main, src);
    let d = c.diagnostics("main.q");
    assert!(d.is_empty(), "{d:?}");
    for (line, col) in [(1, 10), (3, 4)] {
        let mut params = c.at(&main, line, col);
        params["newName"] = json!("z");
        let r = c.call("textDocument/rename", params);
        let msg = r["error"]["message"].as_str().unwrap_or("");
        assert!(msg.contains("shorthand"), "{r}");
    }
}

/// `app` depends on `tools`; the editor may have opened `app` itself or the folder around it.
fn app_with_package(name: &str) -> PathBuf {
    let dir = project(name);
    std::fs::create_dir_all(dir.join("tools")).unwrap();
    std::fs::write(
        dir.join("tools/qlang.toml"),
        "[package]\nname = \"tools\"\nmain = \"lib.q\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("tools/lib.q"),
        "fun twice(n: int) -> int\n  n * 2\nend\nexport twice\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("app/sub/deep")).unwrap();
    std::fs::write(
        dir.join("app/qlang.toml"),
        "[dependencies]\ntools = { path = \"../tools\" }\n",
    )
    .unwrap();
    dir
}

const USES_PACKAGE: &str = "import twice from \"@tools/lib.q\"\nprint(twice(2))\n";

#[test]
fn a_package_import_works_in_a_subfolder_of_the_project() {
    let dir = app_with_package("pkg-sub");
    let file = dir.join("app/sub/deep/a.q");
    std::fs::write(&file, USES_PACKAGE).unwrap();
    let mut c = Client::start(&dir.join("app"));
    c.open(&file, USES_PACKAGE);
    let d = c.diagnostics("a.q");
    assert!(d.is_empty(), "{d:?}");
}

#[test]
fn a_package_import_works_when_the_editor_opened_the_folder_around_the_project() {
    let dir = app_with_package("pkg-around");
    let file = dir.join("app/sub/deep/a.q");
    std::fs::write(&file, USES_PACKAGE).unwrap();
    let mut c = Client::start(&dir);
    c.open(&file, USES_PACKAGE);
    let d = c.diagnostics("a.q");
    assert!(d.is_empty(), "{d:?}");
    // and in the project's own folder
    let top = dir.join("app/b.q");
    std::fs::write(&top, USES_PACKAGE).unwrap();
    c.open(&top, USES_PACKAGE);
    assert!(c.diagnostics("b.q").is_empty());
}

fn labels(items: &Value) -> Vec<String> {
    items
        .as_array()
        .map(|a| {
            a.iter()
                .map(|i| i["label"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The completion at the end of `line`, in a file next to `helper.q` and the package `tools`.
fn complete_import(c: &mut Client, file: &Path, line: &str) -> Value {
    c.open(file, &format!("{line}\n"));
    let mut p = c.at(file, 0, line.chars().count() as u64);
    p["context"] = json!({ "triggerKind": 2, "triggerCharacter": "/" });
    c.request("textDocument/completion", p)
}

#[test]
fn imports_complete_packages_files_and_folders() {
    let dir = app_with_package("import-complete");
    std::fs::write(dir.join("tools/extra.q"), "export\n").unwrap();
    std::fs::create_dir_all(dir.join("tools/web")).unwrap();
    std::fs::write(dir.join("tools/web/app.q"), "\n").unwrap();
    let app = dir.join("app");
    std::fs::write(app.join("helper.q"), "fun h() -> int\n  1\nend\nexport h\n").unwrap();
    std::fs::create_dir_all(app.join("sub")).unwrap();
    std::fs::write(app.join("sub/inner.q"), "\n").unwrap();
    let main = app.join("main.q");
    std::fs::write(&main, "").unwrap();
    let mut c = Client::start(&dir);

    // the packages, by name
    let items = complete_import(&mut c, &main, "import h from \"@");
    assert_eq!(labels(&items), vec!["@tools/"]);
    // the inside of a package: files and folders (a folder goes on proposing)
    let items = complete_import(&mut c, &main, "import twice from \"@tools/");
    let l = labels(&items);
    assert!(
        l.contains(&"lib.q".to_string()) && l.contains(&"extra.q".to_string()),
        "{l:?}"
    );
    assert!(l.contains(&"web/".to_string()), "{l:?}");
    let folder = items
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["label"] == "web/")
        .unwrap();
    assert_eq!(folder["command"]["command"], "editor.action.triggerSuggest");
    let items = complete_import(&mut c, &main, "import \"@tools/web/");
    assert_eq!(labels(&items), vec!["app.q"]);
    // next to the file, without the file itself
    let items = complete_import(&mut c, &main, "import h from \"./");
    let l = labels(&items);
    assert!(
        l.contains(&"helper.q".to_string()) && l.contains(&"sub/".to_string()),
        "{l:?}"
    );
    assert!(!l.contains(&"main.q".to_string()), "{l:?}");
    let items = complete_import(&mut c, &main, "import \"./sub/");
    assert_eq!(labels(&items), vec!["deep/", "inner.q"]);
    // nothing typed: where to start
    let items = complete_import(&mut c, &main, "import \"");
    let l = labels(&items);
    assert!(
        l.contains(&"./".to_string()) && l.contains(&"@tools/".to_string()),
        "{l:?}"
    );
    // the replaced text is the part after the last slash
    let items = complete_import(&mut c, &main, "import \"@tools/li");
    let lib = items
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["label"] == "lib.q")
        .unwrap();
    assert_eq!(lib["textEdit"]["range"]["start"]["character"], 15);
}

#[test]
fn a_slash_outside_an_import_proposes_nothing() {
    let dir = project("slash");
    let main = dir.join("main.q");
    std::fs::write(&main, "").unwrap();
    let mut c = Client::start(&dir);
    c.open(&main, "let a = \"x/\"\n");
    let mut p = c.at(&main, 0, 12);
    p["context"] = json!({ "triggerKind": 2, "triggerCharacter": "/" });
    let r = c.request("textDocument/completion", p);
    assert!(
        r.is_null() || r.as_array().is_some_and(|a| a.is_empty()),
        "{r}"
    );
}

#[test]
fn formatting_returns_the_document_laid_out_and_nothing_when_it_already_is() {
    let dir = project("format");
    let main = dir.join("main.q");
    std::fs::write(&main, "").unwrap();
    let mut c = Client::start(&dir);
    c.open(
        &main,
        "fun f(x: int) -> int\nif x > 0 then\nreturn 1\nend\n0\nend\n",
    );
    let r = c.request(
        "textDocument/formatting",
        json!({ "textDocument": { "uri": uri(&main) }, "options": { "tabSize": 2, "insertSpaces": true } }),
    );
    let edits = r.as_array().unwrap();
    assert_eq!(edits.len(), 1, "{r}");
    assert_eq!(
        edits[0]["newText"],
        "fun f(x: int) -> int\n  if x > 0 then\n    return 1\n  end\n  0\nend\n"
    );
    assert_eq!(edits[0]["range"]["start"]["line"], 0);
    // a document that is already laid out needs no edit
    c.open(&main, "let a = 1\n");
    let r = c.request(
        "textDocument/formatting",
        json!({ "textDocument": { "uri": uri(&main) }, "options": {} }),
    );
    assert_eq!(r.as_array().unwrap().len(), 0, "{r}");
}

/// The labels of the completion at (`line`, `col`) of `src`, written to `name` in `dir`.
fn labels_at(
    c: &mut Client,
    dir: &Path,
    name: &str,
    src: &str,
    line: u64,
    col: u64,
) -> Vec<String> {
    let file = dir.join(name);
    std::fs::write(&file, src).unwrap();
    c.open(&file, src);
    let r = c.request("textDocument/completion", c.at(&file, line, col));
    r.as_array()
        .map(|a| {
            a.iter()
                .map(|i| i["label"].as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

const LIB: &str = "struct App\n  public n: int\nend\n\nimpl App\n  public fun new(items: array<string>) -> App\n    App { n: 0 }\n  end\n\n  public fun validate(self) -> array<string>\n    []\n  end\nend\n\nfun helper(a: int) -> int\n  a\nend\nexport App, helper\n";

#[test]
fn completion_works_after_an_operator_in_a_module_type_and_in_a_type_position() {
    let dir = project("complete-more");
    std::fs::write(dir.join("lib.q"), LIB).unwrap();
    let mut c = Client::start(&dir);
    let head = "import \"./lib.q\" as web\n\nfun build() -> web.App\n  let app = web.App.new([])\n  let p = app.validate()\n";
    // `not p.|`: the dot belongs to `p`, not to `not p`
    let l = labels_at(
        &mut c,
        &dir,
        "a.q",
        &format!("{head}  if not p. then\n  end\n  app\nend\n"),
        5,
        11,
    );
    assert!(l.contains(&"is_empty".to_string()), "{l:?}");
    let l = labels_at(
        &mut c,
        &dir,
        "b.q",
        &format!("{head}  let n = 1 + p.\n  app\nend\n"),
        5,
        17,
    );
    assert!(l.contains(&"len".to_string()), "{l:?}");
    // a type reached through a module
    let l = labels_at(
        &mut c,
        &dir,
        "c.q",
        &format!("{head}  let q = web.App.\n  app\nend\n"),
        5,
        18,
    );
    assert!(l.contains(&"new".to_string()), "{l:?}");
    // the module in a type position, where there is no expression at all
    let l = labels_at(
        &mut c,
        &dir,
        "d.q",
        "import \"./lib.q\" as web\n\nfun build() -> web.\n  web.App.new([])\nend\n",
        2,
        19,
    );
    assert!(l.contains(&"App".to_string()), "{l:?}");
    let l = labels_at(
        &mut c,
        &dir,
        "e.q",
        "import \"./lib.q\" as web\n\nfun build(app: web.) -> int\n  1\nend\n",
        2,
        19,
    );
    assert!(l.contains(&"App".to_string()), "{l:?}");
    // the functions of Reflect
    let l = labels_at(
        &mut c,
        &dir,
        "f.q",
        "fun f()\n  let x = Reflect.\nend\n",
        1,
        18,
    );
    assert!(
        l.contains(&"callables".to_string()) && l.contains(&"struct_info".to_string()),
        "{l:?}"
    );
}

#[test]
fn a_function_is_completed_with_its_parentheses_and_parameters() {
    let dir = project("complete-snippet");
    std::fs::write(dir.join("lib.q"), LIB).unwrap();
    let mut c = Client::start(&dir);
    let src = "import \"./lib.q\" as web\n\nlet x = web.\n";
    let file = dir.join("g.q");
    std::fs::write(&file, src).unwrap();
    c.open(&file, src);
    let r = c.request("textDocument/completion", c.at(&file, 2, 12));
    let helper = r
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["label"] == "helper")
        .unwrap();
    assert_eq!(helper["insertTextFormat"], 2);
    assert_eq!(helper["textEdit"]["newText"], "helper(${1:a})$0");
    // a type is inserted as it is
    let app = r
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["label"] == "App")
        .unwrap();
    assert_eq!(app["textEdit"]["newText"], "App");
    assert!(app.get("insertTextFormat").is_none());
    // a method without parameters gets empty parentheses
    let src = "import \"./lib.q\" as web\n\nlet app = web.App.new([])\nlet y = app.\n";
    std::fs::write(&file, src).unwrap();
    c.open(&file, src);
    let r = c.request("textDocument/completion", c.at(&file, 3, 12));
    let v = r
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["label"] == "validate")
        .unwrap();
    assert_eq!(v["textEdit"]["newText"], "validate()$0");
}

#[test]
fn completion_works_while_a_block_is_still_open() {
    let dir = project("complete-open");
    let mut c = Client::start(&dir);
    // `fun` without its `end`: the file does not parse yet
    let ok = labels_at(&mut c, &dir, "ok.q", "fun handle(req: R)\nend\n", 0, 17);
    assert!(ok.contains(&"Request".to_string()), "closed: {ok:?}");
    let l = labels_at(&mut c, &dir, "a.q", "fun handle(req: R)\n", 0, 17);
    assert!(l.contains(&"Request".to_string()), "{l:?}");
    // and a parenthesis too
    let l = labels_at(&mut c, &dir, "p.q", "fun handle(req: R\n", 0, 17);
    assert!(l.contains(&"Request".to_string()), "{l:?}");
    let l = labels_at(
        &mut c,
        &dir,
        "b.q",
        "fun handle(count: int) -> int\n  let total = 1\n  tot\n",
        2,
        5,
    );
    assert!(l.contains(&"total".to_string()), "{l:?}");
}
