//! SQL connections for `Db.connect`. The URL's scheme picks the driver: `sqlite://path`
//! for SQLite (via `rusqlite`), `postgres://...` / `postgresql://...` for Postgres (via
//! the synchronous `postgres` crate — no async runtime needed, the interpreter is
//! synchronous too). Connections are kept open in a table keyed by a handle qlang only
//! ever sees as the `conn: int` field of its `Db` struct.
//!
//! qlang code always writes `?` placeholders, whichever database it targets: SQLite
//! already wants that syntax, and [`rewrite_placeholders`] turns them into Postgres's
//! `$1, $2...` before the query reaches it. MySQL and MariaDB (`mysql://...`, via the
//! synchronous `mysql` crate) take `?` as they are.

use qlang_core::host::{HostError, SqlRow, SqlValue};
use std::collections::HashMap;

trait SqlConn: Send {
    fn query(&mut self, sql: &str, params: &[SqlValue]) -> Result<Vec<SqlRow>, String>;
    fn execute(&mut self, sql: &str, params: &[SqlValue]) -> Result<u64, String>;
    /// Told by the owner after each statement: a transaction is open (`true`), so the connection
    /// must stay the same one until it ends. Only a pooled connection cares.
    fn set_pinned(&mut self, _pinned: bool) {}
}

// ---------------------------------------------------------------- the pool

/// `--db-pool N`: at most this many real connections to a PostgreSQL or MySQL server, shared by
/// every worker of the process (0: each handle has its own connection, the default).
static POOL_SIZE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub fn set_pool_size(n: usize) {
    POOL_SIZE.store(n, std::sync::atomic::Ordering::SeqCst);
}

/// How long a statement waits for a free connection before it fails.
const POOL_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

struct UrlPool {
    max: usize,
    /// How a real connection is opened (replaced in the tests).
    opener: fn(&str) -> Result<Box<dyn SqlConn>, HostError>,
    wait: std::time::Duration,
    state: std::sync::Mutex<PoolState>,
    freed: std::sync::Condvar,
}

#[derive(Default)]
struct PoolState {
    idle: Vec<Box<dyn SqlConn>>,
    /// Real connections that exist: idle ones and the ones in use.
    total: usize,
}

fn pools() -> &'static std::sync::Mutex<HashMap<String, std::sync::Arc<UrlPool>>> {
    static POOLS: std::sync::OnceLock<std::sync::Mutex<HashMap<String, std::sync::Arc<UrlPool>>>> =
        std::sync::OnceLock::new();
    POOLS.get_or_init(Default::default)
}

fn pool_for(url: &str, max: usize) -> std::sync::Arc<UrlPool> {
    let mut all = pools().lock().unwrap_or_else(|e| e.into_inner());
    all.entry(url.to_string())
        .or_insert_with(|| {
            std::sync::Arc::new(UrlPool {
                max,
                opener: SqlConns::open_physical,
                wait: POOL_WAIT,
                state: Default::default(),
                freed: std::sync::Condvar::new(),
            })
        })
        .clone()
}

impl UrlPool {
    fn acquire(&self, url: &str) -> Result<Box<dyn SqlConn>, String> {
        let deadline = std::time::Instant::now() + self.wait;
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(conn) = state.idle.pop() {
                return Ok(conn);
            }
            if state.total < self.max {
                state.total += 1;
                drop(state);
                return match (self.opener)(url) {
                    Ok(conn) => Ok(conn),
                    Err(e) => {
                        self.forget();
                        Err(format!("{e:?}"))
                    }
                };
            }
            let now = std::time::Instant::now();
            if now >= deadline {
                return Err(format!(
                    "the database pool is exhausted: its {} connections are all busy (--db-pool)",
                    self.max
                ));
            }
            state = self
                .freed
                .wait_timeout(state, deadline - now)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }

    fn release(&self, conn: Box<dyn SqlConn>) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.idle.push(conn);
        self.freed.notify_one();
    }

    /// A real connection is gone (lost, or never opened): its place is free again.
    fn forget(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.total = state.total.saturating_sub(1);
        self.freed.notify_one();
    }
}

/// What a handle is when there is a pool: it takes a real connection for each statement, and keeps
/// the same one for a whole transaction.
struct PooledConn {
    url: String,
    pool: std::sync::Arc<UrlPool>,
    held: Option<Box<dyn SqlConn>>,
    pinned: bool,
}

impl PooledConn {
    fn run<T>(
        &mut self,
        mut f: impl FnMut(&mut dyn SqlConn) -> Result<T, String>,
        starts_transaction: bool,
    ) -> Result<T, String> {
        let mut retried = false;
        loop {
            let mut conn = match self.held.take() {
                Some(c) => c,
                None => self.pool.acquire(&self.url)?,
            };
            let result = f(conn.as_mut());
            match &result {
                Err(why) if connection_lost(why) => {
                    // the server dropped it: a new one replaces it, unless a transaction was open
                    // (what it did is lost, and the owner says so)
                    drop(conn);
                    self.pool.forget();
                    if self.pinned || retried {
                        self.pinned = false;
                        return result;
                    }
                    retried = true;
                    continue;
                }
                _ => {}
            }
            if self.pinned || (starts_transaction && result.is_ok()) {
                self.held = Some(conn);
            } else {
                self.pool.release(conn);
            }
            return result;
        }
    }
}

