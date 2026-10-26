// The embed conformance program for a compiler with the Ferrous Goose
// patches: everything basic.goose has, and exports with several results,
// builders (goose-patches/0001) and aligned structs (0006).
import basic;

struct Config { level: i32, verbose: bool }

export fn parse_config(text: const u8[:]) -> Config, bool {
    if text.len == 0 { return Config { level: 0, verbose: false }, false; }
    return Config { level: text.len as i32, verbose: text[0] == 'v' }, true;
}

export fn render_config(cfg: const Config&, out: u8[>..]&) {
    format(out, "level=", cfg.level, " verbose=", cfg.verbose);
}

export fn squares(n: i64, out: i64[>..]&) -> i64 {
    for i in n { out.push(i * i); }
    return n;
}

export fn split_half(text: const u8[:]) -> const u8[:], const u8[:] {
    let h = text.len / 2;
    return text[0..h], text[h..text.len];
}

export fn minmax(xs: const i64[:]) -> i64, i64, bool {
    if xs.len == 0 { return 0, 0, false; }
    var lo = xs[0];
    var hi = xs[0];
    for x in xs {
        if x < lo { lo = x; }
        if x > hi { hi = x; }
    }
    return lo, hi, true;
}

struct Hit { align 8, t: f32, pad 4, id: u64, hit: bool, pad 7 }

export fn best_hit(hits: const Hit[:]) -> Hit, bool {
    var best = Hit { t: 0.0, id: 0, hit: false };
    var found = false;
    for h in hits {
        if h.hit && (!found || h.t < best.t) {
            best = h;
            found = true;
        }
    }
    return best, found;
}

export fn hits_above(hits: const Hit[:], t: f32, out: Hit[>..]&) {
    for h in hits { if h.t > t { out.push(h); } }
}

export fn two_builders(n: i64, words: u8[>..]&, nums: i64[>..]&) {
    for i in n {
        format(words, "w", i, " ");
        nums.push(i * 10);
    }
}
