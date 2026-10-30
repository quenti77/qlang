//! The `qlang` command line tool: runs programs in a terminal, speaks JSON
//! for other programs (PHP, Python...), and can serve that JSON over HTTP.

mod db;
mod devpage;
mod docgen;
mod fmtcmd;
mod index;
mod init;
mod install;
mod json;
mod logging;
mod lsp;
mod mem;
mod packages;
mod password;
mod process;
mod providers;
mod redis;
mod selfupdate;
mod serve;
mod signature;
mod stop;
mod term;
mod testrun;
mod ws;

use qlang_core::check::compile;
use qlang_core::host::{
    Capabilities, Host, HostError, HostValue, HttpRequest, HttpResponse, Limits, SQL_EXTERNS,
    SqlBackend, SqlRow, SqlValue, sql_call,
};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
qlang - a small, readable language

USAGE:
    qlang init [dir]                     start a project: qlang.toml, main.q, .gitignore
    qlang init --web [dir]               start a web application: routes, services, a test, and
                                         the `std` package (--no-install: do not fetch it)
    qlang doc [path...] [-o FILE]        Markdown documentation of the `--\" ... --\"` comments of the
                                         project's files (or of the paths given)
    qlang fmt [path...] [--check]        lay the files out (two spaces per level; only the white
                                         space at the start and end of lines changes); --check
                                         lists the files that would change and exits 1
    qlang test [path...] [--filter TEXT] [--seed N] [--max-steps N] [--trust-externs]
                                         run the `fun test_...()` of the *_test.q / test_*.q
                                         files of the project (or of the paths given)
    qlang run [file.q] [--max-steps N] [--root DIR] [--seed N] [--extern CMD] [--trust-externs]
                    [--allow-process] [-- ARGS...]
                                         run a program (imports may live anywhere, or only
                                         under DIR with --root; the same --seed gives the
                                         same random numbers; ARGS after -- are env.args();
                                         --extern CMD starts a provider of `extern fun`, as
                                         do the [externs] of a qlang.toml near the program,
                                         once you agreed to them: --trust-externs says yes;
                                         --allow-process lets the program start programs)
    qlang check [file.q] [--extern CMD]  check a program without running it (no file: the `entry`
                                         of qlang.toml's [package], for run too)
    qlang check --json                   same, with a JSON request on stdin (used by editors)
    qlang run --json                     read a JSON request on stdin, write a JSON response
    qlang serve [--port 8080] [--bind 127.0.0.1]
                                         serve the JSON protocol over HTTP (POST /run)
    qlang serve <app.q> [--port 8080] [--bind 127.0.0.1] [--static DIR] [--workers N]
                        [--max-steps N] [--root DIR] [--seed N] [--watch] [--dev] [--compress]
                        [--max-connections N] [--db-pool N] [--max-body SIZE] [--max-headers SIZE]
                        [--read-timeout S] [--idle-timeout S] [--no-access-log] [--dev-public]
                                         serve a qlang program: its `handle(req: Request) ->
                                         Response` function answers every HTTP request
                                         (--watch: reload when a file of the program changes;
                                         --dev: show the error of the program in the 500 response;
                                         --compress: gzip text answers)
    qlang lsp                            language server (Language Server Protocol, on standard
                                         input and output): errors, hovers, completion, go to
                                         definition, references, rename, symbols
    qlang self-update [--version X.Y.Z] [--check]
                                         replace qlang by the latest release (or by X.Y.Z); the
                                         download is checked against the release's SHA256SUMS
    qlang packages                       the packages of the project here, as JSON (for editors)
    qlang install                        fetch the git packages listed in qlang.toml into
                                         .qlang/packages, at the commits of qlang.lock
    qlang update [name...]               move them (all, or those named) to what their
                                         tag/branch points to now, and rewrite qlang.lock
    qlang add <name> --git URL [--tag T | --rev R | --branch B]
    qlang add <name> --path DIR          add a dependency to qlang.toml and install it
    qlang add <name>                     the same, from the package index (the latest tag)
    qlang search [text]                  the packages of the index
    qlang remove <name>                  remove a dependency
    qlang --help | --version

OPTIONS FOR EVERY COMMAND:
    --max-memory MB                      stop a program that holds more than MB megabytes (R904;
                                         default 1024, or $QLANG_MAX_MEMORY; 0: no limit)
    --log-level LEVEL                    hide `Log` lines below debug|info|warn|error (default info,
                                         or $QLANG_LOG)
    --log-format text|json               how `Log` lines are written on the error output (or
                                         $QLANG_LOG_FORMAT)
";

#[global_allocator]
static ALLOCATOR: mem::Counting = mem::Counting;

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = mem::configure(&mut args) {
        eprintln!("{e}");
        return ExitCode::from(64);
    }
    if let Err(e) = logging::configure(&mut args) {
        eprintln!("{e}");
        return ExitCode::from(64);
    }
    // run on a big stack: the interpreter recurses for nested calls
    let handle = std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(move || real_main(args))
        .expect("cannot start the interpreter thread");
    let code = match handle.join() {
        Ok(code) => code,
        Err(_) => ExitCode::from(101),
    };
    term::restore();
    code
}

