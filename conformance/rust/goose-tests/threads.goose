// test: build
// A Send + Sync handle shared with worker threads, each calling Rust.
import cf;

thread_fn worker(s: cf::Stack, base: i64) {
    for i in 10 { cf::push(s, base + i); }
    qput(1);
}

fn main() {
    let s = cf::stack_new();
    var ids: i64[>..] = [];
    for w in 4 { ids.push(thread_spawn(worker, s, w * 100)); }
    for w in 4 { qget<i64>(); }
    for id in ids { thread_wait(id); }
    print("depth ", cf::depth(s));
    print("on main ", cf::on_main());
    cf::release(s);
}
