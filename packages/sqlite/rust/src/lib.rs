//! SQLite (rusqlite, bundled) for Goose:
//! `import sqlite from "github.com/goose-rs/sqlite@v1";`.
//!
//! A connection, a parameter list and a query's rows are handles. A query
//! reads its rows when it runs, and `step` moves through them.

use goose::GooseBuf;
use rusqlite::types::Value;
use std::sync::Mutex;

/// A database connection. Release it with `sqlite::release` to close it.
#[goose::handle]
pub struct Db(Mutex<rusqlite::Connection>);

/// Parameters for a statement's `?`s, in order.
#[goose::handle]
pub struct Params(Vec<Value>);

/// The rows a query returned, and which one is current.
#[goose::handle]
pub struct Rows {
    names: Vec<String>,
    rows: Vec<Vec<Value>>,
    at: Option<usize>,
}

/// What a column of the current row holds.
#[derive(goose::Flat, Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum Type {
    Null,
    Integer,
    Real,
    Text,
    Blob,
    /// No current row, or no such column.
    Missing,
}

/// Opens (creating if needed) the database file.
#[goose::export]
pub fn open(path: &str) -> Result<Db, rusqlite::Error> {
    Ok(Db(Mutex::new(rusqlite::Connection::open(path)?)))
}

/// A database in memory.
#[goose::export]
pub fn open_memory() -> Result<Db, rusqlite::Error> {
    Ok(Db(Mutex::new(rusqlite::Connection::open_in_memory()?)))
}

/// Runs SQL statements, separated by `;`, that take no parameters.
#[goose::export]
pub fn exec(db: &Db, sql: &str) -> Result<(), rusqlite::Error> {
    db.0.lock().unwrap_or_else(|e| e.into_inner()).execute_batch(sql)
}

/// An empty parameter list.
#[goose::export]
pub fn params() -> Params {
    Params(Vec::new())
}

#[goose::export]
pub fn bind_int(p: &mut Params, v: i64) {
    p.0.push(Value::Integer(v));
}
#[goose::export]
pub fn bind_real(p: &mut Params, v: f64) {
    p.0.push(Value::Real(v));
}
#[goose::export]
pub fn bind_text(p: &mut Params, v: &str) {
    p.0.push(Value::Text(v.to_string()));
}
#[goose::export]
pub fn bind_blob(p: &mut Params, v: &[u8]) {
    p.0.push(Value::Blob(v.to_vec()));
}
#[goose::export]
pub fn bind_null(p: &mut Params) {
    p.0.push(Value::Null);
}

/// Runs one statement with parameters; how many rows it changed.
#[goose::export]
pub fn execute(db: &Db, sql: &str, p: &Params) -> Result<i64, rusqlite::Error> {
    let c = db.0.lock().unwrap_or_else(|e| e.into_inner());
    Ok(c.execute(sql, rusqlite::params_from_iter(p.0.iter()))? as i64)
}

/// Runs a query with parameters, reading its rows.
#[goose::export]
pub fn query(db: &Db, sql: &str, p: &Params) -> Result<Rows, rusqlite::Error> {
    let c = db.0.lock().unwrap_or_else(|e| e.into_inner());
    let mut st = c.prepare(sql)?;
    let names: Vec<String> = st.column_names().iter().map(|s| s.to_string()).collect();
    let n = names.len();
    let mut rows = Vec::new();
    let mut q = st.query(rusqlite::params_from_iter(p.0.iter()))?;
    while let Some(r) = q.next()? {
        let mut row = Vec::with_capacity(n);
        for i in 0..n {
            row.push(r.get::<_, Value>(i)?);
        }
        rows.push(row);
    }
    Ok(Rows { names, rows, at: None })
}

/// Runs a query that takes no parameters.
#[goose::export]
pub fn select(db: &Db, sql: &str) -> Result<Rows, rusqlite::Error> {
    query(db, sql, &Params(Vec::new()))
}

/// The rowid the last insert made.
#[goose::export]
pub fn last_insert_rowid(db: &Db) -> i64 {
    db.0.lock().unwrap_or_else(|e| e.into_inner()).last_insert_rowid()
}

/// Moves to the next row: false when there is none.
#[goose::export]
pub fn step(rows: &mut Rows) -> bool {
    let next = rows.at.map(|a| a + 1).unwrap_or(0);
    rows.at = Some(next.min(rows.rows.len()));
    next < rows.rows.len()
}

/// How many rows the query returned.
#[goose::export]
pub fn row_count(rows: &Rows) -> i64 {
    rows.rows.len() as i64
}

#[goose::export]
pub fn columns(rows: &Rows) -> i64 {
    rows.names.len() as i64
}

#[goose::export]
pub fn column_name(rows: &Rows, i: i64, out: &mut GooseBuf) -> bool {
    match rows.names.get(i.max(0) as usize) {
        Some(n) => {
            out.push_str(n);
            true
        }
        None => false,
    }
}

fn cell(rows: &Rows, i: i64) -> Option<&Value> {
    rows.rows.get(rows.at?)?.get(usize::try_from(i).ok()?)
}

/// What column `i` of the current row holds.
#[goose::export]
pub fn column_type(rows: &Rows, i: i64) -> Type {
    match cell(rows, i) {
        None => Type::Missing,
        Some(Value::Null) => Type::Null,
        Some(Value::Integer(_)) => Type::Integer,
        Some(Value::Real(_)) => Type::Real,
        Some(Value::Text(_)) => Type::Text,
        Some(Value::Blob(_)) => Type::Blob,
    }
}

/// Column `i` of the current row as an integer.
#[goose::export]
pub fn get_int(rows: &Rows, i: i64) -> Option<i64> {
    match cell(rows, i)? {
        Value::Integer(v) => Some(*v),
        Value::Real(v) => Some(*v as i64),
        _ => None,
    }
}

/// Column `i` of the current row as a number.
#[goose::export]
pub fn get_real(rows: &Rows, i: i64) -> Option<f64> {
    match cell(rows, i)? {
        Value::Integer(v) => Some(*v as f64),
        Value::Real(v) => Some(*v),
        _ => None,
    }
}

/// Column `i` of the current row as text.
#[goose::export]
pub fn get_text(rows: &Rows, i: i64) -> Option<String> {
    match cell(rows, i)? {
        Value::Text(s) => Some(s.clone()),
        Value::Integer(v) => Some(v.to_string()),
        Value::Real(v) => Some(v.to_string()),
        _ => None,
    }
}

/// Column `i` of the current row as bytes.
#[goose::export]
pub fn get_blob(rows: &Rows, i: i64) -> Option<Vec<u8>> {
    match cell(rows, i)? {
        Value::Blob(b) => Some(b.clone()),
        Value::Text(s) => Some(s.as_bytes().to_vec()),
        _ => None,
    }
}

/// The SQLite version inside.
#[goose::export]
pub fn sqlite_version() -> String {
    rusqlite::version().to_string()
}
