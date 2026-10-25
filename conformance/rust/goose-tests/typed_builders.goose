// test: needs typed-builders
// A typed builder (goose-patches/0001): Rust appends whole elements to the
// caller's array, which a stock compiler cannot pass.
import cf;

fn main() {
    var sq: i64[>..] = [100];
    cf::fill_squares(4, sq);
    print("squares ", sq);
    print("fresh ", cf::fill_squares(3));
}
