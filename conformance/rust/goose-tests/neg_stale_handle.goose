// A handle used after release: a clean abort naming it, never freed memory.
import cf;

fn main() {
    let s = cf::stack_new();
    cf::push(s, 1);
    cf::release(s);
    print("released; using the stale handle next");
    cf::push(s, 2);
    print("not reached");
}
