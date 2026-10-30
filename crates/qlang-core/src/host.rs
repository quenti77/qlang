//! The boundary between the language and the outside world.
//!
//! The core never prints, reads or touches the disk: everything goes through
//! a [`Host`] supplied by the caller (terminal, web server, tests...).

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostError {
    /// This environment does not offer the requested capability.
    Unsupported(&'static str),
    Other(String),
}

/// What an environment can do. Checked at compile time so that, for example,
/// a call to `read()` is reported before running when no input is available.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Capabilities {
    /// `read()` is available.
    pub read: bool,
    /// `time.now()` is available (the host has a clock).
    pub time: bool,
    /// `random` and the generators of `uuid` and `ulid` are available.
    pub random: bool,
    /// `http.send(...)` is available (the host can reach the network).
    pub http: bool,
    /// `fs.read(...)`/`fs.write(...)` are available (the host can touch the disk).
    pub fs: bool,
    /// `env.get(...)`/`env.args()` are available (the host has a process environment).
    pub env: bool,
}

/// A column value read from, or bound to, a SQL statement. Null is explicit: a database
/// column has no equivalent of qlang's `none` by default, so both sides must say so.
#[derive(Clone, Debug)]
pub enum SqlValue {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
    Bool(bool),
}

/// One row of a `Db.query` result: `(column name, value)`, in column order.
#[derive(Clone, Debug)]
pub struct SqlRow {
    pub columns: Vec<(String, SqlValue)>,
}

/// The database side of a host. `Db` is written in qlang on top of four `extern fun`
/// (`sql_connect`, `sql_query`, `sql_execute`, `sql_close`, see [`SQL_EXTERNS`]); a host
/// that has a database implements this trait and forwards those names to [`sql_call`]
/// from [`Host::call`], so it does not have to convert the values itself.
pub trait SqlBackend {
    /// Open a connection (`sqlite://path`, `postgres://...`...), returning a handle for
    /// the other methods. What the handle means is entirely up to the host.
    fn sql_connect(&mut self, url: &str) -> Result<u64, HostError>;

    /// Run a statement that returns rows (`SELECT`), with positional `?` parameters.
    fn sql_query(
        &mut self,
        conn: u64,
        sql: &str,
        params: Vec<SqlValue>,
    ) -> Result<Vec<SqlRow>, HostError>;

    /// Run a statement that does not return rows (`INSERT`/`UPDATE`/`DELETE`/DDL),
    /// returning the number of rows affected.
    fn sql_execute(
        &mut self,
        conn: u64,
        sql: &str,
        params: Vec<SqlValue>,
    ) -> Result<u64, HostError>;

    /// Close a connection opened by `sql_connect`.
    fn sql_close(&mut self, conn: u64) -> Result<(), HostError>;
}

/// The `extern fun` of the prelude that the methods of [`Host`] above implement: `fs.read`
/// and `fs.write` (`fs_read`, `fs_write`), `env.get` and `env.args` (`env_get`, `env_args`)
/// and `http.send` (`http_send`). Whether a host has them follows its [`Capabilities`].
pub fn std_has(caps: Capabilities, name: &str) -> bool {
    match name {
        "fs_read" | "fs_write" | "fs_read_bytes" | "fs_write_bytes" | "fs_create_dir"
        | "fs_exists" | "fs_list" | "fs_is_dir" | "fs_open_lines" | "fs_read_line"
        | "fs_close_lines" => caps.fs,
        "env_get" | "env_args" => caps.env,
        "secure_random_bytes" => caps.random,
        // pure functions, and the log: every host has them
        "constant_time_eq" | "log_write" => true,
        "http_send" | "http_send_all" => caps.http,
        _ => false,
    }
}

