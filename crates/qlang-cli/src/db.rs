//! SQL connections for `Db.connect`. The URL's scheme picks the driver: `sqlite://path`
//! for SQLite (via `rusqlite`), `postgres://...` / `postgresql://...` for Postgres (via
//! the synchronous `postgres` crate — no async runtime needed, the interpreter is
//! synchronous too). Connections are kept open in a table keyed by a handle qlang only
//! ever sees as the `conn: int` field of its `Db` struct.
//!
//! qlang code always writes `?` placeholders, whichever database it targets: SQLite
//! already wants that syntax, and [`rewrite_placeholders`] turns them into Postgres's
//! `$1, $2...` before the query reaches it.

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
        rusqlite::types::ValueRef::Text(t) => SqlValue::Text(String::from_utf8_lossy(t).into_owned()),
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
                let columns = names.iter().enumerate().map(|(i, n)| Ok((n.clone(), from_sqlite_value(row.get_ref(i)?)))).collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(SqlRow { columns })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    fn execute(&mut self, sql: &str, params: &[SqlValue]) -> Result<u64, String> {
        let bind = rusqlite::params_from_iter(params.iter().map(to_sqlite_value));
        self.0.execute(sql, bind).map(|n| n as u64).map_err(|e| e.to_string())
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
    fn to_sql(&self, ty: &postgres::types::Type, out: &mut bytes::BytesMut) -> Result<postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
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
        matches!(*ty, Type::BOOL | Type::INT2 | Type::INT4 | Type::INT8 | Type::FLOAT4 | Type::FLOAT8 | Type::TEXT | Type::VARCHAR | Type::BPCHAR)
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
    let get = |row: &postgres::Row| row.try_get::<_, Option<String>>(i).ok().flatten().map_or(SqlValue::Null, SqlValue::Text);
    match *row.columns()[i].type_() {
        Type::BOOL => row.try_get::<_, Option<bool>>(i).ok().flatten().map_or(SqlValue::Null, SqlValue::Bool),
        Type::INT2 => row.try_get::<_, Option<i16>>(i).ok().flatten().map_or(SqlValue::Null, |v| SqlValue::Int(v as i64)),
        Type::INT4 => row.try_get::<_, Option<i32>>(i).ok().flatten().map_or(SqlValue::Null, |v| SqlValue::Int(v as i64)),
        Type::INT8 => row.try_get::<_, Option<i64>>(i).ok().flatten().map_or(SqlValue::Null, SqlValue::Int),
        Type::FLOAT4 => row.try_get::<_, Option<f32>>(i).ok().flatten().map_or(SqlValue::Null, |v| SqlValue::Float(v as f64)),
        Type::FLOAT8 => row.try_get::<_, Option<f64>>(i).ok().flatten().map_or(SqlValue::Null, SqlValue::Float),
        _ => get(row),
    }
}

impl SqlConn for PostgresConn {
    fn query(&mut self, sql: &str, params: &[SqlValue]) -> Result<Vec<SqlRow>, String> {
        let bound = to_pg_params(params);
        let refs: Vec<&(dyn postgres::types::ToSql + Sync)> = bound.iter().map(|b| b as &(dyn postgres::types::ToSql + Sync)).collect();
        let rows = self.0.query(&rewrite_placeholders(sql), &refs).map_err(|e| e.to_string())?;
        Ok(rows
            .iter()
            .map(|row| SqlRow { columns: row.columns().iter().enumerate().map(|(i, c)| (c.name().to_string(), pg_value(row, i))).collect() })
            .collect())
    }

    fn execute(&mut self, sql: &str, params: &[SqlValue]) -> Result<u64, String> {
        let bound = to_pg_params(params);
        let refs: Vec<&(dyn postgres::types::ToSql + Sync)> = bound.iter().map(|b| b as &(dyn postgres::types::ToSql + Sync)).collect();
        self.0.execute(&rewrite_placeholders(sql), &refs).map_err(|e| e.to_string())
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
        assert!(wants_tls("postgres://localhost/db?user=x&sslmode=verify-full"));
    }

    #[test]
    fn rewrites_bare_question_marks_in_order() {
        assert_eq!(rewrite_placeholders("SELECT * FROM t WHERE a = ? AND b = ?"), "SELECT * FROM t WHERE a = $1 AND b = $2");
    }

    #[test]
    fn leaves_a_question_mark_inside_a_string_literal_alone() {
        assert_eq!(rewrite_placeholders("SELECT ? , 'what?' FROM t"), "SELECT $1 , 'what?' FROM t");
    }

    #[test]
    fn handles_an_escaped_quote_inside_a_string() {
        assert_eq!(rewrite_placeholders("SELECT 'it''s a ?' , ?"), "SELECT 'it''s a ?' , $1");
    }
}

/// Opens a Postgres connection, in TLS if the URL asks for it
/// (`?sslmode=require`/`prefer`/`verify-ca`/`verify-full`; absent or `disable` is a plain
/// connection, as before). The certificate is always verified against Mozilla's root
/// store (`webpki-roots`, bundled so this works the same in every build): there is no
/// "encrypt but don't check who to" mode here, unlike libpq's own `require`.
fn wants_tls(url: &str) -> bool {
    url.split(['?', '&']).skip(1).any(|p| matches!(p, "sslmode=require" | "sslmode=prefer" | "sslmode=verify-ca" | "sslmode=verify-full"))
}

fn pg_connect(url: &str) -> Result<postgres::Client, HostError> {
    if !wants_tls(url) {
        return postgres::Client::connect(url, postgres::NoTls).map_err(|e| HostError::Other(e.to_string()));
    }
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();
    let tls = tokio_postgres_rustls::MakeRustlsConnect::new(config);
    postgres::Client::connect(url, tls).map_err(|e| HostError::Other(e.to_string()))
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
            Box::new(SqliteConn(rusqlite::Connection::open(path).map_err(|e| HostError::Other(e.to_string()))?))
        } else if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            Box::new(PostgresConn(pg_connect(url)?))
        } else {
            return Err(HostError::Other(format!("unknown database URL \"{url}\" (use sqlite://... or postgres://...)")));
        };
        let id = self.next_id;
        self.next_id += 1;
        self.open.insert(id, conn);
        Ok(id)
    }

    fn get(&mut self, conn: u64) -> Result<&mut Box<dyn SqlConn>, HostError> {
        self.open.get_mut(&conn).ok_or_else(|| HostError::Other("this database connection is closed".to_string()))
    }

    pub fn query(&mut self, conn: u64, sql: &str, params: Vec<SqlValue>) -> Result<Vec<SqlRow>, HostError> {
        self.get(conn)?.query(sql, &params).map_err(HostError::Other)
    }

    pub fn execute(&mut self, conn: u64, sql: &str, params: Vec<SqlValue>) -> Result<u64, HostError> {
        self.get(conn)?.execute(sql, &params).map_err(HostError::Other)
    }

    pub fn close(&mut self, conn: u64) -> Result<(), HostError> {
        self.get(conn)?;
        self.open.remove(&conn);
        Ok(())
    }
}
