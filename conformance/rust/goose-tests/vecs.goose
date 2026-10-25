// Vec<T> results: built straight into the caller's array with typed
// builders, or counted and copied without them.
import cf;

fn main() {
    print("primes ", cf::primes(8));
    var hits: cf::Hit[>..] = [];
    for i in 5 { hits.push(cf::Hit { t: i as f32, id: i as u64, hit: true }); }
    let above = cf::hits_above(hits, 2.5);
    print("above: ", above.len, " first id ", above[0].id);
    let shapes = cf::shapes_of(4);
    var circles = 0;
    for s in shapes { match s { Circle c => { circles++; }, Rect r => {}, Dot => {} } }
    print("shapes ", shapes.len, " circles ", circles);
    let ps, ok = cf::try_primes(3);
    print("try ", ps, " ", ok);
    var err: u8[>..] = [];
    let ps2, ok2 = cf::try_primes(-1, err);
    print("try negative ", ok2, " ", ps2.len, ": ", err);
    print("doubled ", cf::doubled([1, 2, 3]));
}