impl SqlConn for PooledConn {
    fn query(&mut self, sql: &str, params: &[SqlValue]) -> Result<Vec<SqlRow>, String> {
        self.run(|c| c.query(sql, params), false)
    }
    fn execute(&mut self, sql: &str, params: &[SqlValue]) -> Result<u64, String> {
        let word = sql.trim().to_ascii_uppercase();
        let begins =
            word == "BEGIN" || word.starts_with("BEGIN ") || word.starts_with("START TRANSACTION");
        self.run(|c| c.execute(sql, params), begins)
    }
    fn set_pinned(&mut self, pinned: bool) {
        self.pinned = pinned;
        if !pinned && let Some(conn) = self.held.take() {
            self.pool.release(conn);
        }
    }
}

impl Drop for PooledConn {
    fn drop(&mut self) {
        // a handle closed in the middle of a transaction: its connection is not clean
        if let Some(conn) = self.held.take() {
            if self.pinned {
                drop(conn);
                self.pool.forget();
            } else {
                self.pool.release(conn);
            }
        }
    }
}

// ------------------------------------------------------------------- SQLite

struct SqliteConn(rusqlite::Connection);

fn to_sqlite_value(v: &SqlValue) -> rusqlite::types::Value {
    match v {
        SqlValue::Null => rusqlite::types::Value::Null,
        SqlValue::Int(i) => rusqlite::types::Value::Integer(*i),
        SqlValue::Float(f) => rusqlite::types::Value::Real(*f),
        SqlValue::Text(s) => rusqlite::types::Value::Text(s.clone()),
        SqlValue::Bool(b) => rusqlite::types::Value::Integer(i64::from(*b)),
    }
}

fn from_sqlite_value(v: rusqlite::types::ValueRef) -> SqlValue {
    match v {
        rusqlite::types::ValueRef::Null => SqlValue::Null,
        rusqlite::types::ValueRef::Integer(i) => SqlValue::Int(i),
        rusqlite::types::ValueRef::Real(f) => SqlValue::Float(f),
        rusqlite::types::ValueRef::Text(t) => {
            SqlValue::Text(String::from_utf8_lossy(t).into_owned())
        }
        rusqlite::types::ValueRef::Blob(_) => SqlValue::Null,
    }
}

impl SqlConn for SqliteConn {
    fn query(&mut self, sql: &str, params: &[SqlValue]) -> Result<Vec<SqlRow>, String> {
        let mut stmt = self.0.prepare(sql).map_err(|e| e.to_string())?;
        let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let bind = rusqlite::params_from_iter(params.iter().map(to_sqlite_value));
        let rows = stmt
            .query_map(bind, |row| {
                let columns = names
                    .iter()
                    .enumerate()
                    .map(|(i, n)| Ok((n.clone(), from_sqlite_value(row.get_ref(i)?))))
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(SqlRow { columns })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    fn execute(&mut self, sql: &str, params: &[SqlValue]) -> Result<u64, String> {
        let bind = rusqlite::params_from_iter(params.iter().map(to_sqlite_value));
        self.0
            .execute(sql, bind)
            .map(|n| n as u64)
            .map_err(|e| e.to_string())
    }
}

// ------------------------------------------------------------------ Postgres

struct PostgresConn(postgres::Client);

/// qlang code always writes `?` placeholders, same as SQLite; Postgres wants `$1, $2...`
/// instead. Rewrites bare `?` outside of `'...'` string literals (so a literal `?` inside
/// a quoted string is left alone); `''` inside a string toggles the state twice, which
/// correctly leaves it unchanged, matching SQL's own escaping for a quote inside a string.
fn rewrite_placeholders(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut n = 0u32;
    let mut in_string = false;
    for c in sql.chars() {
        match c {
            '\'' => {
                in_string = !in_string;
                out.push(c);
            }
            '?' if !in_string => {
                n += 1;
                out.push('$');
                out.push_str(&n.to_string());
            }
            _ => out.push(c),
        }
    }
    out
}

/// Binds a [`SqlValue`] to whichever Postgres column type the statement actually declares
/// for that placeholder: `Json`'s only number kind is a float, but a value like `5` must
/// still bind into an `INTEGER` column (Postgres never implicitly turns a float8 parameter
/// into an int4 one, so a single fixed Rust type per `SqlValue` variant is not enough).
#[derive(Debug)]
struct PgValue<'a>(&'a SqlValue);

impl postgres::types::ToSql for PgValue<'_> {
    fn to_sql(
        &self,
        ty: &postgres::types::Type,
        out: &mut bytes::BytesMut,
    ) -> Result<postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
        use postgres::types::Type;
        match self.0 {
            SqlValue::Null => Ok(postgres::types::IsNull::Yes),
            SqlValue::Bool(b) => b.to_sql(ty, out),
            SqlValue::Text(s) => s.to_sql(ty, out),
            SqlValue::Int(i) => match *ty {
                Type::INT2 => (*i as i16).to_sql(ty, out),
                Type::INT4 => (*i as i32).to_sql(ty, out),
                Type::FLOAT4 => (*i as f32).to_sql(ty, out),
                Type::FLOAT8 => (*i as f64).to_sql(ty, out),
                Type::TEXT | Type::VARCHAR | Type::BPCHAR => i.to_string().to_sql(ty, out),
                _ => i.to_sql(ty, out),
            },
            SqlValue::Float(f) => match *ty {
                Type::FLOAT4 => (*f as f32).to_sql(ty, out),
                Type::INT2 => (*f as i16).to_sql(ty, out),
                Type::INT4 => (*f as i32).to_sql(ty, out),
                Type::INT8 => (*f as i64).to_sql(ty, out),
                Type::TEXT | Type::VARCHAR | Type::BPCHAR => f.to_string().to_sql(ty, out),
                _ => f.to_sql(ty, out),
            },
        }
    }