fn real_main(args: Vec<String>) -> ExitCode {
    let mut it = args.iter().map(String::as_str);
    match it.next() {
        Some("run") => {
            let rest: Vec<&str> = it.collect();
            if rest.contains(&"--json") {
                json::run_stdin(false)
            } else {
                run_file(&rest, true)
            }
        }
        Some("check") => {
            let rest: Vec<&str> = it.collect();
            if rest.contains(&"--json") {
                json::run_stdin(true)
            } else {
                run_file(&rest, false)
            }
        }
        Some("serve") => serve::serve(&it.collect::<Vec<_>>()),
        Some("lsp") => lsp::run(),
        Some("packages") => packages_json(),
        Some("init") => init::init(&it.collect::<Vec<_>>()),
        Some("test") => testrun::run(&it.collect::<Vec<_>>()),
        Some("doc") => docgen::run(&it.collect::<Vec<_>>()),
        Some("fmt") => fmtcmd::run(&it.collect::<Vec<_>>()),
        Some("self-update") => selfupdate::run(&it.collect::<Vec<_>>()),
        Some("search") => index::search(it.next()),
        Some(cmd @ ("install" | "update" | "add" | "remove")) => {
            package_command(cmd, &it.collect::<Vec<_>>())
        }
        Some("--version") | Some("-V") => {
            println!("qlang {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--help") | Some("-h") | None => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unknown command `{other}`\n\n{USAGE}");
            ExitCode::from(64)
        }
    }
}

/// The biggest file `fs.read` accepts.
const MAX_FS_READ: u64 = 64 << 20;

/// A host connected to the terminal and the disk. Imports can reach any `.q` file the
/// user can read (like `require` in Node), unless `--root` confines them to a directory.
struct TerminalHost {
    root: Option<PathBuf>,
    /// State of the random generator: `--seed N`, or taken from the system.
    rng: u64,
    /// Open SQL connections, for `Db.connect`.
    db: db::SqlConns,
    redis: redis::Pool,
    /// The script's own arguments, for `env.args()` (everything after `--` on the command
    /// line), not including the file path or qlang's own options.
    script_args: Vec<String>,
    /// Programs implementing `extern fun` (`qlang.toml`, `--extern`).
    providers: providers::Providers,
    /// The packages `import "@name/..."` can reach (`[dependencies]` of `qlang.toml`).
    packages: packages::Packages,
    /// Treat every `extern fun` as provided (to check or document code without running it).
    assume_externs: bool,
    /// `Process.run` may start programs (`--allow-process`).
    allow_process: bool,
    /// Files opened by `fs.lines`.
    lines: LineFiles,
}

/// The longest line `fs.lines` reads: a file without line breaks must not fill the memory.
const MAX_LINE: u64 = 16 << 20;
/// How many files `fs.lines` keeps open at once.
const MAX_OPEN_LINES: usize = 64;

/// Text files being read one line at a time, by handle.
#[derive(Default)]
struct LineFiles {
    next: u64,
    open: std::collections::HashMap<u64, (std::io::BufReader<std::fs::File>, u64)>,
}

/// Random bits from the system (`/dev/urandom`, or the keys std picks for its hash tables).
fn system_entropy() -> u64 {
    use std::io::Read;
    let mut bytes = [0u8; 8];
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .is_ok()
    {
        return u64::from_le_bytes(bytes);
    }
    use std::hash::{BuildHasher, Hasher};
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish()
}

impl TerminalHost {
    /// `fs.read`/`fs.write` honour `--root` the same way imports do.
    fn check_fs_root(&self, full: &Path) -> Result<(), HostError> {
        if let Some(root) = &self.root
            && !full.starts_with(root)
        {
            return Err(HostError::Other(format!(
                "\"{}\" is outside of --root {}",
                full.display(),
                root.display()
            )));
        }
        Ok(())
    }
}

