//! serde_json for Goose: `import sjson from "github.com/goose-rs/json@v1";`.
//!
//! A JSON value lives on the Rust side behind a `Json` handle. Reading a
//! part of a value gives a new handle to a copy of that part; text comes out
//! through builders.

use goose::GooseBuf;
use serde_json::Value;

/// A JSON value. Release it with `sjson::release` when done.
#[goose::handle]
pub struct Json(Value);

/// What a value is.
#[derive(goose::Flat, Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum Kind {
    Null,
    Bool,
    Number,
    String,
    Array,
    Object,
}

fn kind_of(v: &Value) -> Kind {
    match v {
        Value::Null => Kind::Null,
        Value::Bool(_) => Kind::Bool,
        Value::Number(_) => Kind::Number,
        Value::String(_) => Kind::String,
        Value::Array(_) => Kind::Array,
        Value::Object(_) => Kind::Object,
    }
}

/// Parses JSON text.
#[goose::export]
pub fn parse(text: &str) -> Result<Json, serde_json::Error> {
    serde_json::from_str(text).map(Json)
}

/// The value as compact JSON text.
#[goose::export]
pub fn stringify(j: &Json, out: &mut GooseBuf) {
    out.push_str(&j.0.to_string());
}

/// The value as indented JSON text.
#[goose::export]
pub fn pretty(j: &Json, out: &mut GooseBuf) {
    if let Ok(s) = serde_json::to_string_pretty(&j.0) {
        out.push_str(&s);
    }
}

/// What the value is.
#[goose::export]
pub fn kind(j: &Json) -> Kind {
    kind_of(&j.0)
}

/// The member of an object, if it has it.
#[goose::export]
pub fn get(j: &Json, key: &str) -> Option<Json> {
    j.0.get(key).cloned().map(Json)
}

/// The element of an array, if it has one at `i`.
#[goose::export]
pub fn at(j: &Json, i: i64) -> Option<Json> {
    if i < 0 {
        return None;
    }
    j.0.get(i as usize).cloned().map(Json)
}

/// The value at a JSON pointer (`/a/0/b`), if there is one.
#[goose::export]
pub fn pointer(j: &Json, path: &str) -> Option<Json> {
    j.0.pointer(path).cloned().map(Json)
}

/// How many elements or members it has; 0 for anything else.
#[goose::export]
pub fn len(j: &Json) -> i64 {
    match &j.0 {
        Value::Array(a) => a.len() as i64,
        Value::Object(o) => o.len() as i64,
        _ => 0,
    }
}

/// The value as a number, if it is one.
#[goose::export]
pub fn as_f64(j: &Json) -> Option<f64> {
    j.0.as_f64()
}

/// The value as an integer, if it is one that fits.
#[goose::export]
pub fn as_i64(j: &Json) -> Option<i64> {
    j.0.as_i64()
}

/// The value as a bool, if it is one.
#[goose::export]
pub fn as_bool(j: &Json) -> Option<bool> {
    j.0.as_bool()
}

/// The string's text, if the value is a string (copied out).
#[goose::export]
pub fn as_str(j: &Json) -> Option<String> {
    j.0.as_str().map(String::from)
}

/// An object's keys, one at a time.
#[goose::export]
pub fn keys(j: &Json) -> impl Iterator<Item = String> {
    let keys: Vec<String> = match &j.0 {
        Value::Object(o) => o.keys().cloned().collect(),
        _ => Vec::new(),
    };
    keys.into_iter()
}

/// A member of an object, set (the object grows by it when new).
#[goose::export]
pub fn set(j: &mut Json, key: &str, value: &Json) -> bool {
    match &mut j.0 {
        Value::Object(o) => {
            o.insert(key.to_string(), value.0.clone());
            true
        }
        _ => false,
    }
}

/// An element pushed onto an array.
#[goose::export]
pub fn push(j: &mut Json, value: &Json) -> bool {
    match &mut j.0 {
        Value::Array(a) => {
            a.push(value.0.clone());
            true
        }
        _ => false,
    }
}

#[goose::export]
pub fn null() -> Json {
    Json(Value::Null)
}
#[goose::export]
pub fn boolean(b: bool) -> Json {
    Json(Value::Bool(b))
}
#[goose::export]
pub fn number(x: f64) -> Json {
    Json(serde_json::Number::from_f64(x).map(Value::Number).unwrap_or(Value::Null))
}
#[goose::export]
pub fn integer(x: i64) -> Json {
    Json(Value::from(x))
}
#[goose::export]
pub fn string(s: &str) -> Json {
    Json(Value::String(s.to_string()))
}
#[goose::export]
pub fn object() -> Json {
    Json(Value::Object(Default::default()))
}
#[goose::export]
pub fn array() -> Json {
    Json(Value::Array(Vec::new()))
}

/// Whether two values are equal.
#[goose::export]
pub fn equal(a: &Json, b: &Json) -> bool {
    a.0 == b.0
}

/// A copy of the value, under a handle of its own.
#[goose::export]
pub fn copy(j: &Json) -> Json {
    Json(j.0.clone())
}
