//! A program that serves requests: top-level code runs once, `handle` runs for each request.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};
use qlang_core::interp::AppRequest;

fn get(url: &str) -> AppRequest {
    AppRequest {
        method: "GET".into(),
        url: url.into(),
        ..Default::default()
    }
}

/// Runs `f` with a started app, on a big stack.
fn with_app<T: Send + 'static>(
    src: &'static str,
    limits: Limits,
    f: impl FnOnce(Result<qlang_core::interp::App<'_>, String>) -> T + Send + 'static,
) -> T {
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let mut host = MemoryHost::new();
            let prog = match compile("app.q", src, &mut host) {
                Ok(p) => p,
                Err(e) => return f(Err(format!("COMPILE: {}", e.render()))),
            };
            let app = prog
                .start_app(&mut host, limits)
                .map_err(|d| format!("{}: {}", d.code, d.message));
            f(app)
        })
        .unwrap()
        .join()
        .unwrap()
}

const COUNTER: &str = r#"
let hits = 0
print("started")

fun handle(req: Request) -> Response
  hits += 1
  if req.url == "/boom" then
    panic("boom")
  end
  if req.url == "/loop" then
    while true do
    end
  end
  Response { status: 200, headers: { "content-type": "text/plain" }, body: "{req.method} {req.url} #{hits}" }
end
"#;

#[test]
fn state_is_kept_between_requests_and_top_level_runs_once() {
    with_app(COUNTER, Limits::default(), |app| {
        let mut app = app.unwrap();
        let a = app.handle(get("/a")).unwrap();
        assert_eq!((a.status, a.body.as_str()), (200, "GET /a #1"));
        assert_eq!(
            a.headers,
            vec![("content-type".to_string(), "text/plain".to_string())]
        );
        let b = app.handle(get("/b")).unwrap();
        assert_eq!(b.body, "GET /b #2");
    });
}

