//! WebSockets: the program hands a `WebSocketHandler` to `Response.websocket`; the server owns the
//! network and calls `App::ws_*` for each event. A worker holds many connections and runs one
//! callback at a time, each with a fresh step budget.

use super::*;

/// One open connection: the handler the program gave and the `WebSocket` its callbacks receive.
#[derive(Clone)]
pub(super) struct WsSession {
    handler: Value,
    conn: Value,
}

/// The outputs of the callbacks: for each connection (its number), what to send.
pub type WsOutputs = Vec<(u64, WsOut)>;

impl Interp<'_> {
    pub(super) fn ws_new(&self, handler: Value, request: Value) -> WsSession {
        let conn = self.new_struct(
            self.prelude_struct("WebSocket"),
            &[
                ("id", Value::str("")),
                ("request", request),
                ("data", Value::Map(Rc::new(RefCell::new(MapObj::default())))),
                ("outbox", Value::array(Vec::new())),
                ("open", Value::Bool(true)),
            ],
        );
        WsSession { handler, conn }
    }

    pub(super) fn ws_handler_tick(&self, handler: &Value) -> u64 {
        match handler {
            Value::Struct(o) => match o.fields.borrow().get("tick_ms") {
                Some(Value::Int(n)) if *n > 0 => (*n as u64).max(10),
                _ => 0,
            },
            _ => 0,
        }
    }

    fn handler_field(session: &WsSession, name: &str) -> Option<Value> {
        let Value::Struct(o) = &session.handler else {
            return None;
        };
        match o.fields.borrow().get(name) {
            Some(v @ Value::Func(_)) => Some(v.clone()),
            _ => None,
        }
    }

    /// Takes what every connection queued to send (a callback may write to another one).
    fn ws_drain(&self) -> WsOutputs {
        let mut out = WsOutputs::new();
        for (n, s) in &self.ws_sessions {
            let Value::Struct(o) = &s.conn else { continue };
            let frames: Vec<Value> = match o.fields.borrow().get("outbox") {
                Some(Value::Array(a)) => std::mem::take(&mut *a.borrow_mut()),
                _ => Vec::new(),
            };
            for f in frames {
                let Value::Struct(fo) = &f else { continue };
                let fields = fo.fields.borrow();
                let text = match fields.get("text") {
                    Some(Value::Str(t)) => t.to_string(),
                    _ => String::new(),
                };
                let kind = match fields.get("kind") {
                    Some(Value::Str(k)) => k.to_string(),
                    _ => continue,
                };
                match kind.as_str() {
                    "text" => out.push((*n, WsOut::Text(text))),
                    "binary" => {
                        let data = match fields.get("data") {
                            Some(Value::Bytes(b)) => b.to_vec(),
                            _ => Vec::new(),
                        };
                        out.push((*n, WsOut::Binary(data)));
                    }
                    "close" => {
                        let code = match fields.get("code") {
                            Some(Value::Int(c)) if (1000..=4999).contains(c) => *c as u16,
                            _ => 1000,
                        };
                        out.push((*n, WsOut::Close(code, text)));
                    }
                    _ => {}
                }
            }
        }
        out
    }

    fn ws_reset_budget(&mut self) {
        self.steps = 0;
        self.depth = 0;
        self.out_bytes = 0;
        self.frames.clear();
        self.fmt_depth = 0;
        self.fmt_nodes = 0;
    }
}

impl App<'_> {
    fn ws_find(&self, n: u64) -> Option<WsSession> {
        self.it
            .ws_sessions
            .iter()
            .find(|(k, _)| *k == n)
            .map(|(_, s)| s.clone())
    }

    /// Runs `callback` of connection `n` (if the handler has it) with `extra` after the
    /// connection, and gives back everything the connections queued to send.
    fn ws_call(
        &mut self,
        n: u64,
        callback: &str,
        extra: Option<Value>,
    ) -> Result<WsOutputs, Box<Diagnostic>> {
        let Some(session) = self.ws_find(n) else {
            return Ok(Vec::new());
        };
        let Some(f) = Interp::handler_field(&session, callback) else {
            return Ok(self.it.ws_drain());
        };
        let it = &mut self.it;
        it.ws_reset_budget();
        let prog = it.prog;
        let mut args = vec![session.conn.clone()];
        args.extend(extra);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            it.call_value(&f, args, Default::default())
        }));
        match outcome {
            Ok(Ok(_)) | Ok(Err(Ctrl::Return(_))) => Ok(self.it.ws_drain()),
            Ok(Err(Ctrl::Error(mut d))) => {
                d.resolve(&prog.sources);
                Err(d)
            }
            Ok(Err(_)) => Ok(self.it.ws_drain()),
            Err(payload) => Err(Box::new(crate::check::internal_error(&payload))),
        }
    }

    /// The server accepted the WebSocket the last `handle` answered with: it is connection `n`,
    /// with this unique `id`. Runs `on_open`.
    pub fn ws_accept(&mut self, n: u64, id: &str) -> Result<WsOutputs, Box<Diagnostic>> {
        let Some(session) = self.it.pending_ws.take() else {
            return Ok(Vec::new());
        };
        if let Value::Struct(o) = &session.conn {
            o.fields.borrow_mut().insert("id", Value::str(id));
        }
        self.it.ws_sessions.push((n, session));
        self.ws_call(n, "on_open", None)
    }

    /// A message arrived on connection `n`.
    pub fn ws_message(&mut self, n: u64, message: WsIn) -> Result<WsOutputs, Box<Diagnostic>> {
        let (text, data, binary) = match message {
            WsIn::Text(t) => (Value::str(&t), Value::None, false),
            WsIn::Binary(b) => (Value::None, Value::Bytes(Rc::from(b)), true),
        };
        let value = self.it.new_struct(
            self.it.prelude_struct("WsMessage"),
            &[
                ("text", text),
                ("data", data),
                ("binary", Value::Bool(binary)),
            ],
        );
        self.ws_call(n, "on_message", Some(value))
    }

    /// The tick of connection `n` came (see `WebSocketHandler.tick_ms`).
    pub fn ws_tick(&mut self, n: u64) -> Result<WsOutputs, Box<Diagnostic>> {
        self.ws_call(n, "on_tick", None)
    }

    /// Connection `n` ended: runs `on_close` and forgets it.
    pub fn ws_close(&mut self, n: u64) -> Result<WsOutputs, Box<Diagnostic>> {
        if let Some(session) = self.ws_find(n)
            && let Value::Struct(o) = &session.conn
        {
            o.fields.borrow_mut().insert("open", Value::Bool(false));
        }
        let out = self.ws_call(n, "on_close", None);
        self.it.ws_sessions.retain(|(k, _)| *k != n);
        out
    }

    /// The server could not give the connection to the program (the client did not ask for a
    /// WebSocket): forget the one `handle` just answered with.
    pub fn ws_reject(&mut self) {
        self.it.pending_ws = None;
    }

    /// How many connections are open in this worker.
    pub fn ws_count(&self) -> usize {
        self.it.ws_sessions.len()
    }
}
