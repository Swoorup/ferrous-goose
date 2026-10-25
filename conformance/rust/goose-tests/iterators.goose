// Iterators: parts of an argument as span lists handed to a block (no
// copies), and lazy iterators making values behind a handle with next.
import cf;

fn main() {
    var n = 0;
    cf::each_words("no heap no gc no lifetime annotations") { w =>
        n++;
        if n <= 2 { print("word: ", w); }
    };
    print(n, " words");
    let spans = cf::words("a bb ccc");
    print("spans ", spans.len, " last at ", spans[2].at, " len ", spans[2].len);
    var xs: i64[>..] = [1, 1, 2, 3, 3, 3];
    cf::each_runs(xs) { r => print("run of ", r[0], " x", r.len); };

    var total = 0;
    cf::each_countdown(4) { k => total += k; };
    print("countdown total ", total);
    let it = cf::countdown(2);
    let a, ok = cf::next(it);
    let b, ok2 = cf::next(it);
    print("next ", a, " ", b, " ", ok && ok2);
    cf::release(it);
    cf::each_labels(2) { l => print(l); };
    cf::each_hits_iter(4 as u64) { h => if h.hit { print("hit ", h.id, " at ", h.t); } };
    print("live ", cf::live_count());
}
