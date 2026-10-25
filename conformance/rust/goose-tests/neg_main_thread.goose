// test: build
// A main_thread function called from a worker.
import cf;

thread_fn worker(x: i64) {
    print(cf::on_main());
}

fn main() {
    let id = thread_spawn(worker, 1);
    thread_wait(id);
}
