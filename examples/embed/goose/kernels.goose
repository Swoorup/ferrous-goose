// Goose kernels a Rust program calls: export fns, which goose_build binds.

// How many words the text has.
export fn count_words(text: const u8[:]) -> i64 {
    var n = 0;
    var inword = false;
    for c in text {
        let space = c == ' ' || c == '\n' || c == '\t';
        if !space && !inword { n++; }
        inword = !space;
    }
    return n;
}

// The library's name, from static data: Rust borrows it for as long as the
// runtime.
export fn banner() -> const u8[:] { return "goose kernels 1.0"; }

struct Stats { count: i64, sum: f64, min: f64, max: f64 }

// Summary statistics of a series, in one pass.
export fn stats(xs: const f64[:]) -> Stats {
    var s = Stats { count: 0, sum: 0.0, min: 0.0, max: 0.0 };
    for x in xs {
        if s.count == 0 || x < s.min { s.min = x; }
        if s.count == 0 || x > s.max { s.max = x; }
        s.sum += x;
        s.count++;
    }
    return s;
}

// Scales a series in place.
export fn scale(xs: f64[:], k: f64) {
    for i in xs.len { xs[i] *= k; }
}
