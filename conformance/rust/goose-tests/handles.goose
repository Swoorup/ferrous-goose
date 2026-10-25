// Handles: Rust objects in a slot table, shared and exclusive borrows, an
// object taken by a call, the scoped helper, live counts.
import cf;

fn main() {
    let s = cf::stack_new();
    cf::push(s, 1);
    cf::push(s, 2);
    cf::push(s, 3);
    print("depth ", cf::depth(s));
    let top, ok = cf::pop(s);
    print("popped ", top, " ", ok, ", depth ", cf::depth(s));
    let other = cf::stack_new();
    cf::push(other, 10);
    cf::move_all(s, other);
    print("after move_all ", cf::depth(s), " and ", cf::depth(other));
    print("live ", cf::live_count(), " stacks ", cf::live_count_stack());
    print("sum taken ", cf::into_sum(s));
    print("live after take ", cf::live_count_stack());
    cf::release(other);
    print("live after release ", cf::live_count());

    // A handle from a constructor with no failure: the scoped helper.
    cf::with_stack_new() { st =>
        cf::push(st, 5);
        print("in scope depth ", cf::depth(st));
    };
    print("after scope ", cf::live_count());

    // A pinned handle used on its own thread.
    let l = cf::local_new();
    cf::local_bump(l);
    print("local ", cf::local_bump(l));
    cf::release(l);
    print("leaks: [", cf::leak_report(), "] live ", cf::live_count());
}
