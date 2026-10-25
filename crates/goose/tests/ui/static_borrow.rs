#[goose::export]
pub fn keep(s: &'static str) -> i64 {
    s.len() as i64
}

fn main() {}
