#[goose::export(zero_copy)]
pub fn owned(s: String) -> i64 { s.len() as i64 }

#[goose::export(zero_copy)]
pub fn optional(xs: Option<Vec<i32>>) -> i64 { xs.map_or(0, |v| v.len() as i64) }

#[goose::export(zero_copy)]
pub fn vector() -> Vec<i64> { vec![1, 2] }

#[goose::export(zero_copy)]
pub fn text() -> Result<String, String> { Ok("copied".into()) }

#[goose::export(zero_copy)]
pub fn lazy() -> impl Iterator<Item = i64> { 0..3 }

#[goose::handle]
pub struct Object { text: String }

#[goose::export(zero_copy)]
pub fn borrowed(object: &Object) -> &str { &object.text }

fn main() {}
