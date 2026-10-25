// Generic functions: each instantiation a Goose overload of its own symbol.
import cf;

fn main() {
    var a: f32[>..] = [1.0, 2.0];
    var b: f32[>..] = [3.0, 4.0];
    print("f32 ", cf::dot(a, b));
    var c: f64[>..] = [0.5, 0.5];
    print("f64 ", cf::dot(c, c));
    var d: i64[>..] = [2, 3];
    print("i64 ", cf::dot(d, d));
    var e: i32[>..] = [4, 9, 2];
    var f: u8[>..] = [7, 200, 3];
    print("largest ", cf::largest(e), " ", cf::largest(f));
}
