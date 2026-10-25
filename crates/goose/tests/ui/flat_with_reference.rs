#[derive(goose::Flat, Clone, Copy)]
#[repr(C)]
pub struct Named {
    name: &'static str,
}

fn main() {}