#[test]
fn the_request_carries_headers_and_body() {
    let src = r#"
fun handle(req: Request) -> Response
  let agent = req.headers.get("user-agent", "none")
  let body = req.body
  let text = if body == none then "no body" else body end
  Response { status: 201, body: "{agent}|{text}" }
end
"#;
    with_app(src, Limits::default(), |app| {
        let mut app = app.unwrap();
        let r = app
            .handle(AppRequest {
                method: "POST".into(),
                url: "/x".into(),
                headers: vec![("user-agent".into(), "test".into())],
                body: Some("hello".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!((r.status, r.body.as_str()), (201, "test|hello"));
        assert_eq!(app.handle(get("/")).unwrap().body, "none|no body");
    });
}

#[test]
fn an_error_in_one_request_does_not_stop_the_next() {
    with_app(
        COUNTER,
        Limits {
            max_steps: 10_000,
            ..Limits::default()
        },
        |app| {
            let mut app = app.unwrap();
            let boom = app.handle(get("/boom")).unwrap_err();
            assert_eq!(boom.code, "R050");
            // an infinite loop hits the step limit of this request only
            let looped = app.handle(get("/loop")).unwrap_err();
            assert_eq!(looped.code, "R900");
            let ok = app.handle(get("/after")).unwrap();
            assert_eq!(ok.body, "GET /after #3");
        },
    );
}

#[test]
fn the_handler_is_checked_before_serving() {
    with_app("print(1)", Limits::default(), |app| {
        assert!(app.err().unwrap().starts_with("R070"))
    });
    with_app(
        "fun handle(x: int) -> int\n  x\nend",
        Limits::default(),
        |app| assert!(app.err().unwrap().starts_with("R070")),
    );
    with_app(
        "fun handle(req: Request) -> string\n  \"x\"\nend",
        Limits::default(),
        |app| assert!(app.err().unwrap().starts_with("R070")),
    );
}

#[test]
fn a_runtime_error_at_start_is_reported() {
    with_app(
        "panic(\"no\")\nfun handle(req: Request) -> Response\n  Response { status: 200, body: \"\" }\nend",
        Limits::default(),
        |app| {
            assert!(app.err().unwrap().starts_with("R050"));
        },
    );
}

#[test]
fn a_bad_status_is_an_error() {
    let src = "fun handle(req: Request) -> Response\n  Response { status: 42, body: \"\" }\nend";
    with_app(src, Limits::default(), |app| {
        assert_eq!(app.unwrap().handle(get("/")).unwrap_err().code, "R071");
    });
}

#[test]
fn path_query_and_header_helpers() {
    let src = r#"
fun handle(req: Request) -> Response
  let who = req.query.get("name", "nobody")
  let agent = req.header("User-Agent")
  let a = if agent == none then "-" else agent end
  Response.text("{req.path}|{who}|{a}|{req.url}")
end
"#;
    with_app(src, Limits::default(), |app| {
        let mut app = app.unwrap();
        let r = app
            .handle(AppRequest {
                method: "GET".into(),
                url: "/hi%20there?name=Ada".into(),
                path: "/hi there".into(),
                query: vec![("name".into(), "Ada".into())],
                headers: vec![("user-agent".into(), "curl".into())],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(r.body, "/hi there|Ada|curl|/hi%20there?name=Ada");
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| k == "Content-Type" && v == "text/plain; charset=utf-8")
        );
        let bare = app.handle(get("/")).unwrap();
        assert_eq!(bare.body, "|nobody|-|/");
    });
}

#[test]
fn response_helpers() {
    let src = r#"
fun handle(req: Request) -> Response
  match req.url
    case "/html" then Response.html("<p>x</p>")
    case "/json" then Response.json("\{\"ok\": true\}")
    case "/go" then Response.redirect("/there")
    case "/created" then Response.text("made").with_status(201).with_header("X-Id", "7")
    else Response.not_found()
  end
end
"#;
    with_app(src, Limits::default(), |app| {
        let mut app = app.unwrap();
        let header = |r: &qlang_core::interp::AppResponse, name: &str| {
            r.headers
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        };
        let html = app.handle(get("/html")).unwrap();
        assert_eq!(
            (html.status, header(&html, "Content-Type").unwrap()),
            (200, "text/html; charset=utf-8".to_string())
        );
        let json = app.handle(get("/json")).unwrap();
        assert_eq!(
            (json.body.as_str(), header(&json, "Content-Type").unwrap()),
            (
                "{\"ok\": true}",
                "application/json; charset=utf-8".to_string()
            )
        );
        let go = app.handle(get("/go")).unwrap();
        assert_eq!(
            (go.status, header(&go, "Location").unwrap()),
            (302, "/there".to_string())
        );
        let made = app.handle(get("/created")).unwrap();
        assert_eq!(
            (
                made.status,
                made.body.as_str(),
                header(&made, "X-Id").unwrap()
            ),
            (201, "made", "7".to_string())
        );
        assert_eq!(app.handle(get("/zzz")).unwrap().status, 404);
    });
}

fn at(method: &str, path: &str) -> AppRequest {
    AppRequest {
        method: method.into(),
        url: path.into(),
        path: path.into(),
        ..Default::default()
    }
}

const ROUTES: &str = r#"
let router = Router.new()
router.get("/", fun(req: Request) -> Response Response.text("home") end)
router.get("/items/:id", fun(req: Request) -> Response
  let id = req.params.get("id", "?")
  Response.text("item {id}")
end).post("/items", fun(req: Request) -> Response Response.text("created").with_status(201) end)
router.get("/users/:user/posts/:post", fun(req: Request) -> Response
  let user = req.params.get("user", "?")
  let post = req.params.get("post", "?")
  Response.text("{user}/{post}")
end)
router.get("/files/*", fun(req: Request) -> Response
  let rest = req.params.get("*", "?")
  Response.text("file {rest}")
end)
router.delete("/items/:id", fun(req: Request) -> Response Response.text("gone") end)

fun handle(req: Request) -> Response
  router.handle(req)
end
"#;

#[test]
fn the_router_sends_requests_to_matching_routes() {
    with_app(ROUTES, Limits::default(), |app| {
        let mut app = app.unwrap();
        let mut ask = |m: &str, p: &str| {
            let r = app.handle(at(m, p)).unwrap();
            (r.status, r.body)
        };
        assert_eq!(ask("GET", "/"), (200, "home".into()));
        assert_eq!(ask("GET", "/items/42"), (200, "item 42".into()));
        assert_eq!(ask("POST", "/items"), (201, "created".into()));
        assert_eq!(ask("DELETE", "/items/7"), (200, "gone".into()));
        assert_eq!(ask("GET", "/users/ada/posts/9"), (200, "ada/9".into()));
        // a final `*` takes the rest of the path
        assert_eq!(
            ask("GET", "/files/a/b/c.txt"),
            (200, "file a/b/c.txt".into())
        );
        assert_eq!(ask("GET", "/files/x"), (200, "file x".into()));
        assert_eq!(ask("GET", "/files/"), (200, "file ".into()));
        // what does not match
        assert_eq!(ask("GET", "/nothing").0, 404);
        assert_eq!(
            ask("GET", "/items/").0,
            404,
            "an empty segment is not a parameter"
        );
        assert_eq!(ask("GET", "/items/1/extra").0, 404);
        assert_eq!(ask("GET", "/users/ada/posts").0, 404);
        assert_eq!(ask("GET", "/files").0, 404);
    });
}

#[test]
fn a_known_path_with_another_method_is_a_405_with_the_allowed_methods() {
    with_app(ROUTES, Limits::default(), |app| {
        let mut app = app.unwrap();
        let r = app.handle(at("PUT", "/items/3")).unwrap();
        assert_eq!(r.status, 405);
        let allow = r
            .headers
            .iter()
            .find(|(k, _)| k == "Allow")
            .map(|(_, v)| v.clone());
        assert_eq!(allow.as_deref(), Some("GET, DELETE"));
        assert_eq!(app.handle(at("POST", "/")).unwrap().status, 405);
    });
}

#[test]
fn the_fallback_replaces_the_404() {
    let src = r#"
let router = Router.new().get("/", fun(req: Request) -> Response Response.text("home") end)
router.fallback(fun(req: Request) -> Response Response.text("nothing at {req.path}").with_status(404) end)
fun handle(req: Request) -> Response
  router.handle(req)
end
"#;
    with_app(src, Limits::default(), |app| {
        let mut app = app.unwrap();
        let r = app.handle(at("GET", "/lost")).unwrap();
        assert_eq!((r.status, r.body.as_str()), (404, "nothing at /lost"));
        assert_eq!(app.handle(at("GET", "/")).unwrap().body, "home");
    });
}

#[test]
fn matches_is_usable_on_its_own() {
    let src = r#"
let a = Router.matches("/items/:id", "/items/3")
let b = Router.matches("/items/:id", "/other/3")
if a != none then
  print(a.get("id", "?"))
end
print(b == none)
"#;
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let mut host = MemoryHost::new();
            let prog =
                compile("main.q", src, &mut host).unwrap_or_else(|e| panic!("{}", e.render()));
            let r = prog.run(&mut host, Limits::default());
            assert!(r.error.is_none());
            assert_eq!(host.output, "3\ntrue\n");
        })
        .unwrap()
        .join()
        .unwrap();
}
