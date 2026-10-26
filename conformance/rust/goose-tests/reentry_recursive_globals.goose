// test: needs reentrant
// A recursive edge must not cache a safe result for another exported entry.
import cf;

var count: i64 = 0;

export recursive fn first(n: i64) -> i64 {
    if n > 0 { tally(n - 1); }
    count += 1;
    return count;
}

export recursive fn tally(n: i64) -> i64 {
    if n > 0 { return first(n - 1); }
    return 0;
}

fn main() {
    var err: u8[>..] = [];
    let value, ok = cf::call_tally(4, err);
    assert(!ok);
    assert(count == 0);
    print(err);
    print("count untouched: ", count);
}
