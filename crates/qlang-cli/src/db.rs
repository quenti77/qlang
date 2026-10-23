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

trait SqlConn {
    fn query(&mut self, sql: &str, params: &[SqlValue]) -> Result<Vec<SqlRow>, String>;
    fn execute(&mut self, sql: &str, params: &[SqlValue]) -> Result<u64, String>;
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

// --------------------------------------------------------------------- table

/// Open connections, keyed by the handle given to qlang as `Db.conn`.
#[derive(Default)]
pub struct SqlConns {
    next_id: u64,
    open: HashMap<u64, Box<dyn SqlConn>>,
}

impl SqlConns {
    pub fn connect(&mut self, url: &str) -> Result<u64, HostError> {
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
        let id = self.next_id;
        self.next_id += 1;
        self.open.insert(id, conn);
        Ok(id)
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
        self.get(conn)?
            .query(sql, &params)
            .map_err(HostError::Other)
    }

    pub fn execute(
        &mut self,
        conn: u64,
        sql: &str,
        params: Vec<SqlValue>,
    ) -> Result<u64, HostError> {
        self.get(conn)?
            .execute(sql, &params)
            .map_err(HostError::Other)
    }

    pub fn close(&mut self, conn: u64) -> Result<(), HostError> {
        self.get(conn)?;
        self.open.remove(&conn);
        Ok(())
    }
}