impl SqlBackend for TerminalHost {
    fn sql_connect(&mut self, url: &str) -> Result<u64, HostError> {
        // `--root` confines the database file like it confines `fs.write` (not what the SQL
        // itself does: `ATTACH DATABASE` reaches any file the user can)
        if let Some(path) = url.strip_prefix("sqlite://")
            && !path.is_empty()
            && path != ":memory:"
            && !path.starts_with("file::memory:")
        {
            let p = Path::new(path);
            let target = if p.exists() {
                p.canonicalize()
            } else {
                p.parent()
                    .filter(|d| !d.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."))
                    .canonicalize()
            }
            .map_err(|e| HostError::Other(e.to_string()))?;
            self.check_fs_root(&target)?;
        }
        self.db.connect(url)
    }

    fn sql_query(
        &mut self,
        conn: u64,
        sql: &str,
        params: Vec<SqlValue>,
    ) -> Result<Vec<SqlRow>, HostError> {
        self.db.query(conn, sql, params)
    }

    fn sql_execute(
        &mut self,
        conn: u64,
        sql: &str,
        params: Vec<SqlValue>,
    ) -> Result<u64, HostError> {
        self.db.execute(conn, sql, params)
    }

    fn sql_close(&mut self, conn: u64) -> Result<(), HostError> {
        self.db.close(conn)
    }
}

impl Host for TerminalHost {
    fn print(&mut self, text: &str) {
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(text.as_bytes());
    }

    fn eprint(&mut self, text: &str) {
        let mut err = std::io::stderr().lock();
        let _ = err.write_all(text.as_bytes());
    }

    fn read_line(&mut self) -> Result<Option<String>, HostError> {
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        match std::io::stdin().lock().read_line(&mut line) {
            Ok(0) => Ok(None),
            Ok(_) => Ok(Some(line.trim_end_matches(['\n', '\r']).to_string())),
            Err(e) => Err(HostError::Other(e.to_string())),
        }
    }

    fn load_module(&mut self, path: &str) -> Result<String, String> {
        if qlang_core::span::is_package_path(path) {
            // a package's files are the project's own choice (`[dependencies]`), so
            // `--root` does not apply to them
            let full = self.packages.resolve(path)?;
            if full.extension().is_none_or(|e| e != "q") {
                return Err(format!(
                    "\"{path}\" is not a qlang file (only .q files can be imported)"
                ));
            }
            return std::fs::read_to_string(&full).map_err(|e| e.to_string());
        }
        // only qlang sources: importing any file would show its lines in error messages
        if !path.ends_with(".q") {
            return Err(format!(
                "\"{path}\" is not a qlang file (only .q files can be imported)"
            ));
        }
        let full = Path::new(path)
            .canonicalize()
            .map_err(|_| format!("file \"{path}\" not found"))?;
        if let Some(root) = &self.root
            && !full.starts_with(root)
        {
            return Err(format!(
                "\"{path}\" is outside of --root {}",
                root.display()
            ));
        }
        std::fs::read_to_string(&full).map_err(|e| e.to_string())
    }

    fn package_file(&mut self, path: &str) -> String {
        self.packages.with_main(path)
    }

    fn canonical_path(&mut self, path: &str) -> String {
        if qlang_core::span::is_package_path(path) {
            // the same file under two names is one module
            return match self.packages.resolve(path) {
                Ok(full) => full.to_string_lossy().into_owned(),
                Err(_) => path.to_string(),
            };
        }
        // the same file reached through `..` or a symbolic link is the same module
        Path::new(path)
            .canonicalize()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string())
    }

    fn now_ms(&mut self) -> Result<i64, HostError> {
        let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
        since
            .map(|d| d.as_millis() as i64)
            .map_err(|e| HostError::Other(e.to_string()))
    }

    fn sleep_ms(&mut self, ms: u64) -> Result<(), HostError> {
        let _ = std::io::stdout().flush();
        std::thread::sleep(std::time::Duration::from_millis(ms));
        Ok(())
    }

