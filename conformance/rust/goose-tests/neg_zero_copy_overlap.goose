// test: needs emit-interface
import cf;

fn main() {
    var xs: i64[>..] = [1, 2, 3];
    cf::zc_axpy(xs, xs);
}