    fn accepts(ty: &postgres::types::Type) -> bool {
        use postgres::types::Type;
        matches!(
            *ty,
            Type::BOOL
                | Type::INT2
                | Type::INT4
                | Type::INT8
                | Type::FLOAT4
                | Type::FLOAT8
                | Type::TEXT
                | Type::VARCHAR
                | Type::BPCHAR
        )
    }

    postgres::types::to_sql_checked!();
}

fn to_pg_params(params: &[SqlValue]) -> Vec<PgValue<'_>> {
    params.iter().map(PgValue).collect()
}

/// Common scalar types are read as the matching `SqlValue`; anything else (dates, uuid,
/// json, arrays...) falls back to its text form, or `Null` if that is not possible either.
fn pg_value(row: &postgres::Row, i: usize) -> SqlValue {
    use postgres::types::Type;
    let get = |row: &postgres::Row| {
        row.try_get::<_, Option<String>>(i)
            .ok()
            .flatten()
            .map_or(SqlValue::Null, SqlValue::Text)
    };
    match *row.columns()[i].type_() {
        Type::BOOL => row
            .try_get::<_, Option<bool>>(i)
            .ok()
            .flatten()
            .map_or(SqlValue::Null, SqlValue::Bool),
        Type::INT2 => row
            .try_get::<_, Option<i16>>(i)
            .ok()
            .flatten()
            .map_or(SqlValue::Null, |v| SqlValue::Int(v as i64)),
        Type::INT4 => row
            .try_get::<_, Option<i32>>(i)
            .ok()
            .flatten()
            .map_or(SqlValue::Null, |v| SqlValue::Int(v as i64)),
        Type::INT8 => row
            .try_get::<_, Option<i64>>(i)
            .ok()
            .flatten()
            .map_or(SqlValue::Null, SqlValue::Int),
        Type::FLOAT4 => row
            .try_get::<_, Option<f32>>(i)
            .ok()
            .flatten()
            .map_or(SqlValue::Null, |v| SqlValue::Float(v as f64)),
        Type::FLOAT8 => row
            .try_get::<_, Option<f64>>(i)
            .ok()
            .flatten()
            .map_or(SqlValue::Null, SqlValue::Float),
        _ => get(row),
    }
}

impl SqlConn for PostgresConn {
    fn query(&mut self, sql: &str, params: &[SqlValue]) -> Result<Vec<SqlRow>, String> {
        let bound = to_pg_params(params);
        let refs: Vec<&(dyn postgres::types::ToSql + Sync)> = bound
            .iter()
            .map(|b| b as &(dyn postgres::types::ToSql + Sync))
            .collect();
        let rows = self
            .0
            .query(&rewrite_placeholders(sql), &refs)
            .map_err(|e| e.to_string())?;
        Ok(rows
            .iter()
            .map(|row| SqlRow {
                columns: row
                    .columns()
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (c.name().to_string(), pg_value(row, i)))
                    .collect(),
            })
            .collect())
    }

    fn execute(&mut self, sql: &str, params: &[SqlValue]) -> Result<u64, String> {
        let bound = to_pg_params(params);
        let refs: Vec<&(dyn postgres::types::ToSql + Sync)> = bound
            .iter()
            .map(|b| b as &(dyn postgres::types::ToSql + Sync))
            .collect();
        self.0
            .execute(&rewrite_placeholders(sql), &refs)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{rewrite_placeholders, wants_tls};

    #[test]
    fn wants_tls_only_when_sslmode_asks_for_it() {
        assert!(!wants_tls("postgres://localhost/db"));
        assert!(!wants_tls("postgres://localhost/db?sslmode=disable"));
        assert!(wants_tls("postgres://localhost/db?sslmode=require"));
        assert!(wants_tls(
            "postgres://localhost/db?user=x&sslmode=verify-full"
        ));
    }

    #[test]
    fn a_remote_server_needs_an_explicit_sslmode() {
        use super::{has_sslmode, is_local};
        assert!(is_local("postgres://localhost/db"));
        assert!(is_local("postgres://u:p@127.0.0.1:5432/db"));
        assert!(is_local("postgres://u:p@[::1]/db"));
        assert!(is_local("postgres:///db"));
        assert!(!is_local("postgres://db.example.com/db"));
        assert!(!is_local("postgres://u:p@10.0.0.5/db?x=localhost"));
        assert!(!has_sslmode("postgres://h/db"));
        assert!(has_sslmode("postgres://h/db?user=x&sslmode=disable"));
    }

    #[test]
    fn rewrites_bare_question_marks_in_order() {
        assert_eq!(
            rewrite_placeholders("SELECT * FROM t WHERE a = ? AND b = ?"),
            "SELECT * FROM t WHERE a = $1 AND b = $2"
        );
    }

    #[test]
    fn leaves_a_question_mark_inside_a_string_literal_alone() {
        assert_eq!(
            rewrite_placeholders("SELECT ? , 'what?' FROM t"),
            "SELECT $1 , 'what?' FROM t"
        );
    }

    #[test]
    fn handles_an_escaped_quote_inside_a_string() {
        assert_eq!(
            rewrite_placeholders("SELECT 'it''s a ?' , ?"),
            "SELECT 'it''s a ?' , $1"
        );
    }
}

/// Opens a Postgres connection, in TLS if the URL asks for it
/// (`?sslmode=require`/`prefer`/`verify-ca`/`verify-full`; absent or `disable` is a plain
/// connection, as before). The certificate is always verified against Mozilla's root
/// store (`webpki-roots`, bundled so this works the same in every build): there is no
/// "encrypt but don't check who to" mode here, unlike libpq's own `require`.
fn wants_tls(url: &str) -> bool {
    url.split(['?', '&']).skip(1).any(|p| {
        matches!(
            p,
            "sslmode=require" | "sslmode=prefer" | "sslmode=verify-ca" | "sslmode=verify-full"
        )
    })
}

