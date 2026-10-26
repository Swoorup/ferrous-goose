// test: needs reentrant
// Recursive callbacks without mutable globals remain eligible.
import cf;

export recursive fn first(n: i64) -> i64 {
    if n > 0 { return tally(n - 1) + 1; }
    return 0;
}

export recursive fn tally(n: i64) -> i64 {
    if n > 0 { return first(n - 1) + 1; }
    return 0;
}

fn main() {
    let value, ok = cf::call_tally(4);
    assert(ok && value == 4);
    let again, cached = cf::call_tally(2);
    assert(cached && again == 2);
    print("recursive callbacks: ", value, " ", again);
}
