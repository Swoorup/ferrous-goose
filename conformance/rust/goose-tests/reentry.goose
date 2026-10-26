// test: needs reentrant
// Rust calling the program back while it waits (goose-patches/0003): the
// call back starts above every live stack top, may call Rust again, and
// only export fns that leave the var globals alone can be called back.
import cf;

var count: i64 = 0;

// Allocates on a data stack while Rust waits, and calls Rust again.
export fn on_node(depth: i64, label: const u8[:]) -> i64 {
    var parts: i64[>..] = [];
    for i in depth + 1 { parts.push(i); }
    var s: i64 = 0;
    for p in parts { s += p; }
    return cf::add_i64(s, label.len);
}

export fn tally(x: i64) -> i64 {
    count += x;
    return count;
}

export fn wrong(x: f64) -> f64 { return x; }

export fn nudge(p: cf::Hit&, by: f32) { p.t += by; }

fn main() {
    var keep: i64[>..] = [7, 8, 9];
    let total, ok = cf::walk(4);
    keep.push(10);
    print("walk ", total, " ", ok, "; live data kept: ", keep);

    var e1: u8[>..] = [];
    let t1, ok1 = cf::call_tally(3, e1);
    print("tally: ", ok1, ": ", e1);
    var e2: u8[>..] = [];
    let t2, ok2 = cf::call_missing(e2);
    print("missing: ", ok2, ": ", e2);
    var e3: u8[>..] = [];
    let t3, ok3 = cf::call_wrong(e3);
    print("wrong: ", ok3, ": ", e3);

    var hits: cf::Hit[>..] = [];
    hits.push(cf::Hit { t: 1.0, id: 1, hit: true });
    hits.push(cf::Hit { t: 2.0, id: 2, hit: true });
    assert(cf::nudge_all(hits));
    print("nudged ", hits[0].t, " ", hits[1].t);
    print("count untouched: ", count);
}
