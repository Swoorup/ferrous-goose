#[derive(Clone, Copy, goose::Flat)]
#[repr(C)]
pub struct Data { x: i64 }

#[goose::export(zero_copy)]
pub fn by_value(data: Data) -> i64 { data.x }

#[goose::export(zero_copy)]
pub fn result() -> Data { Data { x: 1 } }

fn main() {}
