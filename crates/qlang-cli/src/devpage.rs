//! The page `qlang serve --dev` shows when the program fails: what went wrong, where, the code
//! around it, the calls that led there, the request and the SQL it ran. As HTML for a browser,
//! as JSON when the client asks for it. Never produced without `--dev`: it shows the source.

use crate::db::{LoggedQuery, repeated};
use qlang_core::diag::Diagnostic;
use qlang_core::span::SourceMap;

/// One step of the stack: what was being called, and where.
pub struct Frame {
    pub message: String,
    pub file: String,
    pub line: u32,
    pub col: u32,
}

/// The request that failed, as the page shows it.
pub struct RequestInfo {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
}

/// A failure of the program while it answered a request.
pub struct Failure {
    /// The diagnostic as the terminal shows it (what goes to the log).
    pub text: String,
    pub code: String,
    pub message: String,
    pub file: String,
    pub line: u32,
    pub col: u32,
    /// The lines around the failure, with their numbers.
    pub snippet: Vec<(u32, String)>,
    /// The stack, the innermost call first.
    pub frames: Vec<Frame>,
    pub request: Option<RequestInfo>,
    pub queries: Vec<LoggedQuery>,
}

/// How many lines of code are shown on each side of the failing one.
const CONTEXT: u32 = 5;

/// A header whose value is not shown: it holds a credential.
fn secret_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "cookie" | "authorization" | "proxy-authorization" | "x-csrf-token" | "x-api-key"
    )
}

impl Failure {
    pub fn new(d: &Diagnostic, map: &SourceMap) -> Failure {
        let loc = map.locate(d.span);
        let mut snippet = Vec::new();
        let from = loc.line.saturating_sub(CONTEXT).max(1);
        for n in from..=loc.line + CONTEXT {
            if let Some(text) = map.line_text(d.span.file, n) {
                snippet.push((n, text.to_string()));
            }
        }
        let frames = d
            .notes
            .iter()
            .map(|n| {
                let at = n.span.map(|s| map.locate(s));
                Frame {
                    message: n.message.clone(),
                    file: at.as_ref().map(|l| l.file.clone()).unwrap_or_default(),
                    line: at.as_ref().map_or(0, |l| l.line),
                    col: at.as_ref().map_or(0, |l| l.col),
                }
            })
            .collect();
        Failure {
            text: d.render(map),
            code: d.code.clone(),
            message: d.message.clone(),
            file: loc.file,
            line: loc.line,
            col: loc.col,
            snippet,
            frames,
            request: None,
            queries: Vec::new(),
        }
    }

    /// The `context` of the log line of this failure (`--log-format json`): the error with its
    /// stack, so it can be searched and grouped, without the source code.
    pub fn log_context(&self) -> String {
        serde_json::json!({
            "code": self.code,
            "location": format!("{}:{}:{}", self.file, self.line, self.col),
            "stack": self.frames.iter().map(|f| {
                if f.file.is_empty() { f.message.clone() }
                else { format!("{} ({}:{}:{})", f.message, f.file, f.line, f.col) }
            }).collect::<Vec<_>>(),
            "method": self.request.as_ref().map(|r| r.method.clone()),
            "url": self.request.as_ref().map(|r| r.url.clone()),
        })
        .to_string()
    }

    /// The same failure for a client that asked for JSON.
    pub fn json(&self) -> String {
        let queries: Vec<_> = self
            .queries
            .iter()
            .map(|q| serde_json::json!({"sql": q.sql, "ms": (q.ms * 100.0).round() / 100.0}))
            .collect();
        serde_json::json!({
            "error": {
                "code": self.code,
                "message": self.message,
                "file": self.file,
                "line": self.line,
                "column": self.col,
                "snippet": self.snippet.iter()
                    .map(|(n, t)| serde_json::json!({"line": n, "text": t}))
                    .collect::<Vec<_>>(),
                "stack": self.frames.iter().map(|f| serde_json::json!({
                    "message": f.message, "file": f.file, "line": f.line, "column": f.col,
                })).collect::<Vec<_>>(),
            },
            "request": self.request.as_ref().map(|r| serde_json::json!({
                "method": r.method,
                "url": r.url,
                "headers": r.headers.iter().map(|(k, v)| {
                    let shown = if secret_header(k) { "(hidden)".to_string() } else { v.clone() };
                    (k.clone(), serde_json::Value::String(shown))
                }).collect::<serde_json::Map<String, serde_json::Value>>(),
            })),
            "queries": queries,
        })
        .to_string()
    }

