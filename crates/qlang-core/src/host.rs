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
    /// `Db.connect(...)` is available (the host can reach a database).
    pub sql: bool,
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

/// A request given to [`Host::http_send`]: the fields of the prelude's `Request` struct,
/// read by the interpreter (it does not depend on `qlang-core`'s struct layout).
#[derive(Clone, Debug)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    /// `(name, value)` pairs, in the order given.
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

/// What a host returns on success: the fields of the prelude's `Response` struct.
#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: i64,
    /// `(name, value)` pairs. Names are in lower case.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

pub trait Host {
    /// Output text, exactly as given (newlines included).
    fn print(&mut self, text: &str);

    /// Read one line of input (without its newline). `Ok(None)` is end of input.
    fn read_line(&mut self) -> Result<Option<String>, HostError> {
        Err(HostError::Unsupported("read"))
    }

    /// The current time: milliseconds since 1970-01-01 00:00:00 UTC.
    fn now_ms(&mut self) -> Result<i64, HostError> {
        Err(HostError::Unsupported("time"))
    }

    /// 64 random bits. Hosts that offer a seed return the same sequence for the same seed.
    fn random_u64(&mut self) -> Result<u64, HostError> {
        Err(HostError::Unsupported("random"))
    }

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

    /// Open a connection (`sqlite://path`, `postgres://...`...), returning a handle for
    /// the other `sql_*` methods. What the handle means is entirely up to the host.
    fn sql_connect(&mut self, _url: &str) -> Result<u64, HostError> {
        Err(HostError::Unsupported("sql"))
    }

    /// Run a statement that returns rows (`SELECT`), with positional `?` parameters.
    fn sql_query(&mut self, _conn: u64, _sql: &str, _params: Vec<SqlValue>) -> Result<Vec<SqlRow>, HostError> {
        Err(HostError::Unsupported("sql"))
    }

    /// Run a statement that does not return rows (`INSERT`/`UPDATE`/`DELETE`/DDL),
    /// returning the number of rows affected.
    fn sql_execute(&mut self, _conn: u64, _sql: &str, _params: Vec<SqlValue>) -> Result<u64, HostError> {
        Err(HostError::Unsupported("sql"))
    }

    /// Close a connection opened by `sql_connect`.
    fn sql_close(&mut self, _conn: u64) -> Result<(), HostError> {
        Err(HostError::Unsupported("sql"))
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

    /// The value of an environment variable, or `Ok(None)` if it is not set.
    fn env_var(&mut self, _name: &str) -> Result<Option<String>, HostError> {
        Err(HostError::Unsupported("env"))
    }

    /// The program's command-line arguments (not including the program name itself).
    fn args(&mut self) -> Result<Vec<String>, HostError> {
        Err(HostError::Unsupported("env"))
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
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

/// A host that keeps everything in memory. Used by tests and by web servers.
#[derive(Default)]
pub struct MemoryHost {
    pub output: String,
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
}

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

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            read: self.allow_read,
            time: self.clock.is_some(),
            random: self.seed.is_some(),
            http: self.http.is_some(),
            sql: self.sql.is_some(),
            fs: self.fs.is_some(),
            env: self.env.is_some(),
        }
    }

    fn http_send(&mut self, req: HttpRequest) -> Result<HttpResponse, HostError> {
        let table = self.http.as_ref().ok_or(HostError::Unsupported("http"))?;
        match table.get(&(req.method, req.url)) {
            Some(Ok(r)) => Ok(r.clone()),
            Some(Err(m)) => Err(HostError::Other(m.clone())),
            None => Err(HostError::Other("no canned response for this request".to_string())),
        }
    }

    fn sql_connect(&mut self, _url: &str) -> Result<u64, HostError> {
        if self.sql.is_some() {
            self.sql_closed = false;
            Ok(1)
        } else {
            Err(HostError::Unsupported("sql"))
        }
    }

    fn sql_query(&mut self, _conn: u64, sql: &str, _params: Vec<SqlValue>) -> Result<Vec<SqlRow>, HostError> {
        if self.sql_closed {
            return Err(HostError::Other("this database connection is closed".to_string()));
        }
        let table = self.sql.as_ref().ok_or(HostError::Unsupported("sql"))?;
        match table.get(sql) {
            Some(Ok(rows)) => Ok(rows.clone()),
            Some(Err(m)) => Err(HostError::Other(m.clone())),
            None => Ok(vec![]),
        }
    }

    fn sql_execute(&mut self, _conn: u64, sql: &str, _params: Vec<SqlValue>) -> Result<u64, HostError> {
        if self.sql_closed {
            return Err(HostError::Other("this database connection is closed".to_string()));
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

    fn read_file(&mut self, path: &str) -> Result<String, HostError> {
        let table = self.fs.as_ref().ok_or(HostError::Unsupported("fs"))?;
        table.get(path).cloned().ok_or_else(|| HostError::Other(format!("\"{path}\" not found")))
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