/// Whether the URL says anything about `sslmode`.
fn has_sslmode(url: &str) -> bool {
    url.split(['?', '&'])
        .skip(1)
        .any(|p| p.starts_with("sslmode="))
}

/// Whether the server is on this machine (a name, an address or a socket directory).
fn is_local(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?']).next().unwrap_or("");
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = host.split(':').next().unwrap_or("");
    let host = host.trim_matches(['[', ']']);
    // no host in the URL: a local socket
    host.is_empty()
        || host == "localhost"
        || host == "127.0.0.1"
        || host == "::1"
        || host.starts_with('%')
        || host.starts_with('/')
}

fn pg_connect(url: &str) -> Result<postgres::Client, HostError> {
    // a password sent in the clear to another machine must be a choice, not an accident
    if !has_sslmode(url) && !is_local(url) {
        return Err(HostError::Other(
            "this Postgres server is not on this machine: add ?sslmode=require to use TLS, or ?sslmode=disable to confirm that you want an unencrypted connection".to_string(),
        ));
    }
    if !wants_tls(url) {
        return postgres::Client::connect(url, postgres::NoTls)
            .map_err(|e| HostError::Other(e.to_string()));
    }
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let tls = tokio_postgres_rustls::MakeRustlsConnect::new(config);
    postgres::Client::connect(url, tls).map_err(|e| HostError::Other(e.to_string()))
}

// --------------------------------------------------------------------- MySQL

struct MysqlConn(mysql::Conn);

fn to_mysql_value(v: &SqlValue) -> mysql::Value {
    match v {
        SqlValue::Null => mysql::Value::NULL,
        SqlValue::Int(i) => mysql::Value::Int(*i),
        SqlValue::Float(f) => mysql::Value::Double(*f),
        SqlValue::Text(s) => mysql::Value::Bytes(s.clone().into_bytes()),
        SqlValue::Bool(b) => mysql::Value::Int(i64::from(*b)),
    }
}

fn from_mysql_value(v: mysql::Value) -> SqlValue {
    use mysql::Value;
    match v {
        Value::NULL => SqlValue::Null,
        // text, or bytes that are not text (a BLOB, which has no `Json` form: null, like SQLite)
        Value::Bytes(b) => String::from_utf8(b).map_or(SqlValue::Null, SqlValue::Text),
        Value::Int(i) => SqlValue::Int(i),
        Value::UInt(u) => i64::try_from(u).map_or(SqlValue::Float(u as f64), SqlValue::Int),
        Value::Float(f) => SqlValue::Float(f64::from(f)),
        Value::Double(f) => SqlValue::Float(f),
        Value::Date(y, mo, d, h, mi, s, us) => {
            let day = format!("{y:04}-{mo:02}-{d:02}");
            if (h, mi, s, us) == (0, 0, 0, 0) {
                SqlValue::Text(day)
            } else if us == 0 {
                SqlValue::Text(format!("{day} {h:02}:{mi:02}:{s:02}"))
            } else {
                SqlValue::Text(format!("{day} {h:02}:{mi:02}:{s:02}.{us:06}"))
            }
        }
        Value::Time(neg, days, h, mi, s, us) => {
            let hours = u32::from(h) + days * 24;
            let sign = if neg { "-" } else { "" };
            if us == 0 {
                SqlValue::Text(format!("{sign}{hours:02}:{mi:02}:{s:02}"))
            } else {
                SqlValue::Text(format!("{sign}{hours:02}:{mi:02}:{s:02}.{us:06}"))
            }
        }
    }
}

impl SqlConn for MysqlConn {
    fn query(&mut self, sql: &str, params: &[SqlValue]) -> Result<Vec<SqlRow>, String> {
        use mysql::prelude::Queryable;
        // prepared statements bring the columns back with their types (the text protocol
        // gives everything as text); the few statements that cannot be prepared (error 1295)
        // fall back to it
        let bound: Vec<mysql::Value> = params.iter().map(to_mysql_value).collect();
        let first: Result<Vec<mysql::Row>, mysql::Error> = if bound.is_empty() {
            self.0.exec(sql, mysql::Params::Empty)
        } else {
            self.0.exec(sql, mysql::Params::Positional(bound))
        };
        let rows = match first {
            Ok(rows) => rows,
            Err(mysql::Error::MySqlError(ref e)) if e.code == 1295 && params.is_empty() => {
                self.0.query(sql).map_err(|e| e.to_string())?
            }
            Err(e) => return Err(e.to_string()),
        };
        Ok(rows
            .into_iter()
            .map(|row| {
                let names: Vec<String> = row
                    .columns_ref()
                    .iter()
                    .map(|c| c.name_str().into_owned())
                    .collect();
                let columns = names
                    .into_iter()
                    .zip(row.unwrap())
                    .map(|(n, v)| (n, from_mysql_value(v)))
                    .collect();
                SqlRow { columns }
            })
            .collect())
    }

    fn execute(&mut self, sql: &str, params: &[SqlValue]) -> Result<u64, String> {
        use mysql::prelude::Queryable;
        if params.is_empty() {
            self.0.query_drop(sql)
        } else {
            let bound: Vec<mysql::Value> = params.iter().map(to_mysql_value).collect();
            self.0.exec_drop(sql, mysql::Params::Positional(bound))
        }
        .map_err(|e| e.to_string())?;
        Ok(self.0.affected_rows())
    }
}