    /// The page for a browser.
    pub fn html(&self) -> String {
        let mut out = String::new();
        out.push_str(PAGE_HEAD);
        out.push_str(&format!(
            "<h1><span class=\"code\">{}</span> {}</h1>\n<p class=\"where\">{}:{}:{}</p>\n",
            esc(&self.code),
            esc(&self.message),
            esc(&self.file),
            self.line,
            self.col
        ));
        out.push_str("<h2>Code</h2>\n<pre class=\"code-block\">");
        for (n, text) in &self.snippet {
            let class = if *n == self.line { "line hit" } else { "line" };
            out.push_str(&format!(
                "<span class=\"{class}\"><span class=\"n\">{n}</span>{}</span>\n",
                esc(text)
            ));
        }
        out.push_str("</pre>\n");
        if !self.frames.is_empty() {
            out.push_str("<h2>Stack</h2>\n<ol class=\"stack\">\n");
            for f in &self.frames {
                let at = if f.file.is_empty() {
                    String::new()
                } else {
                    format!(
                        " <span class=\"at\">{}:{}:{}</span>",
                        esc(&f.file),
                        f.line,
                        f.col
                    )
                };
                out.push_str(&format!("<li>{}{at}</li>\n", esc(&f.message)));
            }
            out.push_str("</ol>\n");
        }
        if let Some(r) = &self.request {
            out.push_str(&format!(
                "<h2>Request</h2>\n<p><b>{}</b> {}</p>\n<table>\n",
                esc(&r.method),
                esc(&r.url)
            ));
            for (k, v) in &r.headers {
                let shown = if secret_header(k) { "(hidden)" } else { v };
                out.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td></tr>\n",
                    esc(k),
                    esc(shown)
                ));
            }
            out.push_str("</table>\n");
        }
        if !self.queries.is_empty() {
            let total: f64 = self.queries.iter().map(|q| q.ms).sum();
            out.push_str(&format!(
                "<h2>SQL <small>{} statement(s), {total:.1} ms</small></h2>\n",
                self.queries.len()
            ));
            for (sql, n, ms) in repeated(&self.queries) {
                out.push_str(&format!(
                    "<p class=\"warn\">N+1? the same statement ran {n} times ({ms:.1} ms): <code>{}</code></p>\n",
                    esc(&sql)
                ));
            }
            out.push_str("<table>\n");
            for q in &self.queries {
                out.push_str(&format!(
                    "<tr><td class=\"ms\">{:.2} ms</td><td><code>{}</code></td></tr>\n",
                    q.ms,
                    esc(&q.sql)
                ));
            }
            out.push_str("</table>\n");
        }
        out.push_str("<p class=\"foot\">Shown because the server runs with <code>--dev</code>: never use it in production.</p>\n</body></html>\n");
        out
    }
}

/// A line for the terminal when a request ran the same statement many times.
pub fn n_plus_one_warning(method: &str, url: &str, queries: &[LoggedQuery]) -> Option<String> {
    let again = repeated(queries);
    let (sql, n, ms) = again.first()?;
    Some(format!(
        "N+1? {method} {url} ran the same statement {n} times ({ms:.1} ms): {sql}\n"
    ))
}

fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

