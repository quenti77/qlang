//! Distributed tracing (OpenTelemetry): `qlang serve --otlp-endpoint URL` makes a span of every request
//! (continuing the W3C `traceparent` the client sent), spans of the outgoing HTTP calls of the program
//! (with `traceparent` added to them) and of `Trace.span(name, f)`, and sends them to an OTLP/HTTP
//! collector in the background. The same ids are written on the log lines of the request.
//!
//! Without an endpoint nothing is exported, but a `traceparent` that arrives is still kept: the logs carry
//! its ids and the calls the program makes pass it on, so a service in the middle of a chain does not
//! break it.

use qlang_core::host::{HostError, HostValue, HttpRequest, HttpResponse};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const TRACE_EXTERNS: &[&str] = &[
    "trace_start",
    "trace_end",
    "trace_id",
    "trace_traceparent",
    "trace_attr",
];

/// Spans one request may hold (a loop calling `Trace.span` must not fill the memory).
const MAX_SPANS: usize = 1000;

struct Config {
    endpoint: String,
    service: String,
    flush: Duration,
}

static CONFIG: OnceLock<Option<Config>> = OnceLock::new();
static EXPORTER: OnceLock<Mutex<mpsc::Sender<Msg>>> = OnceLock::new();

enum Msg {
    Spans(Vec<Span>),
    Flush(mpsc::Sender<()>),
}

#[derive(Clone)]
struct Span {
    trace: [u8; 16],
    id: [u8; 8],
    parent: Option<[u8; 8]>,
    name: String,
    /// OTLP span kind: 1 internal, 2 server, 3 client.
    kind: u8,
    start_ns: u64,
    end_ns: u64,
    attrs: Vec<(String, String)>,
    error: bool,
}

struct Ctx {
    trace: [u8; 16],
    /// The `sampled` bit: 1 when the spans are exported.
    flags: u8,
    root: Span,
    /// Spans started by `Trace.start` and not ended, the innermost last.
    open: Vec<Span>,
    done: Vec<Span>,
}

thread_local! {
    static CURRENT: RefCell<Option<Ctx>> = const { RefCell::new(None) };
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
}

fn random_id<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    loop {
        for chunk in out.chunks_mut(8) {
            let bytes = crate::system_entropy().to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
        // an all-zero id is invalid
        if out.iter().any(|b| *b != 0) {
            return out;
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != N * 2 || !text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return None;
    }
    let mut out = [0u8; N];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
    }
    (out.iter().any(|b| *b != 0)).then_some(out)
}

/// `00-<32 hex>-<16 hex>-<2 hex>` of the W3C Trace Context.
fn parse_traceparent(value: &str) -> Option<([u8; 16], [u8; 8], u8)> {
    let mut parts = value.trim().split('-');
    let version = parts.next()?;
    let trace = unhex::<16>(parts.next()?)?;
    let parent = unhex::<8>(parts.next()?)?;
    let flags = u8::from_str_radix(parts.next()?, 16).ok()?;
    // version ff is invalid; a version 00 header has no more parts
    if version.len() != 2 || version == "ff" || (version == "00" && parts.next().is_some()) {
        return None;
    }
    Some((trace, parent, flags))
}

/// Turns tracing on: the collector's address (`--otlp-endpoint`, or `OTEL_EXPORTER_OTLP_ENDPOINT`) and
/// the name of this service (`--service-name`, `OTEL_SERVICE_NAME`, `qlang` by default).
pub fn init(endpoint: Option<String>, service: Option<String>) {
    let endpoint = endpoint
        .or_else(|| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok())
        .filter(|e| !e.trim().is_empty());
    let service = service
        .or_else(|| std::env::var("OTEL_SERVICE_NAME").ok())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "qlang".to_string());
    let flush_ms = std::env::var("QLANG_OTLP_FLUSH_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000u64);
    let config = endpoint.map(|endpoint| Config {
        endpoint: endpoint.trim_end_matches('/').to_string(),
        service,
        flush: Duration::from_millis(flush_ms.max(10)),
    });
    let _ = CONFIG.set(config);
    if let Some(Some(c)) = CONFIG.get() {
        let (send, recv) = mpsc::channel();
        if EXPORTER.set(Mutex::new(send)).is_ok() {
            let _ = std::thread::Builder::new()
                .name("trace-export".into())
                .spawn(move || export_loop(recv, c));
        }
    }
}

