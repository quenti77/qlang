//! WebSockets seen from the program: `Response.websocket` and the callbacks the server drives.

use qlang_core::check::compile;
use qlang_core::host::{Limits, MemoryHost};
use qlang_core::interp::{AppRequest, WsIn, WsOut};

fn with_app<T: Send + 'static>(
    src: &'static str,
    f: impl FnOnce(qlang_core::interp::App<'_>) -> T + Send + 'static,
) -> T {
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || {
            let mut host = MemoryHost::new();
            let Ok(prog) = compile("app.q", src, &mut host) else {
                panic!("does not compile");
            };
            let Ok(app) = prog.start_app(&mut host, Limits::default()) else {
                panic!("does not start");
            };
            f(app)
        })
        .unwrap()
        .join()
        .unwrap()
}

fn request(path: &str) -> AppRequest {
    AppRequest {
        method: "GET".into(),
        url: path.into(),
        path: path.into(),
        headers: vec![("upgrade".into(), "websocket".into())],
        ..AppRequest::default()
    }
}

const CHAT: &str = r#"
let seen = 0

fun handle(req: Request) -> Response
  if req.path != "/chat" then
    return Response.text("plain")
  end
  Response.websocket(WebSocketHandler {
    tick_ms: 250,
    on_open: fun(c: WebSocket) c.send("hello {c.id} on {c.request.path}") end,
    on_message: fun(c: WebSocket, m: WsMessage)
      seen += 1
      if m.binary then
        c.send_bytes(m.data ?? bytes.empty())
      else
        let text = m.text ?? ""
        if text == "bye" then
          c.close_with(4001, "bye")
        else
          c.send("echo {text} #{seen}")
        end
      end
    end,
    on_tick: fun(c: WebSocket) c.send("tick") end,
    on_close: fun(c: WebSocket) seen += 100 end,
  })
end
"#;

#[test]
fn a_websocket_response_is_announced_and_driven_by_events() {
    with_app(CHAT, |mut app| {
        let plain = app.handle(request("/")).unwrap();
        assert!(plain.websocket.is_none());
        let r = app.handle(request("/chat")).unwrap();
        assert_eq!(r.status, 101);
        assert_eq!(r.websocket.unwrap().tick_ms, 250);
        let opened = app.ws_accept(1, "abc").unwrap();
        assert_eq!(opened, vec![(1, WsOut::Text("hello abc on /chat".into()))]);
        let echoed = app.ws_message(1, WsIn::Text("hi".into())).unwrap();
        assert_eq!(echoed, vec![(1, WsOut::Text("echo hi #1".into()))]);
        let bin = app.ws_message(1, WsIn::Binary(vec![1, 2, 3])).unwrap();
        assert_eq!(bin, vec![(1, WsOut::Binary(vec![1, 2, 3]))]);
        assert_eq!(
            app.ws_tick(1).unwrap(),
            vec![(1, WsOut::Text("tick".into()))]
        );
        let bye = app.ws_message(1, WsIn::Text("bye".into())).unwrap();
        assert_eq!(bye, vec![(1, WsOut::Close(4001, "bye".into()))]);
        assert_eq!(app.ws_count(), 1);
        app.ws_close(1).unwrap();
        assert_eq!(app.ws_count(), 0);
    });
}

#[test]
fn connections_of_a_worker_can_write_to_each_other() {
    const SRC: &str = r#"
let members: array<WebSocket> = []

fun handle(req: Request) -> Response
  Response.websocket(WebSocketHandler {
    on_open: fun(c: WebSocket) members.push(c) end,
    on_message: fun(c: WebSocket, m: WsMessage)
      for other in members do
        if other.id != c.id then
          other.send("{c.id}: {m.text ?? ""}")
        end
      end
    end,
  })
end
"#;
    with_app(SRC, |mut app| {
        app.handle(request("/")).unwrap();
        app.ws_accept(1, "a").unwrap();
        app.handle(request("/")).unwrap();
        app.ws_accept(2, "b").unwrap();
        let out = app.ws_message(1, WsIn::Text("hi".into())).unwrap();
        assert_eq!(out, vec![(2, WsOut::Text("a: hi".into()))]);
    });
}

#[test]
fn an_error_in_a_callback_is_reported_and_the_worker_goes_on() {
    const SRC: &str = r#"
fun handle(req: Request) -> Response
  Response.websocket(WebSocketHandler {
    on_message: fun(c: WebSocket, m: WsMessage)
      let xs: array<int> = []
      c.send("{xs[3]}")
    end,
  })
end
"#;
    with_app(SRC, |mut app| {
        app.handle(request("/")).unwrap();
        app.ws_accept(1, "a").unwrap();
        assert!(app.ws_message(1, WsIn::Text("x".into())).is_err());
        app.ws_close(1).unwrap();
        assert!(app.handle(request("/")).is_ok());
    });
}
