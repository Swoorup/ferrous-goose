#[goose::export]
pub fn first<T: Copy + Default>(xs: &[T]) -> T {
    xs.first().copied().unwrap_or_default()
}

fn main() {}
