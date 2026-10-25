// test: build
// A pinned handle (its type is not Send + Sync) used on another thread.
import cf;

thread_fn worker(l: cf::Local) {
    print(cf::local_bump(l));
}

fn main() {
    let l = cf::local_new();
    print("main: ", cf::local_bump(l));
    let id = thread_spawn(worker, l);
    thread_wait(id);
    print("not reached");
}