fn enabled() -> bool {
    matches!(CONFIG.get(), Some(Some(_)))
}

/// A request begins: its span is the root of the trace (or a child of the caller's span).
pub fn begin_request(headers: &[(String, String)], method: &str, url: &str) {
    let incoming = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("traceparent"))
        .and_then(|(_, v)| parse_traceparent(v));
    if incoming.is_none() && !enabled() {
        CURRENT.with(|c| *c.borrow_mut() = None);
        return;
    }
    let (trace, parent, flags) = match incoming {
        Some((t, p, f)) => (t, Some(p), f),
        None => (random_id::<16>(), None, 1),
    };
    let path = url.split(['?', '#']).next().unwrap_or("");
    let root = Span {
        trace,
        id: random_id::<8>(),
        parent,
        name: format!("{method} {path}"),
        kind: 2,
        start_ns: now_ns(),
        end_ns: 0,
        attrs: vec![
            ("http.request.method".into(), method.into()),
            ("url.path".into(), path.into()),
        ],
        error: false,
    };
    CURRENT.with(|c| {
        *c.borrow_mut() = Some(Ctx {
            trace,
            flags,
            root,
            open: Vec::new(),
            done: Vec::new(),
        })
    });
}

/// The request is answered: its spans leave for the collector.
pub fn end_request(status: u16) {
    let Some(mut ctx) = CURRENT.with(|c| c.borrow_mut().take()) else {
        return;
    };
    let end = now_ns();
    ctx.root.end_ns = end;
    ctx.root
        .attrs
        .push(("http.response.status_code".into(), status.to_string()));
    ctx.root.error = status >= 500;
    // spans the program left open end with the request
    while let Some(mut s) = ctx.open.pop() {
        s.end_ns = end;
        ctx.done.push(s);
    }
    ctx.done.push(ctx.root);
    if ctx.flags & 1 == 1
        && let Some(tx) = EXPORTER.get()
        && let Ok(tx) = tx.lock()
    {
        let _ = tx.send(Msg::Spans(ctx.done));
    }
}

/// The ids to write on a log line: the trace and the span being run.
pub fn current_ids() -> Option<(String, String)> {
    CURRENT.with(|c| {
        c.borrow().as_ref().map(|ctx| {
            let span = ctx.open.last().unwrap_or(&ctx.root);
            (hex(&ctx.trace), hex(&span.id))
        })
    })
}

fn traceparent_now() -> Option<String> {
    CURRENT.with(|c| {
        c.borrow().as_ref().map(|ctx| {
            let span = ctx.open.last().unwrap_or(&ctx.root);
            format!("00-{}-{}-{:02x}", hex(&ctx.trace), hex(&span.id), ctx.flags)
        })
    })
}

/// An outgoing HTTP call: the header is added (unless the program set its own), and the span of the call is
/// returned to be closed with [`finish_client`].
pub fn client_call(req: &mut HttpRequest) -> Option<ClientSpan> {
    let (trace, parent, flags) = CURRENT.with(|c| {
        c.borrow().as_ref().map(|ctx| {
            let span = ctx.open.last().unwrap_or(&ctx.root);
            (ctx.trace, span.id, ctx.flags)
        })
    })?;
    let id = random_id::<8>();
    if !req
        .headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("traceparent"))
    {
        req.headers.push((
            "traceparent".into(),
            format!("00-{}-{}-{:02x}", hex(&trace), hex(&id), flags),
        ));
    }
    // no query and no credentials in the span: only where the call went
    let url = req.url.split(['?', '#']).next().unwrap_or("");
    let url = match url.split_once("://") {
        Some((scheme, rest)) => match rest.split_once('@') {
            Some((_, after))
                if !rest.split('/').next().unwrap_or("").is_empty()
                    && rest.find('@') < rest.find('/') =>
            {
                format!("{scheme}://{after}")
            }
            _ => url.to_string(),
        },
        None => url.to_string(),
    };
    Some(ClientSpan {
        span: Span {
            trace,
            id,
            parent: Some(parent),
            name: format!("HTTP {}", req.method),
            kind: 3,
            start_ns: now_ns(),
            end_ns: 0,
            attrs: vec![
                ("http.request.method".into(), req.method.clone()),
                ("url.full".into(), url),
            ],
            error: false,
        },
    })
}

