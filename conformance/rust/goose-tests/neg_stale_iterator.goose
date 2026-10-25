// An iterator used after release.
import cf;

fn main() {
    let it = cf::countdown(3);
    cf::release(it);
    let x, ok = cf::next(it);
}
