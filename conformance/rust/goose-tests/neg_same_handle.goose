// The same handle as a &mut and a & argument of one call.
import cf;

fn main() {
    let s = cf::stack_new();
    cf::move_all(s, s);
    print("not reached");
}