/// Runs one of the names of [`std_has`] on `host`; `None` if `name` is not one of them.
pub fn std_call<H: Host + ?Sized>(
    host: &mut H,
    name: &str,
    args: Vec<HostValue>,
) -> Option<Result<HostValue, HostError>> {
    let wrong =
        |name: &str| HostError::Other(format!("`{name}` was called with the wrong arguments"));
    Some(match (name, args.as_slice()) {
        ("fs_read", [HostValue::Str(path)]) => host.read_file(path).map(HostValue::Str),
        ("fs_write", [HostValue::Str(path), HostValue::Str(content)]) => host
            .write_file(path, content)
            .map(|()| HostValue::Bool(true)),
        ("fs_read_bytes", [HostValue::Str(path)]) => {
            host.read_file_bytes(path).map(HostValue::Bytes)
        }
        ("fs_write_bytes", [HostValue::Str(path), HostValue::Bytes(content)]) => host
            .write_file_bytes(path, content)
            .map(|()| HostValue::Bool(true)),
        ("fs_create_dir", [HostValue::Str(path)]) => {
            host.create_dir(path).map(|()| HostValue::Bool(true))
        }
        ("fs_exists", [HostValue::Str(path)]) => host.path_exists(path).map(HostValue::Bool),
        ("fs_is_dir", [HostValue::Str(path)]) => host.is_dir(path).map(HostValue::Bool),
        ("fs_list", [HostValue::Str(path)]) => host
            .list_dir(path)
            .map(|names| HostValue::Array(names.into_iter().map(HostValue::Str).collect())),
        ("fs_open_lines", [HostValue::Str(path)]) => {
            host.open_lines(path).map(|id| HostValue::Int(id as i64))
        }
        ("fs_read_line", [HostValue::Int(id)]) => host
            .read_file_line(*id as u64)
            .map(|line| line.map_or(HostValue::Null, HostValue::Str)),
        ("fs_close_lines", [HostValue::Int(id)]) => {
            host.close_lines(*id as u64).map(|()| HostValue::Bool(true))
        }
        ("secure_random_bytes", [HostValue::Int(n)]) => {
            if !(0..=1_048_576).contains(n) {
                return Some(Err(HostError::Other(
                    "secure random bytes: from 0 to 1048576".to_string(),
                )));
            }
            host.secure_random(*n as usize).map(HostValue::Bytes)
        }
        (
            "log_write",
            [
                HostValue::Str(level),
                HostValue::Str(message),
                HostValue::Map(fields),
            ],
        ) => {
            let fields: Vec<(String, String)> = fields
                .iter()
                .filter_map(|(k, v)| match (k, v) {
                    (HostValue::Str(k), HostValue::Str(v)) => Some((k.clone(), v.clone())),
                    _ => None,
                })
                .collect();
            host.log(level, message, &fields);
            Ok(HostValue::Bool(true))
        }
        ("constant_time_eq", [HostValue::Bytes(a), HostValue::Bytes(b)]) => {
            // every byte is looked at, whatever the first difference
            let mut diff = a.len() ^ b.len();
            for i in 0..a.len().max(b.len()) {
                diff |=
                    usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
            }
            Ok(HostValue::Bool(diff == 0))
        }
        ("env_get", [HostValue::Str(name)]) => host
            .env_var(name)
            .map(|v| v.map_or(HostValue::Null, HostValue::Str)),
        ("env_args", []) => host
            .args()
            .map(|xs| HostValue::Array(xs.into_iter().map(HostValue::Str).collect())),
        ("http_send", [HostValue::Map(fields)]) => {
            let request = request_of(fields);
            host.http_send(request).map(|r| response_value(Ok(r)))
        }
        ("http_send_all", [HostValue::Array(items)]) => {
            let mut requests = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    HostValue::Map(fields) => requests.push(request_of(fields)),
                    _ => return Some(Err(wrong(name))),
                }
            }
            Ok(HostValue::Array(
                host.http_send_all(requests)
                    .into_iter()
                    .map(response_value)
                    .collect(),
            ))
        }
        (
            "fs_read"
            | "fs_write"
            | "fs_read_bytes"
            | "fs_write_bytes"
            | "fs_create_dir"
            | "fs_exists"
            | "fs_list"
            | "fs_is_dir"
            | "fs_open_lines"
            | "fs_read_line"
            | "fs_close_lines"
            | "env_get"
            | "env_args"
            | "http_send"
            | "http_send_all"
            | "secure_random_bytes"
            | "constant_time_eq"
            | "log_write",
            _,
        ) => Err(wrong(name)),
        _ => return None,
    })
}