pub struct ClientSpan {
    span: Span,
}

pub fn finish_client(call: Option<ClientSpan>, result: &Result<HttpResponse, HostError>) {
    let Some(ClientSpan { mut span }) = call else {
        return;
    };
    span.end_ns = now_ns();
    match result {
        Ok(r) => {
            span.attrs
                .push(("http.response.status_code".into(), r.status.to_string()));
            span.error = r.status >= 500;
        }
        Err(e) => {
            span.attrs.push(("error.type".into(), format!("{e:?}")));
            span.error = true;
        }
    }
    CURRENT.with(|c| {
        if let Some(ctx) = c.borrow_mut().as_mut()
            && ctx.done.len() < MAX_SPANS
        {
            ctx.done.push(span);
        }
    });
}

fn fail(message: impl Into<String>) -> HostError {
    HostError::Other(message.into())
}

/// The externs behind `Trace`.
pub fn call(name: &str, args: &[HostValue]) -> Result<HostValue, HostError> {
    CURRENT.with(|c| {
        let mut guard = c.borrow_mut();
        match (name, args) {
            ("trace_id", []) => Ok(HostValue::Str(
                guard
                    .as_ref()
                    .map(|ctx| hex(&ctx.trace))
                    .unwrap_or_default(),
            )),
            ("trace_traceparent", []) => {
                drop(guard);
                Ok(HostValue::Str(traceparent_now().unwrap_or_default()))
            }
            ("trace_start", [HostValue::Str(span_name)]) => {
                // outside a traced request this does nothing: 0 is "no span"
                let Some(ctx) = guard.as_mut() else {
                    return Ok(HostValue::Int(0));
                };
                if ctx.open.len() + ctx.done.len() >= MAX_SPANS {
                    return Err(fail(format!("at most {MAX_SPANS} spans in one request")));
                }
                let parent = ctx.open.last().unwrap_or(&ctx.root).id;
                ctx.open.push(Span {
                    trace: ctx.trace,
                    id: random_id::<8>(),
                    parent: Some(parent),
                    name: span_name.chars().take(200).collect(),
                    kind: 1,
                    start_ns: now_ns(),
                    end_ns: 0,
                    attrs: Vec::new(),
                    error: false,
                });
                Ok(HostValue::Int(ctx.open.len() as i64))
            }
            ("trace_end", [HostValue::Int(handle), HostValue::Bool(failed)]) => {
                let Some(ctx) = guard.as_mut() else {
                    return Ok(HostValue::Null);
                };
                let depth = *handle as usize;
                // ends the span and the ones left open inside it
                while ctx.open.len() >= depth && depth > 0 {
                    let mut s = ctx.open.pop().expect("an open span");
                    s.end_ns = now_ns();
                    s.error = *failed && ctx.open.len() + 1 == depth;
                    ctx.done.push(s);
                }
                Ok(HostValue::Null)
            }
            ("trace_attr", [HostValue::Str(key), HostValue::Str(value)]) => {
                if let Some(ctx) = guard.as_mut() {
                    let span = ctx.open.last_mut().unwrap_or(&mut ctx.root);
                    if span.attrs.len() < 64 {
                        span.attrs.push((
                            key.chars().take(100).collect(),
                            value.chars().take(1000).collect(),
                        ));
                    }
                }
                Ok(HostValue::Null)
            }
            _ => Err(HostError::Unsupported("extern")),
        }
    })
}