    fn random_u64(&mut self) -> Result<u64, HostError> {
        Ok(qlang_core::host::splitmix64(&mut self.rng))
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            read: true,
            time: true,
            random: true,
            http: true,
            fs: true,
            env: true,
        }
    }

    fn log(&mut self, level: &str, message: &str, fields: &[(String, String)]) {
        let now = self.now_ms().ok();
        logging::write(now, level, message, fields);
    }

    fn secure_random(&mut self, n: usize) -> Result<Vec<u8>, HostError> {
        let mut out = vec![0u8; n];
        getrandom::fill(&mut out)
            .map_err(|e| HostError::Other(format!("no source of randomness: {e}")))?;
        Ok(out)
    }

    fn read_file(&mut self, path: &str) -> Result<String, HostError> {
        let bytes = self.read_file_bytes(path)?;
        String::from_utf8(bytes)
            .map_err(|_| HostError::Other("stream did not contain valid UTF-8".to_string()))
    }

    fn read_file_bytes(&mut self, path: &str) -> Result<Vec<u8>, HostError> {
        let full = Path::new(path)
            .canonicalize()
            .map_err(|_| HostError::Other(format!("\"{path}\" not found")))?;
        self.check_fs_root(&full)?;
        // a regular file of a sensible size: not a device (`/dev/zero` never ends) nor a
        // file that would not fit in memory
        let meta = std::fs::metadata(&full).map_err(|e| HostError::Other(e.to_string()))?;
        if !meta.is_file() {
            return Err(HostError::Other(format!(
                "\"{path}\" is not a regular file"
            )));
        }
        if meta.len() > MAX_FS_READ {
            return Err(HostError::Other(format!(
                "\"{path}\" is too big to read ({} MB at most)",
                MAX_FS_READ >> 20
            )));
        }
        std::fs::read(&full).map_err(|e| HostError::Other(e.to_string()))
    }

    fn write_file(&mut self, path: &str, content: &str) -> Result<(), HostError> {
        self.write_file_bytes(path, content.as_bytes())
    }

    fn write_file_bytes(&mut self, path: &str, content: &[u8]) -> Result<(), HostError> {
        let p = Path::new(path);
        let parent = p
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let full_parent = parent
            .canonicalize()
            .map_err(|e| HostError::Other(e.to_string()))?;
        self.check_fs_root(&full_parent)?;
        let Some(name) = p.file_name() else {
            return Err(HostError::Other(format!(
                "\"{path}\" is not a valid file name"
            )));
        };
        let target = full_parent.join(name);
        // the folder was checked, not the file: a link in it must not lead out of `--root`
        if std::fs::symlink_metadata(&target).is_ok_and(|m| m.file_type().is_symlink()) {
            let real = target
                .canonicalize()
                .map_err(|e| HostError::Other(e.to_string()))?;
            self.check_fs_root(&real)?;
            return std::fs::write(real, content).map_err(|e| HostError::Other(e.to_string()));
        }
        std::fs::write(target, content).map_err(|e| HostError::Other(e.to_string()))
    }

    fn is_dir(&mut self, path: &str) -> Result<bool, HostError> {
        match Path::new(path).canonicalize() {
            Ok(full) => {
                self.check_fs_root(&full)?;
                Ok(full.is_dir())
            }
            Err(_) => Ok(false),
        }
    }

    fn list_dir(&mut self, path: &str) -> Result<Vec<String>, HostError> {
        let full = Path::new(path)
            .canonicalize()
            .map_err(|_| HostError::Other(format!("\"{path}\" not found")))?;
        self.check_fs_root(&full)?;
        let entries = std::fs::read_dir(&full).map_err(|e| HostError::Other(e.to_string()))?;
        let mut names: Vec<String> = entries
            .flatten()
            .take(100_000)
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        names.sort();
        Ok(names)
    }

    fn open_lines(&mut self, path: &str) -> Result<u64, HostError> {
        let full = Path::new(path)
            .canonicalize()
            .map_err(|_| HostError::Other(format!("\"{path}\" not found")))?;
        self.check_fs_root(&full)?;
        let meta = std::fs::metadata(&full).map_err(|e| HostError::Other(e.to_string()))?;
        if !meta.is_file() {
            return Err(HostError::Other(format!(
                "\"{path}\" is not a regular file"
            )));
        }
        if self.lines.open.len() >= MAX_OPEN_LINES {
            return Err(HostError::Other(format!(
                "too many files open for reading ({MAX_OPEN_LINES}): close some with `lines.close()`"
            )));
        }
        let file = std::fs::File::open(&full).map_err(|e| HostError::Other(e.to_string()))?;
        self.lines.next += 1;
        let id = self.lines.next;
        self.lines
            .open
            .insert(id, (std::io::BufReader::new(file), 0));
        Ok(id)
    }

    fn read_file_line(&mut self, handle: u64) -> Result<Option<String>, HostError> {
        use std::io::{BufRead, Read};
        let Some((reader, count)) = self.lines.open.get_mut(&handle) else {
            return Err(HostError::Other("this file is closed".to_string()));
        };
        let mut buf = Vec::new();
        let n = reader
            .by_ref()
            .take(MAX_LINE + 1)
            .read_until(b'\n', &mut buf)
            .map_err(|e| HostError::Other(e.to_string()))?;
        if n == 0 {
            self.lines.open.remove(&handle);
            return Ok(None);
        }
        *count += 1;
        let number = *count;
        if buf.len() as u64 > MAX_LINE {
            self.lines.open.remove(&handle);
            return Err(HostError::Other(format!(
                "line {number} is longer than {} MB",
                MAX_LINE >> 20
            )));
        }
        if buf.last() == Some(&b'\n') {
            buf.pop();
            if buf.last() == Some(&b'\r') {
                buf.pop();
            }
        }
        String::from_utf8(buf)
            .map(Some)
            .map_err(|_| HostError::Other(format!("line {number} is not valid UTF-8")))
    }

    fn close_lines(&mut self, handle: u64) -> Result<(), HostError> {
        self.lines.open.remove(&handle);
        Ok(())
    }

    fn create_dir(&mut self, path: &str) -> Result<(), HostError> {
        // the folder may not exist yet: check the nearest one that does, and refuse `..` in
        // what is still to be made (it could climb out of `--root` after the check)
        let mut existing = Path::new(path);
        let mut tail: Vec<&std::ffi::OsStr> = Vec::new();
        while !existing.exists() {
            let Some(name) = existing.file_name() else {
                return Err(HostError::Other(format!(
                    "\"{path}\" is not a valid folder"
                )));
            };
            if name == ".." {
                return Err(HostError::Other(format!(
                    "\"{path}\": `..` after a missing folder"
                )));
            }
            tail.push(name);
            existing = existing
                .parent()
                .filter(|d| !d.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
        }
        let mut full = existing
            .canonicalize()
            .map_err(|e| HostError::Other(e.to_string()))?;
        for name in tail.iter().rev() {
            full.push(name);
        }
        self.check_fs_root(&full)?;
        std::fs::create_dir_all(&full).map_err(|e| HostError::Other(e.to_string()))
    }

    fn path_exists(&mut self, path: &str) -> Result<bool, HostError> {
        match Path::new(path).canonicalize() {
            Ok(full) => {
                self.check_fs_root(&full)?;
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    fn env_var(&mut self, name: &str) -> Result<Option<String>, HostError> {
        Ok(std::env::var(name).ok())
    }

    fn args(&mut self) -> Result<Vec<String>, HostError> {
        Ok(self.script_args.clone())
    }

    fn has_extern(&self, name: &str) -> bool {
        self.assume_externs
            || SQL_EXTERNS.contains(&name)
            || (self.allow_process && name == "process_run")
            || PASSWORD_EXTERNS.contains(&name)
            || redis::REDIS_EXTERNS.contains(&name)
            || signature::SIGNATURE_EXTERNS.contains(&name)
            || name == "process_stopping"
            || term::TERM_EXTERNS.contains(&name)
            || self.providers.has(name)
            || qlang_core::host::std_has(self.capabilities(), name)
    }

    fn call(&mut self, name: &str, args: Vec<HostValue>) -> Result<HostValue, HostError> {
        if self.providers.has(name) {
            return self.providers.call(name, args);
        }
        if name == "process_run" && self.allow_process {
            return process::run(&args);
        }
        if SQL_EXTERNS.contains(&name) {
            return sql_call(self, name, args).unwrap_or(Err(HostError::Unsupported("extern")));
        }
        if PASSWORD_EXTERNS.contains(&name) {
            return password_call(name, &args);
        }
        if signature::SIGNATURE_EXTERNS.contains(&name) {
            return signature::call(name, &args);
        }
        if term::TERM_EXTERNS.contains(&name) {
            return term::call(name, &args);
        }
        if name == "process_stopping" {
            return Ok(HostValue::Bool(stop::stopping(serve::is_stopping())));
        }
        if redis::REDIS_EXTERNS.contains(&name) {
            return redis::call(&mut self.redis, name, &args);
        }
        qlang_core::host::std_call(self, name, args)
            .unwrap_or(Err(HostError::Unsupported("extern")))
    }

    fn http_send(&mut self, req: HttpRequest) -> Result<HttpResponse, HostError> {
        send_http(req)
    }

    fn http_send_all(&mut self, reqs: Vec<HttpRequest>) -> Vec<Result<HttpResponse, HostError>> {
        // at most 16 at a time, the answers in the order of the requests
        let mut out = Vec::with_capacity(reqs.len());
        let mut reqs = reqs.into_iter();
        loop {
            let batch: Vec<HttpRequest> = reqs.by_ref().take(16).collect();
            if batch.is_empty() {
                break;
            }
            std::thread::scope(|scope| {
                let handles: Vec<_> = batch
                    .into_iter()
                    .map(|r| scope.spawn(move || send_http(r)))
                    .collect();
                for h in handles {
                    out.push(
                        h.join().unwrap_or_else(|_| {
                            Err(HostError::Other("the request panicked".into()))
                        }),
                    );
                }
            });
        }
        out
    }
}

fn send_http(req: HttpRequest) -> Result<HttpResponse, HostError> {
    // a non-2xx status is still a response, not a failure
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .http_status_as_error(false)
        .build()
        .into();
    let mut builder = ureq::http::Request::builder()
        .method(req.method.as_str())
        .uri(req.url.as_str());
    for (name, value) in &req.headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    let bad = |e: ureq::http::Error| HostError::Other(e.to_string());
    let result = match (&req.body_bytes, &req.body) {
        (Some(bytes), _) => agent.run(builder.body(bytes.clone()).map_err(bad)?),
        (None, Some(body)) => agent.run(builder.body(body.clone()).map_err(bad)?),
        (None, None) => agent.run(builder.body(()).map_err(bad)?),
    };
    let resp = result.map_err(|e| HostError::Other(e.to_string()))?;
    let status = i64::from(resp.status().as_u16());
    let headers = response_headers(&resp);
    let (body, raw) = read_body(resp)?;
    Ok(HttpResponse {
        status,
        headers,
        body,
        raw,
    })
}

/// The `extern fun` the prelude's `Password` is built on.
const PASSWORD_EXTERNS: [&str; 3] = ["password_hash", "password_verify", "password_needs_rehash"];

fn password_call(name: &str, args: &[HostValue]) -> Result<HostValue, HostError> {
    let wrong = || HostError::Other(format!("`{name}` was called with the wrong arguments"));
    match (name, args) {
        ("password_hash", [HostValue::Str(plain)]) => password::hash(plain)
            .map(HostValue::Str)
            .map_err(HostError::Other),
        ("password_verify", [HostValue::Str(plain), HostValue::Str(hash)]) => {
            password::verify(plain, hash)
                .map(HostValue::Bool)
                .map_err(HostError::Other)
        }
        ("password_needs_rehash", [HostValue::Str(hash)]) => password::needs_rehash(hash)
            .map(HostValue::Bool)
            .map_err(HostError::Other),
        _ => Err(wrong()),
    }
}

/// The body of a response as text, and as bytes when it is not valid text. At most 16 MiB.
#[allow(clippy::type_complexity)]
fn read_body(
    resp: ureq::http::Response<ureq::Body>,
) -> Result<(String, Option<Vec<u8>>), HostError> {
    use std::io::Read;
    let mut data = Vec::new();
    resp.into_body()
        .into_reader()
        .take(16 << 20)
        .read_to_end(&mut data)
        .map_err(|e| HostError::Other(e.to_string()))?;
    Ok(match String::from_utf8(data) {
        Ok(text) => (text, None),
        Err(e) => {
            let bytes = e.into_bytes();
            (String::from_utf8_lossy(&bytes).into_owned(), Some(bytes))
        }
    })
}

/// The headers of a response, with lower-case names.
fn response_headers(resp: &ureq::http::Response<ureq::Body>) -> Vec<(String, String)> {
    resp.headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_ascii_lowercase(), v.to_string()))
        })
        .collect()
}