/// The `Request` map of the prelude read as an [`HttpRequest`].
fn request_of(fields: &[(HostValue, HostValue)]) -> HttpRequest {
    let text = |key: &str| {
        fields.iter().find_map(|(k, v)| match (k, v) {
            (HostValue::Str(k), HostValue::Str(v)) if k == key => Some(v.clone()),
            _ => None,
        })
    };
    let headers = fields
        .iter()
        .find_map(|(k, v)| match (k, v) {
            (HostValue::Str(k), HostValue::Map(h)) if k == "headers" => Some(h),
            _ => None,
        })
        .map(|h| {
            h.iter()
                .filter_map(|(k, v)| match (k, v) {
                    (HostValue::Str(k), HostValue::Str(v)) => Some((k.clone(), v.clone())),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    HttpRequest {
        method: text("method").unwrap_or_default(),
        url: text("url").unwrap_or_default(),
        headers,
        body: text("body"),
        body_bytes: fields.iter().find_map(|(k, v)| match (k, v) {
            (HostValue::Str(k), HostValue::Bytes(b)) if k == "raw" => Some(b.clone()),
            _ => None,
        }),
    }
}

/// An answer as the `Response` map of the prelude. A request that failed (no connection, a
/// timeout) is a response with status `0` whose body is the reason: that is how `http.send_all`
/// returns many answers at once.
fn response_value(r: Result<HttpResponse, HostError>) -> HostValue {
    let (status, headers, body, raw) = match r {
        Ok(r) => (r.status, r.headers, r.body, r.raw),
        Err(e) => {
            let why = match e {
                HostError::Unsupported(what) => format!("{what} is not available here"),
                HostError::Other(m) => m,
            };
            (0, Vec::new(), why, None)
        }
    };
    HostValue::Map(vec![
        (HostValue::Str("status".into()), HostValue::Int(status)),
        (
            HostValue::Str("headers".into()),
            HostValue::Map(
                headers
                    .into_iter()
                    .map(|(k, v)| (HostValue::Str(k), HostValue::Str(v)))
                    .collect(),
            ),
        ),
        (HostValue::Str("body".into()), HostValue::Str(body)),
        (
            HostValue::Str("raw".into()),
            raw.map_or(HostValue::Null, HostValue::Bytes),
        ),
    ])
}

/// The names of the `extern fun` the prelude's `Db` is built on.
pub const SQL_EXTERNS: [&str; 4] = ["sql_connect", "sql_query", "sql_execute", "sql_close"];

/// Runs one of the [`SQL_EXTERNS`] on a [`SqlBackend`]; `None` if `name` is not one of
/// them. Values follow the signatures declared in the prelude: `Json` parameters become
/// [`SqlValue`]s (arrays and objects, which a column cannot hold, bind as `NULL`), and
/// columns come back as `Json` (numbers are floats).
pub fn sql_call(
    db: &mut dyn SqlBackend,
    name: &str,
    args: Vec<HostValue>,
) -> Option<Result<HostValue, HostError>> {
    fn bad(name: &str) -> HostError {
        HostError::Other(format!("`{name}` was called with the wrong arguments"))
    }
    fn to_sql(v: &HostValue) -> SqlValue {
        match v {
            HostValue::Str(s) => SqlValue::Text(s.clone()),
            HostValue::Float(f) => SqlValue::Float(*f),
            HostValue::Int(i) => SqlValue::Int(*i),
            HostValue::Bool(b) => SqlValue::Bool(*b),
            _ => SqlValue::Null,
        }
    }
    fn from_sql(v: &SqlValue) -> HostValue {
        match v {
            SqlValue::Null => HostValue::Null,
            SqlValue::Int(i) => HostValue::Float(*i as f64),
            SqlValue::Float(f) => HostValue::Float(*f),
            SqlValue::Text(s) => HostValue::Str(s.clone()),
            SqlValue::Bool(b) => HostValue::Bool(*b),
        }
    }
    let params = |v: &HostValue| match v {
        HostValue::Array(items) => items.iter().map(to_sql).collect(),
        _ => Vec::new(),
    };
    Some(match (name, args.as_slice()) {
        ("sql_connect", [HostValue::Str(url)]) => {
            db.sql_connect(url).map(|id| HostValue::Int(id as i64))
        }
        ("sql_query", [HostValue::Int(c), HostValue::Str(sql), p]) => {
            db.sql_query(*c as u64, sql, params(p)).map(|rows| {
                HostValue::Array(
                    rows.iter()
                        .map(|row| {
                            HostValue::Map(
                                row.columns
                                    .iter()
                                    .map(|(n, v)| (HostValue::Str(n.clone()), from_sql(v)))
                                    .collect(),
                            )
                        })
                        .collect(),
                )
            })
        }
        ("sql_execute", [HostValue::Int(c), HostValue::Str(sql), p]) => db
            .sql_execute(*c as u64, sql, params(p))
            .map(|n| HostValue::Int(n as i64)),
        ("sql_close", [HostValue::Int(c)]) => {
            db.sql_close(*c as u64).map(|()| HostValue::Bool(true))
        }
        (n, _) if SQL_EXTERNS.contains(&n) => Err(bad(n)),
        _ => return None,
    })
}

/// A request given to [`Host::http_send`]: the fields of the prelude's `Request` struct,
/// read by the interpreter (it does not depend on `qlang-core`'s struct layout).
#[derive(Clone, Debug)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    /// `(name, value)` pairs, in the order given.
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    /// A body that is not text (`Request.raw`); it wins over `body`.
    pub body_bytes: Option<Vec<u8>>,
}

/// What a host returns on success: the fields of the prelude's `Response` struct.
#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: i64,
    /// `(name, value)` pairs. Names are in lower case.
    pub headers: Vec<(String, String)>,
    pub body: String,
    /// The exact bytes of the body, when they are not valid text (`body` is then lossy).
    pub raw: Option<Vec<u8>>,
}

/// A value crossing the boundary with an `extern fun`: plain data, always copied.
/// A qlang struct is a `Map` with its field names as `Str` keys, an enum value is the
/// `Str` name of its variant, and `Json` is the matching JSON-shaped value (numbers are
/// `Float`).
#[derive(Clone, Debug, PartialEq)]
pub enum HostValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    /// Bytes. To an external command they are a base64 string.
    Bytes(Vec<u8>),
    Array(Vec<HostValue>),
    /// Entries in insertion order.
    Map(Vec<(HostValue, HostValue)>),
}

pub trait Host {
    /// Output text, exactly as given (newlines included).
    fn print(&mut self, text: &str);

    /// Output text on the error output. Hosts without one drop it.
    fn eprint(&mut self, _text: &str) {}

    /// Read one line of input (without its newline). `Ok(None)` is end of input.
    fn read_line(&mut self) -> Result<Option<String>, HostError> {
        Err(HostError::Unsupported("read"))
    }

    /// The current time: milliseconds since 1970-01-01 00:00:00 UTC.
    fn now_ms(&mut self) -> Result<i64, HostError> {
        Err(HostError::Unsupported("time"))
    }

    /// Wait `ms` milliseconds. A host with a virtual clock may move it forward instead.
    fn sleep_ms(&mut self, _ms: u64) -> Result<(), HostError> {
        Err(HostError::Unsupported("time"))
    }

    /// 64 random bits. Hosts that offer a seed return the same sequence for the same seed.
    fn random_u64(&mut self) -> Result<u64, HostError> {
        Err(HostError::Unsupported("random"))
    }

    /// Bytes that nobody can predict (tokens, salts, keys), from the operating system.
    /// Unlike `random_u64` they never follow a seed. A host for tests may answer from its
    /// seed; one that has no such source says so.
    fn secure_random(&mut self, _n: usize) -> Result<Vec<u8>, HostError> {
        Err(HostError::Unsupported("random"))
    }

    /// One line of the program's log (`Log.info(...)`): `level` is `debug`, `info`, `warn` or
    /// `error`. A host with no place to write it drops the line.
    fn log(&mut self, _level: &str, _message: &str, _fields: &[(String, String)]) {}

    /// Source text of a module, for `import`. `path` is already normalised.
    fn load_module(&mut self, _path: &str) -> Result<String, String> {
        Err("modules are not available in this environment".to_string())
    }

    /// Send an HTTP request and wait for the response. A failed request (DNS, timeout,
    /// connection refused...) is `Err`, same as a host that cannot reach the network at
    /// all; the caller turns both into a qlang `Result::err`, never a crash.
    fn http_send(&mut self, _req: HttpRequest) -> Result<HttpResponse, HostError> {
        Err(HostError::Unsupported("http"))
    }

    /// Send several requests and give the answers in the same order. A host may do them at
    /// the same time; the default does one after the other.
    fn http_send_all(&mut self, reqs: Vec<HttpRequest>) -> Vec<Result<HttpResponse, HostError>> {
        reqs.into_iter().map(|r| self.http_send(r)).collect()
    }

    /// Read a file's whole content as text. A missing file, a permission error, or content
    /// that is not valid UTF-8 are all `Err`, never a crash.
    fn read_file(&mut self, _path: &str) -> Result<String, HostError> {
        Err(HostError::Unsupported("fs"))
    }

    /// Write `content` to a file, replacing it entirely (creating it if needed).
    fn write_file(&mut self, _path: &str, _content: &str) -> Result<(), HostError> {
        Err(HostError::Unsupported("fs"))
    }

    /// Create a folder and the ones above it that are missing (nothing to do if it exists).
    fn create_dir(&mut self, _path: &str) -> Result<(), HostError> {
        Err(HostError::Unsupported("fs"))
    }

    /// Whether a file or a folder is there.
    fn path_exists(&mut self, _path: &str) -> Result<bool, HostError> {
        Err(HostError::Unsupported("fs"))
    }

    /// Whether `path` is a folder.
    fn is_dir(&mut self, _path: &str) -> Result<bool, HostError> {
        Err(HostError::Unsupported("fs"))
    }

    /// The names (not the paths) of what a folder holds, sorted.
    fn list_dir(&mut self, _path: &str) -> Result<Vec<String>, HostError> {
        Err(HostError::Unsupported("fs"))
    }

    /// Opens a text file to be read one line at a time; the handle goes to `read_file_line`.
    fn open_lines(&mut self, _path: &str) -> Result<u64, HostError> {
        Err(HostError::Unsupported("fs"))
    }

    /// The next line of an open file without its line break, `None` at the end (the file is then
    /// closed).
    fn read_file_line(&mut self, _handle: u64) -> Result<Option<String>, HostError> {
        Err(HostError::Unsupported("fs"))
    }

    /// Closes a file opened by `open_lines` (nothing happens if it is already closed).
    fn close_lines(&mut self, _handle: u64) -> Result<(), HostError> {
        Err(HostError::Unsupported("fs"))
    }

    /// Read a file's whole content as bytes. By default, the text of `read_file`.
    fn read_file_bytes(&mut self, path: &str) -> Result<Vec<u8>, HostError> {
        self.read_file(path).map(String::into_bytes)
    }

    /// Write bytes to a file, replacing it entirely. By default only text can be written.
    fn write_file_bytes(&mut self, path: &str, content: &[u8]) -> Result<(), HostError> {
        match std::str::from_utf8(content) {
            Ok(text) => self.write_file(path, text),
            Err(_) => Err(HostError::Other(
                "this environment can only write text files".to_string(),
            )),
        }
    }

    /// The value of an environment variable, or `Ok(None)` if it is not set.
    fn env_var(&mut self, _name: &str) -> Result<Option<String>, HostError> {
        Err(HostError::Unsupported("env"))
    }

    /// The program's command-line arguments (not including the program name itself).
    fn args(&mut self) -> Result<Vec<String>, HostError> {
        Err(HostError::Unsupported("env"))
    }

    /// Whether this environment provides the `extern fun` called `name`. Asked at compile
    /// time, so that using a missing one is reported before running.
    fn has_extern(&self, name: &str) -> bool {
        std_has(self.capabilities(), name)
    }

    /// Run the `extern fun` called `name`. `Err` is turned into a `Result::err` when the
    /// function returns a `Result<T, Error>`, and stops the program otherwise.
    fn call(&mut self, name: &str, args: Vec<HostValue>) -> Result<HostValue, HostError> {
        std_call(self, name, args).unwrap_or(Err(HostError::Unsupported("extern")))
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    /// The file `@name` (a package imported without a file) stands for, as `@name/file.q`:
    /// the package says which of its files is the main one. Other paths are returned
    /// unchanged. By default packages have no main file.
    fn package_file(&mut self, path: &str) -> String {
        path.to_string()
    }

    /// A name that is the same for the same module however its path was written
    /// (`lib/a.q`, `lib/../lib/a.q`, a symbolic link...). Used to load a module once
    /// and to detect circular imports. By default the normalised path itself.
    fn canonical_path(&mut self, path: &str) -> String {
        path.to_string()
    }
}

/// Resource limits for one run.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Maximum number of evaluation steps.
    pub max_steps: u64,
    /// Maximum call depth.
    pub max_depth: usize,
    /// Maximum number of output bytes.
    pub max_output: usize,
    /// Maximum size of one array (elements) or string (bytes).
    pub max_alloc: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_steps: 100_000_000,
            max_depth: 1_000,
            max_output: 10_000_000,
            max_alloc: 10_000_000,
        }
    }
}