/// Opens a MySQL or MariaDB connection from `mysql://user:password@host:3306/database`. As for
/// Postgres, a server on another machine needs a `?sslmode=` in the URL (`require` asks for TLS,
/// checked against the public authorities; `disable` confirms a connection in the clear).
fn mysql_connect(url: &str) -> Result<mysql::Conn, HostError> {
    if !has_sslmode(url) && !is_local(url) {
        return Err(HostError::Other(
            "this MySQL server is not on this machine: add ?sslmode=require to use TLS, or ?sslmode=disable to confirm that you want an unencrypted connection".to_string(),
        ));
    }
    let tls = wants_tls(url);
    // the driver does not know `sslmode`: take it out of the URL
    let (base, query) = url.split_once('?').unwrap_or((url, ""));
    let kept: Vec<&str> = query
        .split('&')
        .filter(|p| !p.is_empty() && !p.starts_with("sslmode="))
        .collect();
    let clean = if kept.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{}", kept.join("&"))
    };
    let opts = mysql::Opts::from_url(&clean).map_err(|e| HostError::Other(e.to_string()))?;
    let mut builder = mysql::OptsBuilder::from_opts(opts);
    if tls {
        builder = builder.ssl_opts(Some(mysql::SslOpts::default()));
    }
    mysql::Conn::new(builder).map_err(|e| HostError::Other(e.to_string()))
}

#[cfg(test)]
mod mysql_tests {
    use super::*;

    #[test]
    fn values_come_back_as_json_friendly_columns() {
        assert!(matches!(
            from_mysql_value(mysql::Value::NULL),
            SqlValue::Null
        ));
        assert!(matches!(
            from_mysql_value(mysql::Value::Int(-3)),
            SqlValue::Int(-3)
        ));
        assert!(matches!(
            from_mysql_value(mysql::Value::UInt(7)),
            SqlValue::Int(7)
        ));
        assert!(matches!(
            from_mysql_value(mysql::Value::UInt(u64::MAX)),
            SqlValue::Float(_)
        ));
        match from_mysql_value(mysql::Value::Date(2026, 10, 23, 14, 5, 9, 0)) {
            SqlValue::Text(t) => assert_eq!(t, "2026-10-23 14:05:09"),
            _ => panic!("a date is text"),
        }
        match from_mysql_value(mysql::Value::Date(2026, 10, 23, 0, 0, 0, 0)) {
            SqlValue::Text(t) => assert_eq!(t, "2026-10-23"),
            _ => panic!("a date is text"),
        }
        match from_mysql_value(mysql::Value::Time(true, 1, 2, 3, 4, 0)) {
            SqlValue::Text(t) => assert_eq!(t, "-26:03:04"),
            _ => panic!("a time is text"),
        }
        assert!(matches!(
            from_mysql_value(mysql::Value::Bytes(vec![0xff, 0xfe])),
            SqlValue::Null
        ));
    }

    #[test]
    fn a_remote_server_needs_an_explicit_choice() {
        let e = mysql_connect("mysql://u:p@db.example.com/app")
            .err()
            .unwrap();
        assert!(format!("{e:?}").contains("sslmode"));
    }
}

// --------------------------------------------------------------------- query log

/// One statement run while a request was handled (`--dev`).
#[derive(Clone, Debug)]
pub struct LoggedQuery {
    pub sql: String,
    pub ms: f64,
}

thread_local! {
    static QUERY_LOG: std::cell::RefCell<Option<Vec<LoggedQuery>>> =
        const { std::cell::RefCell::new(None) };
}

/// The most statements kept for one request.
const MAX_LOGGED: usize = 500;

/// Starts recording the statements of this thread (one worker handles one request at a time).
pub fn start_query_log() {
    QUERY_LOG.with(|l| *l.borrow_mut() = Some(Vec::new()));
}

/// What was recorded since [`start_query_log`], and stops recording.
pub fn take_query_log() -> Vec<LoggedQuery> {
    QUERY_LOG.with(|l| l.borrow_mut().take().unwrap_or_default())
}

fn log_query(sql: &str, started: std::time::Instant) {
    QUERY_LOG.with(|l| {
        if let Some(log) = l.borrow_mut().as_mut()
            && log.len() < MAX_LOGGED
        {
            log.push(LoggedQuery {
                sql: sql.split_whitespace().collect::<Vec<_>>().join(" "),
                ms: started.elapsed().as_secs_f64() * 1000.0,
            });
        }
    });
}

/// The statements that ran many times with only their values changing: the sign of an N+1.
/// Each one is the statement, how many times it ran and the time they took together.
pub fn repeated(queries: &[LoggedQuery]) -> Vec<(String, usize, f64)> {
    let mut groups: Vec<(String, usize, f64)> = Vec::new();
    for q in queries {
        match groups.iter_mut().find(|g| g.0 == q.sql) {
            Some(g) => {
                g.1 += 1;
                g.2 += q.ms;
            }
            None => groups.push((q.sql.clone(), 1, q.ms)),
        }
    }
    groups.retain(|g| g.1 >= 3);
    groups.sort_by_key(|a| std::cmp::Reverse(a.1));
    groups
}

/// Whether an error says the connection itself is gone (not that the statement is wrong).
fn connection_lost(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    [
        "connection closed",
        "closed connection",
        "connection is closed",
        "broken pipe",
        "connection reset",
        "server has gone away",
        "lost connection",
        "terminating connection",
        "unexpected eof",
        "unexpected end of file",
        "connection timed out",
    ]
    .iter()
    .any(|s| m.contains(s))
}

// --------------------------------------------------------------------- table

/// Open connections, keyed by the handle given to qlang as `Db.conn`.
#[derive(Default)]
pub struct SqlConns {
    next_id: u64,
    open: HashMap<u64, Box<dyn SqlConn>>,
    /// Where each connection came from, to open it again when the server drops it.
    urls: HashMap<u64, String>,
    /// How many `begin()` are open on each connection: the first is a real transaction, the next
    /// ones are savepoints (so `begin` inside `begin` works, and an inner `rollback` undoes only
    /// what the inner `begin` started). A connection lost inside one cannot be replaced silently.
    depth: HashMap<u64, u32>,
}

