// Callbacks: the Rust function runs as a coroutine, and each call it makes
// of its closure reaches the caller's block, whose result goes back.
import cf;

fn main() {
    var xs: i64[>..] = [1, 2, 3, 4, 5, 6];
    let evens = cf::filter_sum(xs) { x => x % 2 == 0 };
    print("sum of evens ", evens);
    var seen = 0;
    let big = cf::filter_sum(xs) { x =>
        seen++;
        x > 4
    };
    print("sum over 4 ", big, ", called ", seen, " times");

    var hits: cf::Hit[>..] = [];
    hits.push(cf::Hit { t: 1.0, id: 1, hit: true });
    hits.push(cf::Hit { t: 2.0, id: 2, hit: false });
    let n = cf::visit_hits(hits) { label, h => print(label, ": id ", h.id, " hit ", h.hit); };
    print("visited ", n);

    let s = cf::stack_new();
    cf::push(s, 2);
    cf::push(s, 3);
    let total = cf::map_stack(s) { v => cf::Hit { t: (v * 10) as f32, id: v as u64, hit: true } };
    print("mapped total ", total);
    cf::release(s);
    print("live ", cf::live_count());
}