/// How much memory the process uses, and the most a run may use. The core cannot count
/// memory itself (values are shared and freed all over), so the program that embeds it says
/// how, once: the terminal program counts its allocations. Without it, only the other limits
/// apply (they bound a run to a few gigabytes, which is too much for a server).
static MEMORY: std::sync::OnceLock<(fn() -> usize, usize)> = std::sync::OnceLock::new();

/// Stops runs (`R904`) when `probe()`, the bytes in use, goes over `max_bytes`. Call it once,
/// at start-up; a second call is ignored.
pub fn set_memory_limit(probe: fn() -> usize, max_bytes: usize) {
    let _ = MEMORY.set((probe, max_bytes));
}

/// Whether the memory in use is over the limit given to [`set_memory_limit`].
pub fn memory_exceeded() -> bool {
    MEMORY.get().is_some_and(|(probe, max)| probe() > *max)
}

thread_local! {
    static CANCEL: std::cell::RefCell<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>> =
        const { std::cell::RefCell::new(None) };
}

/// Gives the thread a flag that stops the program running on it (`R906`) at its next check, the one
/// that also looks at the memory. `None` removes it. A task started by `Worker.spawn` has one.
pub fn set_cancel_flag(flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>) {
    CANCEL.with(|c| *c.borrow_mut() = flag);
}