impl SqlConns {
    /// A handle for `url`: a real connection, or, with `--db-pool` and a server database, a
    /// handle that borrows one from the shared pool when it needs it.
    fn open_url(url: &str) -> Result<Box<dyn SqlConn>, HostError> {
        let max = POOL_SIZE.load(std::sync::atomic::Ordering::SeqCst);
        let server = url.starts_with("postgres://")
            || url.starts_with("postgresql://")
            || url.starts_with("mysql://")
            || url.starts_with("mariadb://");
        if max > 0 && server {
            // one real connection now, so that a wrong URL or password fails at `Db.connect`
            let pool = pool_for(url, max);
            let first = pool.acquire(url).map_err(HostError::Other)?;
            pool.release(first);
            return Ok(Box::new(PooledConn {
                url: url.to_string(),
                pool,
                held: None,
                pinned: false,
            }));
        }
        Self::open_physical(url)
    }

    fn open_physical(url: &str) -> Result<Box<dyn SqlConn>, HostError> {
        let conn: Box<dyn SqlConn> = if let Some(path) = url.strip_prefix("sqlite://") {
            Box::new(SqliteConn(
                rusqlite::Connection::open(path).map_err(|e| HostError::Other(e.to_string()))?,
            ))
        } else if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            Box::new(PostgresConn(pg_connect(url)?))
        } else if url.starts_with("mysql://") || url.starts_with("mariadb://") {
            let url = url.replacen("mariadb://", "mysql://", 1);
            Box::new(MysqlConn(mysql_connect(&url)?))
        } else {
            return Err(HostError::Other(format!(
                "unknown database URL \"{url}\" (use sqlite://..., postgres://... or mysql://...)"
            )));
        };
        Ok(conn)
    }

    pub fn connect(&mut self, url: &str) -> Result<u64, HostError> {
        let conn = Self::open_url(url)?;
        let id = self.next_id;
        self.next_id += 1;
        self.open.insert(id, conn);
        self.urls.insert(id, url.to_string());
        Ok(id)
    }

    /// Opens `conn` again from its URL, after the server dropped it (a restart, an idle
    /// timeout, a proxy). Never inside a transaction: what was done there is lost, and saying so
    /// is better than running the rest of it outside.
    fn reconnect(&mut self, conn: u64, why: &str) -> Result<(), HostError> {
        if self.depth.remove(&conn).is_some_and(|d| d > 0) {
            return Err(HostError::Other(format!(
                "the database connection was lost inside a transaction: {why}"
            )));
        }
        let Some(url) = self.urls.get(&conn).cloned() else {
            return Err(HostError::Other(why.to_string()));
        };
        let fresh = Self::open_url(&url).map_err(|e| {
            HostError::Other(format!(
                "{why} (and the database cannot be reached again: {e:?})"
            ))
        })?;
        eprintln!("database connection lost ({why}): reconnected");
        self.open.insert(conn, fresh);
        Ok(())
    }

    /// What `BEGIN`, `COMMIT` and `ROLLBACK` become on this connection: the first level is the
    /// real thing, the levels inside it are savepoints. Any other statement is left as it is.
    fn transaction_statement(&self, conn: u64, sql: &str) -> (String, i32) {
        let word = sql.trim().trim_end_matches(';').trim().to_ascii_uppercase();
        let depth = self.depth.get(&conn).copied().unwrap_or(0);
        match word.as_str() {
            "BEGIN" | "BEGIN TRANSACTION" | "START TRANSACTION" => {
                if depth == 0 {
                    (sql.to_string(), 1)
                } else {
                    (format!("SAVEPOINT qlang_sp{depth}"), 1)
                }
            }
            "COMMIT" | "END" | "COMMIT TRANSACTION" if depth > 1 => {
                (format!("RELEASE SAVEPOINT qlang_sp{}", depth - 1), -1)
            }
            "COMMIT" | "END" | "COMMIT TRANSACTION" if depth == 1 => (sql.to_string(), -1),
            "ROLLBACK" | "ROLLBACK TRANSACTION" if depth > 1 => (
                format!(
                    "ROLLBACK TO SAVEPOINT qlang_sp{0}; RELEASE SAVEPOINT qlang_sp{0}",
                    depth - 1
                ),
                -1,
            ),
            "ROLLBACK" | "ROLLBACK TRANSACTION" if depth == 1 => (sql.to_string(), -1),
            _ => (sql.to_string(), 0),
        }
    }

    fn get(&mut self, conn: u64) -> Result<&mut Box<dyn SqlConn>, HostError> {
        self.open
            .get_mut(&conn)
            .ok_or_else(|| HostError::Other("this database connection is closed".to_string()))
    }

    pub fn query(
        &mut self,
        conn: u64,
        sql: &str,
        params: Vec<SqlValue>,
    ) -> Result<Vec<SqlRow>, HostError> {
        let started = std::time::Instant::now();
        let mut result = self.get(conn)?.query(sql, &params);
        if let Err(why) = &result
            && connection_lost(why)
            && self.reconnect(conn, why).is_ok()
        {
            result = self.get(conn)?.query(sql, &params);
        }
        log_query(sql, started);
        result.map_err(HostError::Other)
    }

    /// Runs one statement, or several separated by `;` (a savepoint rollback and its release).
    fn run(&mut self, conn: u64, statement: &str, params: &[SqlValue]) -> Result<u64, String> {
        let handle = self
            .open
            .get_mut(&conn)
            .ok_or_else(|| "this database connection is closed".to_string())?;
        if statement.starts_with("ROLLBACK TO SAVEPOINT") {
            let mut total = 0;
            for part in statement.split("; ") {
                total += handle.execute(part, &[])?;
            }
            return Ok(total);
        }
        handle.execute(statement, params)
    }

    pub fn execute(
        &mut self,
        conn: u64,
        sql: &str,
        params: Vec<SqlValue>,
    ) -> Result<u64, HostError> {
        let started = std::time::Instant::now();
        let (statement, change) = self.transaction_statement(conn, sql);
        let mut result = self.run(conn, &statement, &params);
        if let Err(why) = &result
            && connection_lost(why)
        {
            // inside a transaction the connection is not replaced: the error says so
            match self.reconnect(conn, why) {
                Ok(()) => result = self.run(conn, &statement, &params),
                Err(e) => {
                    log_query(sql, started);
                    return Err(e);
                }
            }
        }
        if result.is_ok() && change != 0 {
            let depth = self.depth.entry(conn).or_insert(0);
            *depth = (*depth as i32 + change).max(0) as u32;
            let open = *depth > 0;
            if let Some(handle) = self.open.get_mut(&conn) {
                handle.set_pinned(open);
            }
        }
        log_query(sql, started);
        result.map_err(HostError::Other)
    }

    pub fn close(&mut self, conn: u64) -> Result<(), HostError> {
        self.get(conn)?;
        self.open.remove(&conn);
        self.urls.remove(&conn);
        self.depth.remove(&conn);
        Ok(())
    }
}