fn span_json(s: &Span) -> Value {
    let mut attrs: Vec<Value> = s
        .attrs
        .iter()
        .map(|(k, v)| json!({"key": k, "value": {"stringValue": v}}))
        .collect();
    // the status code is a number for the collectors that index it
    for a in attrs.iter_mut() {
        if a["key"] == "http.response.status_code"
            && let Some(n) = a["value"]["stringValue"]
                .as_str()
                .and_then(|t| t.parse::<i64>().ok())
        {
            a["value"] = json!({"intValue": n.to_string()});
        }
    }
    let mut v = json!({
        "traceId": hex(&s.trace),
        "spanId": hex(&s.id),
        "name": s.name,
        "kind": s.kind,
        "startTimeUnixNano": s.start_ns.to_string(),
        "endTimeUnixNano": s.end_ns.max(s.start_ns).to_string(),
        "attributes": attrs,
        "status": if s.error { json!({"code": 2}) } else { json!({}) },
    });
    if let Some(p) = s.parent {
        v["parentSpanId"] = json!(hex(&p));
    }
    v
}

fn payload(config: &Config, spans: &[Span]) -> Value {
    json!({"resourceSpans": [{
        "resource": {"attributes": [
            {"key": "service.name", "value": {"stringValue": config.service}},
            {"key": "telemetry.sdk.name", "value": {"stringValue": "qlang"}},
            {"key": "telemetry.sdk.version", "value": {"stringValue": env!("CARGO_PKG_VERSION")}},
        ]},
        "scopeSpans": [{
            "scope": {"name": "qlang", "version": env!("CARGO_PKG_VERSION")},
            "spans": spans.iter().map(span_json).collect::<Vec<_>>(),
        }],
    }]})
}

fn export_loop(recv: mpsc::Receiver<Msg>, config: &'static Config) {
    let mut batch: Vec<Span> = Vec::new();
    let mut warned = false;
    let send = |batch: &mut Vec<Span>, warned: &mut bool| {
        if batch.is_empty() {
            return;
        }
        let body = payload(config, batch).to_string();
        batch.clear();
        let url = if config.endpoint.ends_with("/v1/traces") {
            config.endpoint.clone()
        } else {
            format!("{}/v1/traces", config.endpoint)
        };
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(5)))
            .http_status_as_error(false)
            .build()
            .into();
        let result = agent
            .post(&url)
            .header("Content-Type", "application/json")
            .send(body.as_bytes());
        let failed = match &result {
            Ok(r) => !r.status().is_success(),
            Err(_) => true,
        };
        // a collector that is down must not flood the log, nor slow the requests
        if failed && !*warned {
            *warned = true;
            eprintln!("traces: the collector at {url} did not accept them (this is said once)");
        } else if !failed {
            *warned = false;
        }
    };
    loop {
        match recv.recv_timeout(config.flush) {
            Ok(Msg::Spans(mut spans)) => {
                batch.append(&mut spans);
                if batch.len() >= 256 {
                    send(&mut batch, &mut warned);
                }
            }
            Ok(Msg::Flush(ack)) => {
                send(&mut batch, &mut warned);
                let _ = ack.send(());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => send(&mut batch, &mut warned),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                send(&mut batch, &mut warned);
                return;
            }
        }
    }
}

/// Sends what is waiting (the server is stopping).
pub fn flush() {
    if let Some(tx) = EXPORTER.get() {
        let (ack, wait) = mpsc::channel();
        if let Ok(tx) = tx.lock()
            && tx.send(Msg::Flush(ack)).is_ok()
        {
            drop(tx);
            let _ = wait.recv_timeout(Duration::from_secs(6));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traceparent_is_read_strictly() {
        let ok =
            parse_traceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01").unwrap();
        assert_eq!(hex(&ok.0), "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(hex(&ok.1), "00f067aa0ba902b7");
        assert_eq!(ok.2, 1);
        for bad in [
            "",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7",
            "00-00000000000000000000000000000000-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01",
            "ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-extra",
        ] {
            assert!(parse_traceparent(bad).is_none(), "{bad}");
        }
        // a later version may have more parts
        assert!(
            parse_traceparent("01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-more")
                .is_some()
        );
    }
}