/// Whether the flag of this thread (see [`set_cancel_flag`]) is raised.
pub fn cancelled() -> bool {
    CANCEL.with(|c| {
        c.borrow()
            .as_ref()
            .is_some_and(|f| f.load(std::sync::atomic::Ordering::Relaxed))
    })
}

/// A host that keeps everything in memory. Used by tests and by web servers.
#[derive(Default)]
pub struct MemoryHost {
    pub output: String,
    /// What the program wrote with `eprint` and `ewrite`.
    pub errors: String,
    /// The lines of the program's log: `level message name=value...`.
    pub logs: Vec<String>,
    pub input: std::collections::VecDeque<String>,
    pub files: std::collections::HashMap<String, String>,
    pub allow_read: bool,
    /// The clock, in milliseconds since 1970. `None`: there is no clock.
    pub clock: Option<i64>,
    /// State of the random generator. `None`: there is no randomness.
    pub seed: Option<u64>,
    /// Canned responses for `http.send`, keyed by `(method, url)`. `None`: no network.
    pub http: Option<std::collections::HashMap<(String, String), Result<HttpResponse, String>>>,
    /// Canned results for `sql_query`/`sql_execute`, keyed by the exact SQL text given.
    /// `None`: no database. A query not in the map is `Ok(&[])` (easier to seed in tests
    /// than making every untested statement an error).
    pub sql: Option<std::collections::HashMap<String, Result<Vec<SqlRow>, String>>>,
    /// Set by `sql_close`, so a later `sql_query`/`sql_execute` can report the connection
    /// is gone, the same way a real driver would.
    pub sql_closed: bool,
    /// In-memory filesystem for `fs.read`/`fs.write`, keyed by path. `None`: no filesystem.
    /// A path not in the map is a "not found" error, same as a real one.
    pub fs: Option<std::collections::HashMap<String, String>>,
    /// Environment variables for `env.get`/`env.args`. `None`: no process environment.
    pub env: Option<std::collections::HashMap<String, String>>,
    pub cli_args: Vec<String>,
    /// Functions provided to `extern fun`, by name.
    pub externs: std::collections::HashMap<String, ExternFn>,
    /// Treat every `extern fun` as provided (only to check a program that is not going
    /// to run here, like an editor does: it must not report what the real host may have).
    pub assume_externs: bool,
    /// For each package, the file `@name` stands for (see `Host::package_file`).
    pub package_mains: std::collections::HashMap<String, String>,
}