/// The program `qlang run` / `qlang check` use when no file is given: the `entry` of the
/// `[package]` table of the nearest `qlang.toml`, as a path from the current directory.
fn project_entry() -> Result<Option<String>, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let Some(config) = providers::find_config(&cwd) else {
        return Ok(None);
    };
    let dir = config.parent().expect("a file has a parent").to_path_buf();
    let Some(table) = packages::manifest_of(&dir)? else {
        return Ok(None);
    };
    let Some(entry) = table
        .get("package")
        .and_then(|p| p.get("entry"))
        .and_then(|e| e.as_str())
    else {
        return Ok(None);
    };
    let full = dir.join(entry);
    if !full.is_file() {
        return Err(format!(
            "{}: the entry \"{entry}\" does not exist",
            config.display()
        ));
    }
    // shown relative to where the user is, when it is below
    let canonical_cwd = cwd.canonicalize().unwrap_or(cwd);
    let shown = full
        .canonicalize()
        .ok()
        .and_then(|f| f.strip_prefix(&canonical_cwd).ok().map(Path::to_path_buf))
        .unwrap_or(full);
    Ok(Some(shown.to_string_lossy().into_owned()))
}

/// `qlang packages`: the packages of the project in the current directory, as JSON
/// (`{"mail": {"dir": "/abs/path", "main": "src/mail.q"}}`), for editors.
fn packages_json() -> ExitCode {
    let found = std::env::current_dir()
        .ok()
        .and_then(|cwd| project_dir(&cwd))
        .map(|dir| packages::Packages::load(&dir))
        .unwrap_or_else(|| Ok(packages::Packages::default()));
    match found {
        Ok(p) => {
            println!("{}", p.to_json());
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("{}", serde_json::json!({ "error": e }));
            ExitCode::from(1)
        }
    }
}

