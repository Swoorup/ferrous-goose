//! A Rust program embedding Goose: `cargo run -p goose-embed-example`.

goose::include!(kernels); // a safe module, one function per export fn

fn main() {
    let rt = goose::Runtime::init().unwrap(); // !Send: Goose calls stay on this thread
    println!("{}", String::from_utf8_lossy(kernels::banner(&rt)));
    let n = kernels::count_words(&rt, b"no heap, no gc, no lifetime annotations");
    println!("{n} words");

    let mut xs = vec![3.0, 1.5, 4.0, 1.0, 5.5];
    let s = kernels::stats(&rt, &xs);
    let (count, sum, min, max) = (s.count, s.sum, s.min, s.max);
    println!("stats: {count} values, sum {sum}, min {min}, max {max}");
    kernels::scale(&rt, &mut xs, 2.0);
    println!("scaled in place: {xs:?}");
}