/// An `extern fun` implemented in Rust, for [`MemoryHost`].
pub type ExternFn = Box<dyn FnMut(Vec<HostValue>) -> Result<HostValue, HostError> + Send>;

impl MemoryHost {
    pub fn new() -> MemoryHost {
        MemoryHost::default()
    }

    pub fn with_input(lines: &[&str]) -> MemoryHost {
        MemoryHost {
            input: lines.iter().map(|s| s.to_string()).collect(),
            allow_read: true,
            ..MemoryHost::default()
        }
    }
}

impl Host for MemoryHost {
    fn print(&mut self, text: &str) {
        self.output.push_str(text);
    }

    fn eprint(&mut self, text: &str) {
        self.errors.push_str(text);
    }

    fn read_line(&mut self) -> Result<Option<String>, HostError> {
        if !self.allow_read {
            return Err(HostError::Unsupported("read"));
        }
        Ok(self.input.pop_front())
    }

    fn load_module(&mut self, path: &str) -> Result<String, String> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| format!("module \"{path}\" not found"))
    }

    fn package_file(&mut self, path: &str) -> String {
        match path
            .strip_prefix('@')
            .and_then(|n| self.package_mains.get(n))
        {
            Some(main) => format!("{path}/{main}"),
            None => path.to_string(),
        }
    }

    fn has_extern(&self, name: &str) -> bool {
        self.assume_externs
            || self.externs.contains_key(name)
            || (self.sql.is_some() && SQL_EXTERNS.contains(&name))
            || std_has(self.capabilities(), name)
    }

    fn call(&mut self, name: &str, args: Vec<HostValue>) -> Result<HostValue, HostError> {
        if let Some(f) = self.externs.get_mut(name) {
            return f(args);
        }
        if SQL_EXTERNS.contains(&name) {
            return sql_call(self, name, args).unwrap_or(Err(HostError::Unsupported("extern")));
        }
        std_call(self, name, args).unwrap_or(Err(HostError::Unsupported("extern")))
    }

    fn log(&mut self, level: &str, message: &str, fields: &[(String, String)]) {
        let mut line = format!("{level} {message}");
        for (k, v) in fields {
            line.push_str(&format!(" {k}={v}"));
        }
        self.logs.push(line);
    }

    fn secure_random(&mut self, n: usize) -> Result<Vec<u8>, HostError> {
        // a host for tests: the bytes follow the seed (never use this for real secrets)
        let mut state = self.seed.ok_or(HostError::Unsupported("random"))?;
        let out = (0..n).map(|_| splitmix64(&mut state) as u8).collect();
        self.seed = Some(state);
        Ok(out)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            read: self.allow_read,
            time: self.clock.is_some(),
            random: self.seed.is_some(),
            http: self.http.is_some(),
            fs: self.fs.is_some(),
            env: self.env.is_some(),
        }
    }

    fn http_send(&mut self, req: HttpRequest) -> Result<HttpResponse, HostError> {
        let table = self.http.as_ref().ok_or(HostError::Unsupported("http"))?;
        match table.get(&(req.method, req.url)) {
            Some(Ok(r)) => Ok(r.clone()),
            Some(Err(m)) => Err(HostError::Other(m.clone())),
            None => Err(HostError::Other(
                "no canned response for this request".to_string(),
            )),
        }
    }

    fn read_file(&mut self, path: &str) -> Result<String, HostError> {
        let table = self.fs.as_ref().ok_or(HostError::Unsupported("fs"))?;
        table
            .get(path)
            .cloned()
            .ok_or_else(|| HostError::Other(format!("\"{path}\" not found")))
    }

    fn write_file(&mut self, path: &str, content: &str) -> Result<(), HostError> {
        let table = self.fs.as_mut().ok_or(HostError::Unsupported("fs"))?;
        table.insert(path.to_string(), content.to_string());
        Ok(())
    }

    fn env_var(&mut self, name: &str) -> Result<Option<String>, HostError> {
        let table = self.env.as_ref().ok_or(HostError::Unsupported("env"))?;
        Ok(table.get(name).cloned())
    }

    fn args(&mut self) -> Result<Vec<String>, HostError> {
        if self.env.is_none() {
            return Err(HostError::Unsupported("env"));
        }
        Ok(self.cli_args.clone())
    }

    fn sleep_ms(&mut self, ms: u64) -> Result<(), HostError> {
        // the clock is virtual: waiting moves it forward, instantly
        let now = self.clock.as_mut().ok_or(HostError::Unsupported("time"))?;
        *now = now.saturating_add(ms as i64);
        Ok(())
    }

    fn now_ms(&mut self) -> Result<i64, HostError> {
        self.clock.ok_or(HostError::Unsupported("time"))
    }

    fn random_u64(&mut self) -> Result<u64, HostError> {
        match &mut self.seed {
            Some(state) => Ok(splitmix64(state)),
            None => Err(HostError::Unsupported("random")),
        }
    }
}