const PAGE_HEAD: &str = "<!doctype html>
<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
<title>qlang error</title>
<style>
:root { --bg: #fff; --fg: #1d2330; --muted: #5f6b7a; --line: #dfe3ea; --hit: #fde8e8; --code: #c0302b; --warn: #8a5a00; --pre: #f6f7f9; }
@media (prefers-color-scheme: dark) { :root { --bg: #14171d; --fg: #e6e9ef; --muted: #98a2b3; --line: #2a303b; --hit: #4a1f22; --code: #ff7a73; --warn: #f0b429; --pre: #1b2028; } }
body { margin: 0; padding: 24px 16px 48px; background: var(--bg); color: var(--fg); font: 15px/1.5 system-ui, sans-serif; }
body > * { max-width: 960px; margin-left: auto; margin-right: auto; }
h1 { font-size: 1.4rem; margin: 0 auto .2rem; } h2 { font-size: 1.05rem; margin-top: 2rem; } h2 small { color: var(--muted); font-weight: 400; }
.code { color: var(--code); font-family: ui-monospace, monospace; }
.where, .at, .foot { color: var(--muted); }
.where { margin-top: 0; font-family: ui-monospace, monospace; }
pre, code, td { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 13px; }
.code-block { background: var(--pre); border: 1px solid var(--line); border-radius: 6px; padding: 8px 0; overflow-x: auto; }
.line { display: block; padding: 0 12px; white-space: pre; } .line.hit { background: var(--hit); }
.n { display: inline-block; width: 3em; color: var(--muted); user-select: none; }
.stack li { margin: .25rem 0; }
table { border-collapse: collapse; width: 100%; } td { border-top: 1px solid var(--line); padding: 4px 8px; vertical-align: top; word-break: break-word; }
.ms { white-space: nowrap; color: var(--muted); width: 1%; }
.warn { color: var(--warn); }
</style></head><body>
";

#[cfg(test)]
mod tests {
    use super::*;
    use qlang_core::span::Span;

    fn sample() -> Failure {
        let mut map = SourceMap::default();
        let file = map.add("main.q", "line one\nlet x = <b>\nline three\n");
        let d = Diagnostic::error("R050", "panic: <boom> & \"quotes\"", Span::new(file, 9, 18))
            .with_note("called from `handle`", Some(Span::new(file, 0, 4)));
        let mut f = Failure::new(&d, &map);
        f.request = Some(RequestInfo {
            method: "GET".into(),
            url: "/x?a=<1>".into(),
            headers: vec![
                ("host".into(), "localhost".into()),
                ("cookie".into(), "qs=secret".into()),
                ("authorization".into(), "Bearer t0ken".into()),
            ],
        });
        f.queries = vec![
            LoggedQuery {
                sql: "SELECT 1".into(),
                ms: 1.5,
            },
            LoggedQuery {
                sql: "SELECT * FROM t WHERE id = ?".into(),
                ms: 0.5,
            },
            LoggedQuery {
                sql: "SELECT * FROM t WHERE id = ?".into(),
                ms: 0.5,
            },
            LoggedQuery {
                sql: "SELECT * FROM t WHERE id = ?".into(),
                ms: 0.5,
            },
        ];
        f
    }

    #[test]
    fn the_page_escapes_what_comes_from_the_program_and_the_request() {
        let html = sample().html();
        assert!(
            html.contains("panic: &lt;boom&gt; &amp; &quot;quotes&quot;"),
            "{html}"
        );
        assert!(html.contains("/x?a=&lt;1&gt;"), "{html}");
        assert!(html.contains("let x = &lt;b&gt;"), "{html}");
        assert!(!html.contains("<boom>"), "{html}");
        assert!(html.contains("class=\"line hit\""), "{html}");
        assert!(html.contains("N+1?"), "{html}");
    }

    #[test]
    fn credentials_are_not_shown() {
        let f = sample();
        for text in [f.html(), f.json()] {
            assert!(
                !text.contains("qs=secret") && !text.contains("t0ken"),
                "{text}"
            );
            assert!(text.contains("(hidden)"), "{text}");
        }
    }

    #[test]
    fn the_json_has_the_error_the_stack_the_request_and_the_queries() {
        let v: serde_json::Value = serde_json::from_str(&sample().json()).unwrap();
        assert_eq!(v["error"]["code"], "R050");
        assert_eq!(v["error"]["line"], 2);
        assert_eq!(v["error"]["stack"][0]["message"], "called from `handle`");
        assert_eq!(v["request"]["method"], "GET");
        assert_eq!(v["queries"].as_array().unwrap().len(), 4);
        assert!(v["error"]["snippet"].as_array().unwrap().len() >= 3);
    }

    #[test]
    fn the_log_line_is_one_object_with_the_stack() {
        let v: serde_json::Value = serde_json::from_str(&sample().log_context()).unwrap();
        assert_eq!(v["code"], "R050");
        assert_eq!(v["location"], "main.q:2:1");
        assert!(
            v["stack"][0]
                .as_str()
                .unwrap()
                .starts_with("called from `handle` (main.q:1:1)")
        );
        assert_eq!(v["url"], "/x?a=<1>");
    }

    #[test]
    fn a_repeated_statement_is_reported_for_the_terminal() {
        let f = sample();
        let line = n_plus_one_warning("GET", "/posts", &f.queries).unwrap();
        assert!(
            line.contains("3 times") && line.contains("SELECT * FROM t WHERE id = ?"),
            "{line}"
        );
        assert!(n_plus_one_warning("GET", "/", &f.queries[..2]).is_none());
    }
}