/// `qlang install | update [name...] | add name --git URL ... | remove name`.
fn package_command(cmd: &str, args: &[&str]) -> ExitCode {
    let fail = |e: String| {
        eprintln!("{e}");
        ExitCode::from(1)
    };
    let Ok(cwd) = std::env::current_dir() else {
        return fail("cannot read the current directory".to_string());
    };
    let project = match cmd {
        "add" => {
            let Some((name, rest)) = args.split_first() else {
                return fail("usage: qlang add <name> --git URL [--tag T | --rev R | --branch B]\n       qlang add <name> --path DIR".to_string());
            };
            let mut options: Vec<(&str, &str)> = Vec::new();
            let mut it = rest.iter();
            while let Some(flag) = it.next() {
                match (flag.strip_prefix("--"), it.next()) {
                    (Some(key), Some(value)) => options.push((key, value)),
                    _ => {
                        return fail(format!(
                            "`{flag}` needs a value (--git URL, --tag T, --path DIR...)"
                        ));
                    }
                }
            }
            // `qlang add std`: no source given, so the index says where the package lives
            let mut from_index: Vec<(String, String)> = Vec::new();
            if options.is_empty() {
                match index::find(name) {
                    Some(entry) => {
                        from_index.push(("git".into(), entry.git.clone()));
                        match index::latest_tag(&entry.git) {
                            Some(tag) => {
                                println!("{name}: {} at {tag} (from the package index)", entry.git);
                                from_index.push(("tag".into(), tag));
                            }
                            None => println!(
                                "{name}: {} (from the package index; no version tag found, the default branch is used)",
                                entry.git
                            ),
                        }
                    }
                    None => {
                        return fail(format!(
                            "`{name}` is not in the package index: give a source (--git URL or --path DIR), or look with `qlang search`"
                        ));
                    }
                }
            }
            let options: Vec<(&str, &str)> = if from_index.is_empty() {
                options
            } else {
                from_index
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect()
            };
            match install::add(&cwd, name, &options) {
                Ok(p) => p,
                Err(e) => return fail(e),
            }
        }
        "remove" => match args {
            [name] => match install::remove(&cwd, name) {
                Ok(p) => p,
                Err(e) => return fail(e),
            },
            _ => return fail("usage: qlang remove <name>".to_string()),
        },
        _ => match project_dir(&cwd) {
            Some(p) => p,
            None => return fail("no qlang.toml here or in a parent directory".to_string()),
        },
    };
    let refresh = if cmd == "update" {
        if args.is_empty() {
            install::Refresh::All
        } else {
            install::Refresh::Some(args.iter().map(|s| s.to_string()).collect())
        }
    } else {
        install::Refresh::Nothing
    };
    match install::install(&project, &refresh) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(e),
    }
}

