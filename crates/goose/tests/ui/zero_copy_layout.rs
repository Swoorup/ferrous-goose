#[derive(Clone, Copy, goose::Flat)]
#[repr(C, u8)]
pub enum Converted { A { x: i64 }, B }

#[goose::export(zero_copy)]
pub fn reference(_x: &Converted) {}

#[goose::export(zero_copy)]
pub fn slice(_x: &[Converted]) {}

#[goose::export(zero_copy)]
pub fn builder(_out: &mut goose::GooseVec<Converted>) {}

fn main() {}
