// Rust's rule that a &mut has no alias: the same array as both arguments.
import cf;

fn main() {
    var xs: i32[>..] = [1, 2, 3];
    cf::copy_into(xs, xs);
    print("not reached");
}