/// The directory of the `qlang.toml` that governs `program`, if there is one.
fn project_dir(program: &Path) -> Option<PathBuf> {
    providers::find_config(program).and_then(|f| f.parent().map(Path::to_path_buf))
}

/// The terminal host for the program `file`: its packages and providers come from the
/// `qlang.toml` near it (and `externs`, the `--extern` commands).
fn new_host(
    file: &Path,
    root: Option<PathBuf>,
    seed: Option<u64>,
    script_args: Vec<String>,
    externs: &[String],
    trust: bool,
) -> Result<TerminalHost, String> {
    new_host_with(file, root, seed, script_args, externs, trust, true)
}

/// Like [`new_host`]; without `run`, the providers are not started and every `extern fun` is
/// assumed to exist (the program is only going to be looked at, not run).
fn new_host_with(
    file: &Path,
    root: Option<PathBuf>,
    seed: Option<u64>,
    script_args: Vec<String>,
    externs: &[String],
    trust: bool,
    run: bool,
) -> Result<TerminalHost, String> {
    let packages = match project_dir(file) {
        Some(d) => packages::Packages::load(&d)?,
        None => packages::Packages::default(),
    };
    let providers = if run {
        let specs = providers::specs_for(file, externs)?;
        providers::ensure_trusted(&specs, trust)?;
        providers::Providers::start(&specs, &SQL_EXTERNS)?
    } else {
        providers::Providers::default()
    };
    Ok(TerminalHost {
        root,
        rng: seed.unwrap_or_else(system_entropy),
        db: db::SqlConns::default(),
        redis: Default::default(),
        script_args,
        providers,
        packages,
        assume_externs: !run,
        allow_process: false,
        lines: LineFiles::default(),
    })
}