#[cfg(test)]
mod reconnect_tests {
    use super::*;

    /// A connection the server dropped: every statement says so.
    struct Dropped;

    impl SqlConn for Dropped {
        fn query(&mut self, _: &str, _: &[SqlValue]) -> Result<Vec<SqlRow>, String> {
            Err("connection closed".to_string())
        }
        fn execute(&mut self, _: &str, _: &[SqlValue]) -> Result<u64, String> {
            Err("server has gone away".to_string())
        }
    }

    #[test]
    fn a_dropped_connection_is_opened_again_and_the_statement_runs() {
        let mut conns = SqlConns::default();
        let id = conns.connect("sqlite://:memory:").unwrap();
        conns.open.insert(id, Box::new(Dropped));
        let rows = conns.query(id, "SELECT 1 AS one", vec![]).unwrap();
        assert_eq!(rows.len(), 1);
        // and it keeps working
        assert!(conns.execute(id, "CREATE TABLE t (a INT)", vec![]).is_ok());
        conns.open.insert(id, Box::new(Dropped));
        assert!(conns.execute(id, "CREATE TABLE u (a INT)", vec![]).is_ok());
    }

    #[test]
    fn a_connection_lost_inside_a_transaction_is_an_error_not_a_silent_retry() {
        let mut conns = SqlConns::default();
        let id = conns.connect("sqlite://:memory:").unwrap();
        conns.execute(id, "BEGIN", vec![]).unwrap();
        conns.open.insert(id, Box::new(Dropped));
        let e = conns
            .execute(id, "CREATE TABLE t (a INT)", vec![])
            .unwrap_err();
        assert!(format!("{e:?}").contains("inside a transaction"), "{e:?}");
        // the next statement starts afresh on a new connection
        assert!(conns.execute(id, "CREATE TABLE t (a INT)", vec![]).is_ok());
    }

    #[test]
    fn an_error_of_the_statement_is_not_taken_for_a_lost_connection() {
        let mut conns = SqlConns::default();
        let id = conns.connect("sqlite://:memory:").unwrap();
        let e = conns
            .query(id, "SELECT * FROM missing_table", vec![])
            .unwrap_err();
        assert!(format!("{e:?}").contains("no such table"), "{e:?}");
        assert!(!connection_lost("syntax error at or near FROM"));
        assert!(connection_lost(
            "db error: terminating connection due to administrator command"
        ));
    }

