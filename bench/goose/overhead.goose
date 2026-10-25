// The overhead budget: what Ferrous Goose's boundary checks cost per call,
// against bare extern calls doing the same work. bench/run.sh builds and
// runs this (goose build, -O2) and writes bench/RESULTS.md.
import bench;
import os;

// The same library's bare functions, bound by hand as C would be.
extern "goose_bench" {
    fn bare_add(a: i64, b: i64) -> i64;
    fn bare_sum(xs: const i64[:]) -> i64;
    fn bare_len_bytes(b: const u8[:]) -> i64;
    fn bare_axpy(y: f64[:], x: const f64[:], a: f64);
}

var sink: i64 = 0;

// Tenths of a nanosecond, as text.
fn ns(out: u8[>..]&, x: f64) {
    let t = (x * 10.0 + 0.5) as i64;
    format(out, t / 10, ".", t % 10);
}

fn report(name: const u8[:], bare_ns: f64, bridged_ns: f64) {
    var line: u8[>..] = [];
    format(line, "| ", name, " | ");
    if bare_ns > 0.0 { ns(line, bare_ns); }
    format(line, " | ");
    ns(line, bridged_ns);
    format(line, " | ");
    if bare_ns > 0.0 { ns(line, bridged_ns - bare_ns); }
    format(line, " |");
    print(line);
}

fn per_call(start: i64, n: i64) -> f64 {
    return (clock_ns() - start) / (n as f64);
}

fn main() {
    let n: i64 = 2000000;
    var xs: i64[>..] = [];
    for i in 64 { xs.push(i); }
    var ys: f64[>..] = [];
    var zs: f64[>..] = [];
    for i in 64 {
        ys.push(i);
        zs.push(1.0);
    }
    var text: u8[>..] = [];
    for i in 64 { text.push('a'); }
    var long_text: u8[>..] = [];
    for i in 4096 { long_text.push('b'); }

    print("| call | bare (ns) | bridged (ns) | checks (ns) |");
    print("|---|---|---|---|");

    var t = clock_ns();
    for i in n { sink += bare_add(i, 1); }
    let bare_add_ns = per_call(t, n);
    t = clock_ns();
    for i in n { sink += bench::add(i, 1); }
    report("scalar `add(i64, i64)`", bare_add_ns, per_call(t, n));

    t = clock_ns();
    for i in n { sink += bare_sum(xs); }
    let bare_sum_ns = per_call(t, n);
    t = clock_ns();
    for i in n { sink += bench::sum(xs); }
    report("`&[i64]` of 64, summed", bare_sum_ns, per_call(t, n));

    t = clock_ns();
    for i in n { sink += bare_len_bytes(text); }
    let bare_len_ns = per_call(t, n);
    t = clock_ns();
    for i in n { sink += bench::len_bytes(text); }
    report("`&[u8]` of 64", bare_len_ns, per_call(t, n));
    t = clock_ns();
    for i in n { sink += bench::len_str(text); }
    report("`&str` of 64 (UTF-8 check)", bare_len_ns, per_call(t, n));
    let m = n / 20;
    t = clock_ns();
    for i in m { sink += bare_len_bytes(long_text); }
    let bare_long_ns = per_call(t, m);
    t = clock_ns();
    for i in m { sink += bench::len_str(long_text); }
    report("`&str` of 4096 (UTF-8 check)", bare_long_ns, per_call(t, m));

    t = clock_ns();
    for i in n { bare_axpy(ys, zs, 0.5); }
    let bare_axpy_ns = per_call(t, n);
    t = clock_ns();
    for i in n { bench::axpy(ys, zs, 0.5); }
    report("`axpy(&mut [f64], &[f64])` of 64 (overlap check)", bare_axpy_ns, per_call(t, n));

    let c = bench::counter();
    t = clock_ns();
    for i in n { sink += bench::bump(c, 1); }
    report("handle `&mut Counter`", 0.0, per_call(t, n));
    t = clock_ns();
    for i in n { sink += bench::peek(c); }
    report("handle `&Counter`", 0.0, per_call(t, n));
    bench::release(c);

    var err: u8[>..] = [];
    t = clock_ns();
    for i in n {
        let v, ok = bench::checked(i, 1, err);
        sink += v;
    }
    report("`Result<i64, String>`", bare_add_ns, per_call(t, n));

    t = clock_ns();
    for i in m {
        var out: u8[>..] = [];
        bench::fill(16, out);
        sink += out.len;
    }
    report("builder, 16 bytes appended", 0.0, per_call(t, m));
    print("");
    print("(sink ", sink % 10, ")");
}
