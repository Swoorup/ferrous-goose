// test: needs runtime-hooks
// test: env GOOSE_RUST_LEAKS=1
// The leak report at the end of the program (goose-patches/0003): every
// live handle, by type.
import cf;

fn main() {
    let a = cf::stack_new();
    let b = cf::stack_new();
    cf::release(a);
    print("leaving one stack behind");
}