    #[test]
    fn statements_are_recorded_with_their_time_and_repeats_are_found() {
        let mut conns = SqlConns::default();
        let id = conns.connect("sqlite://:memory:").unwrap();
        conns.execute(id, "CREATE TABLE t (a INT)", vec![]).unwrap();
        start_query_log();
        for _ in 0..4 {
            conns
                .query(id, "SELECT  a\n FROM t WHERE a = ?", vec![SqlValue::Int(1)])
                .unwrap();
        }
        conns.query(id, "SELECT 1", vec![]).unwrap();
        let log = take_query_log();
        assert_eq!(log.len(), 5);
        assert_eq!(log[0].sql, "SELECT a FROM t WHERE a = ?");
        let again = repeated(&log);
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].1, 4);
        // nothing is recorded once it was taken
        conns.query(id, "SELECT 1", vec![]).unwrap();
        assert!(take_query_log().is_empty());
    }

    fn count(conns: &mut SqlConns, id: u64) -> usize {
        conns.query(id, "SELECT a FROM t", vec![]).unwrap().len()
    }

    fn with_table() -> (SqlConns, u64) {
        let mut conns = SqlConns::default();
        let id = conns.connect("sqlite://:memory:").unwrap();
        conns.execute(id, "CREATE TABLE t (a INT)", vec![]).unwrap();
        (conns, id)
    }

    #[test]
    fn begin_inside_begin_is_a_savepoint_and_commit_keeps_everything() {
        let (mut c, id) = with_table();
        c.execute(id, "BEGIN", vec![]).unwrap();
        c.execute(id, "INSERT INTO t VALUES (1)", vec![]).unwrap();
        c.execute(id, "BEGIN", vec![]).unwrap();
        c.execute(id, "INSERT INTO t VALUES (2)", vec![]).unwrap();
        c.execute(id, "COMMIT", vec![]).unwrap();
        c.execute(id, "COMMIT", vec![]).unwrap();
        assert_eq!(count(&mut c, id), 2);
        assert_eq!(c.depth.get(&id).copied().unwrap_or(0), 0);
    }

    #[test]
    fn an_inner_rollback_undoes_only_the_inner_part() {
        let (mut c, id) = with_table();
        c.execute(id, "BEGIN", vec![]).unwrap();
        c.execute(id, "INSERT INTO t VALUES (1)", vec![]).unwrap();
        c.execute(id, "BEGIN", vec![]).unwrap();
        c.execute(id, "INSERT INTO t VALUES (2)", vec![]).unwrap();
        c.execute(id, "ROLLBACK", vec![]).unwrap();
        assert_eq!(count(&mut c, id), 1);
        c.execute(id, "COMMIT", vec![]).unwrap();
        assert_eq!(count(&mut c, id), 1);
    }

    #[test]
    fn an_outer_rollback_undoes_the_committed_inner_part_too() {
        let (mut c, id) = with_table();
        c.execute(id, "begin;", vec![]).unwrap();
        c.execute(id, "BEGIN", vec![]).unwrap();
        c.execute(id, "BEGIN", vec![]).unwrap();
        c.execute(id, "INSERT INTO t VALUES (3)", vec![]).unwrap();
        c.execute(id, "COMMIT", vec![]).unwrap();
        c.execute(id, "COMMIT", vec![]).unwrap();
        assert_eq!(count(&mut c, id), 1);
        c.execute(id, "ROLLBACK", vec![]).unwrap();
        assert_eq!(count(&mut c, id), 0);
    }

    #[test]
    fn a_commit_without_begin_is_the_databases_error() {
        let (mut c, id) = with_table();
        assert!(c.execute(id, "COMMIT", vec![]).is_err());
        assert_eq!(c.depth.get(&id).copied().unwrap_or(0), 0);
    }
}

#[cfg(test)]
mod pool_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static OPENED: AtomicUsize = AtomicUsize::new(0);

    /// A real connection that says which one it is.
    struct Numbered(usize);

    impl SqlConn for Numbered {
        fn query(&mut self, _: &str, _: &[SqlValue]) -> Result<Vec<SqlRow>, String> {
            Ok(vec![SqlRow {
                columns: vec![("n".to_string(), SqlValue::Int(self.0 as i64))],
            }])
        }
        fn execute(&mut self, _: &str, _: &[SqlValue]) -> Result<u64, String> {
            Ok(0)
        }
    }

    fn open_numbered(_: &str) -> Result<Box<dyn SqlConn>, HostError> {
        Ok(Box::new(Numbered(OPENED.fetch_add(1, Ordering::SeqCst))))
    }

    fn handle(pool: &std::sync::Arc<UrlPool>) -> PooledConn {
        PooledConn {
            url: "postgres://test".to_string(),
            pool: pool.clone(),
            held: None,
            pinned: false,
        }
    }

    fn pool(max: usize, wait_ms: u64) -> std::sync::Arc<UrlPool> {
        std::sync::Arc::new(UrlPool {
            max,
            opener: open_numbered,
            wait: std::time::Duration::from_millis(wait_ms),
            state: Default::default(),
            freed: std::sync::Condvar::new(),
        })
    }

    fn which(c: &mut PooledConn) -> i64 {
        match &c.query("SELECT", &[]).unwrap()[0].columns[0].1 {
            SqlValue::Int(n) => *n,
            _ => -1,
        }
    }

    #[test]
    fn many_handles_share_a_few_real_connections() {
        let p = pool(2, 50);
        let mut handles: Vec<PooledConn> = (0..6).map(|_| handle(&p)).collect();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..3 {
            for h in handles.iter_mut() {
                seen.insert(which(h));
            }
        }
        assert_eq!(
            seen.len(),
            1,
            "one after the other they reuse the same one: {seen:?}"
        );
        assert_eq!(p.state.lock().unwrap().total, 1);
    }

    #[test]
    fn a_transaction_keeps_its_connection_and_others_use_another() {
        let p = pool(2, 50);
        let (mut a, mut b) = (handle(&p), handle(&p));
        a.execute("BEGIN", &[]).unwrap();
        a.set_pinned(true);
        let mine = which(&mut a);
        assert_eq!(which(&mut a), mine);
        assert_ne!(which(&mut b), mine);
        a.execute("COMMIT", &[]).unwrap();
        a.set_pinned(false);
        // released: now b may get either, and the pool never made a third
        which(&mut a);
        assert_eq!(p.state.lock().unwrap().total, 2);
    }

    #[test]
    fn an_exhausted_pool_fails_instead_of_waiting_forever() {
        let p = pool(1, 30);
        let (mut a, mut b) = (handle(&p), handle(&p));
        a.execute("BEGIN", &[]).unwrap();
        a.set_pinned(true);
        let err = b.query("SELECT", &[]).unwrap_err();
        assert!(err.contains("pool is exhausted"), "{err}");
        // and once the transaction ends, b can go
        a.set_pinned(false);
        assert!(b.query("SELECT", &[]).is_ok());
    }

    #[test]
    fn a_handle_closed_inside_a_transaction_frees_its_place() {
        let p = pool(1, 30);
        let mut a = handle(&p);
        a.execute("BEGIN", &[]).unwrap();
        a.set_pinned(true);
        drop(a);
        assert_eq!(p.state.lock().unwrap().total, 0);
        assert!(handle(&p).query("SELECT", &[]).is_ok());
    }
}
