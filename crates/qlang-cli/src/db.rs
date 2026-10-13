//! SQL connections for `Db.connect`. The URL's scheme picks the driver: `sqlite://path`
//! for SQLite (via `rusqlite`), `postgres://...` / `postgresql://...` for Postgres (via
//! the synchronous `postgres` crate — no async runtime needed, the interpreter is
//! synchronous too). Connections are kept open in a table keyed by a handle qlang only
//! ever sees as the `conn: int` field of its `Db` struct.

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

fn to_pg_params(params: &[SqlValue]) -> Vec<Box<dyn postgres::types::ToSql + Sync>> {
    params
        .iter()
        .map(|v| -> Box<dyn postgres::types::ToSql + Sync> {
            match v {
                SqlValue::Null => Box::new(Option::<String>::None),
                SqlValue::Int(i) => Box::new(*i),
                SqlValue::Float(f) => Box::new(*f),
                SqlValue::Text(s) => Box::new(s.clone()),
                SqlValue::Bool(b) => Box::new(*b),
            }
        })
        .collect()
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
        let refs: Vec<&(dyn postgres::types::ToSql + Sync)> = bound.iter().map(|b| b.as_ref()).collect();
        let rows = self.0.query(sql, &refs).map_err(|e| e.to_string())?;
        Ok(rows
            .iter()
            .map(|row| SqlRow { columns: row.columns().iter().enumerate().map(|(i, c)| (c.name().to_string(), pg_value(row, i))).collect() })
            .collect())
    }

    fn execute(&mut self, sql: &str, params: &[SqlValue]) -> Result<u64, String> {
        let bound = to_pg_params(params);
        let refs: Vec<&(dyn postgres::types::ToSql + Sync)> = bound.iter().map(|b| b.as_ref()).collect();
        self.0.execute(sql, &refs).map_err(|e| e.to_string())
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
            Box::new(SqliteConn(rusqlite::Connection::open(path).map_err(|e| HostError::Other(e.to_string()))?))
        } else if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            Box::new(PostgresConn(postgres::Client::connect(url, postgres::NoTls).map_err(|e| HostError::Other(e.to_string()))?))
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
