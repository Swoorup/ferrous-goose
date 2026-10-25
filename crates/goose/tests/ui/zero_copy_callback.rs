#[goose::export(zero_copy)]
pub fn callback(xs: &[i64], mut f: impl FnMut(i64) -> i64) -> i64 {
    xs.iter().map(|x| f(*x)).sum()
}

fn main() {}