/// A small deterministic generator (SplitMix64), for hosts that replay a seed.
pub fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl SqlBackend for MemoryHost {
    fn sql_connect(&mut self, _url: &str) -> Result<u64, HostError> {
        if self.sql.is_some() {
            self.sql_closed = false;
            Ok(1)
        } else {
            Err(HostError::Unsupported("sql"))
        }
    }

    fn sql_query(
        &mut self,
        _conn: u64,
        sql: &str,
        _params: Vec<SqlValue>,
    ) -> Result<Vec<SqlRow>, HostError> {
        if self.sql_closed {
            return Err(HostError::Other(
                "this database connection is closed".to_string(),
            ));
        }
        let table = self.sql.as_ref().ok_or(HostError::Unsupported("sql"))?;
        match table.get(sql) {
            Some(Ok(rows)) => Ok(rows.clone()),
            Some(Err(m)) => Err(HostError::Other(m.clone())),
            None => Ok(vec![]),
        }
    }

    fn sql_execute(
        &mut self,
        _conn: u64,
        sql: &str,
        _params: Vec<SqlValue>,
    ) -> Result<u64, HostError> {
        if self.sql_closed {
            return Err(HostError::Other(
                "this database connection is closed".to_string(),
            ));
        }
        let table = self.sql.as_ref().ok_or(HostError::Unsupported("sql"))?;
        match table.get(sql) {
            Some(Ok(rows)) => Ok(rows.len() as u64),
            Some(Err(m)) => Err(HostError::Other(m.clone())),
            None => Ok(0),
        }
    }

    fn sql_close(&mut self, _conn: u64) -> Result<(), HostError> {
        if self.sql.is_none() {
            return Err(HostError::Unsupported("sql"));
        }
        self.sql_closed = true;
        Ok(())
    }
}
