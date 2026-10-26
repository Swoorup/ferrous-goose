// test: build
// test: needs runtime-hooks
// A Rust abort in a contained worker (goose-patches/0003): only that worker
// ends, and thread_wait hands its message to the program, which runs on.
import cf;

thread_fn worker(s: cf::Stack) {
    cf::release(s);
    cf::push(s, 1);
}

fn main() {
    let s = cf::stack_new();
    let id = thread_spawn_contained(worker, s);
    var msg: u8[>..] = [];
    let ok = thread_wait(id, msg);
    print("worker returned: ", ok);
    print("its message: ", msg);
    print("the program runs on; live ", cf::live_count());
}
