// Slices: lent in place when aligned, copied (and written back) when Goose's
// packed storage leaves them unaligned; slices of structs and of bytes;
// owned parameters.
import cf;

// Packed: vals starts at offset 1, so it is not 4-aligned.
struct P { tag: u8, vals: i32[4] }

fn main() {
    var xs: i32[>..] = [1, 2, 3, 4];
    print("sum ", cf::sum_i32(xs));
    cf::scale_i32(xs, 10);
    print("scaled ", xs);

    var p = P { tag: 1, vals: [1, 2, 3, 4] };
    print("Rust sees aligned slices: ", cf::lent_in_place(xs), ", packed field ", cf::lent_in_place(p.vals));
    cf::scale_i32(p.vals, 3);
    print("unaligned scaled and written back ", p.vals, " tag ", p.tag);
    print("unaligned sum ", cf::sum_i32(p.vals));

    var fs: f64[>..] = [0.5, 0.25, 0.25];
    print("f64 sum ", cf::sum_f64(fs));

    var hits: cf::Hit[>..] = [];
    hits.push(cf::Hit { t: 3.5, id: 7, hit: true });
    hits.push(cf::Hit { t: 1.25, id: 42, hit: true });
    hits.push(cf::Hit { t: 0.5, id: 9, hit: false });
    let best, found = cf::nearest(hits);
    print("nearest ", found, " id ", best.id, " at ", best.t);
    var none: cf::Hit[>..] = [];
    let nb, nf = cf::nearest(none);
    print("nearest of none ", nf);

    print("xor ", cf::bytes_xor("abc"));
    var text: u8[>..] = [];
    format(text, "make me loud");
    cf::upper_in_place(text);
    print(text);

    var a: i32[>..] = [0, 0, 0];
    let b: i32[3] = [7, 8, 9];
    cf::copy_into(a, b);
    print("copied ", a);
    print("owned ", cf::owned_len("four", xs, "xy"));
}