fn run_file(args: &[&str], execute: bool) -> ExitCode {
    let mut file: Option<&str> = None;
    let mut root: Option<PathBuf> = None;
    let mut seed: Option<u64> = None;
    let mut limits = Limits::default();
    let mut script_args: Vec<String> = Vec::new();
    let mut externs: Vec<String> = Vec::new();
    let mut trust = false;
    let mut allow_process = false;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--" {
            script_args = args[i + 1..].iter().map(|s| s.to_string()).collect();
            break;
        }
        match args[i] {
            "--max-steps" => {
                i += 1;
                match args.get(i).and_then(|s| s.parse().ok()) {
                    Some(n) => limits.max_steps = n,
                    None => {
                        eprintln!("--max-steps needs a number");
                        return ExitCode::from(64);
                    }
                }
            }
            "--seed" => {
                i += 1;
                match args.get(i).and_then(|s| s.parse().ok()) {
                    Some(n) => seed = Some(n),
                    None => {
                        eprintln!("--seed needs a number");
                        return ExitCode::from(64);
                    }
                }
            }
            "--trust-externs" => trust = true,
            "--allow-process" => allow_process = true,
            "--extern" => {
                i += 1;
                match args.get(i) {
                    Some(command) => externs.push(command.to_string()),
                    None => {
                        eprintln!("--extern needs a command");
                        return ExitCode::from(64);
                    }
                }
            }
            "--root" => {
                i += 1;
                match args.get(i).map(|d| Path::new(d).canonicalize()) {
                    Some(Ok(dir)) if dir.is_dir() => root = Some(dir),
                    _ => {
                        eprintln!("--root needs an existing directory");
                        return ExitCode::from(64);
                    }
                }
            }
            a if a.starts_with('-') => {
                eprintln!("unknown option `{a}`");
                return ExitCode::from(64);
            }
            a => file = Some(a),
        }
        i += 1;
    }
    let entry;
    let file = match file {
        Some(f) => f,
        None => match project_entry() {
            Ok(Some(f)) => {
                entry = f;
                entry.as_str()
            }
            Ok(None) => {
                eprintln!("missing file (and no `entry` in a qlang.toml here)\n\n{USAGE}");
                return ExitCode::from(64);
            }
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(1);
            }
        },
    };
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {file}: {e}");
            return ExitCode::from(66);
        }
    };
    if let (Some(root), Ok(full)) = (&root, Path::new(file).canonicalize())
        && !full.starts_with(root)
    {
        eprintln!("{file} is outside of --root {}", root.display());
        return ExitCode::from(64);
    }
    let mut host = match new_host(Path::new(file), root, seed, script_args, &externs, trust) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    // checking only looks at the program: nothing is started, so no permission is needed
    host.allow_process = allow_process || !execute;
    let program = match compile(file, &source, &mut host) {
        Ok(p) => p,
        Err(e) => {
            eprint!("{}", e.render());
            let n = e.diagnostics.len();
            eprintln!("\n{n} error{} found", if n == 1 { "" } else { "s" });
            return ExitCode::from(1);
        }
    };
    for w in &program.warnings {
        eprint!("{}", w.render(&program.sources));
    }
    if !execute {
        eprintln!("ok");
        return ExitCode::SUCCESS;
    }
    let result = program.run(&mut host, limits);
    // an error must not be written on the alternate screen, nor in raw mode
    term::restore();
    let _ = std::io::stdout().flush();
    match result.error {
        None => match result.exit_code {
            Some(code) if code != 0 => ExitCode::from(code as u8),
            _ => ExitCode::SUCCESS,
        },
        Some(d) => {
            eprint!("{}", d.render(&program.sources));
            ExitCode::from(2)
        }
    }
}
