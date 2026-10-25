//! Rust calling Goose, every export shape: `cargo run -p goose-embed-conformance`.
//! Exits with 0 and prints the number of checks when they all hold.

goose::include!(embed);

fn main() {
    let mut rt = goose::Runtime::init().expect("the Goose runtime");
    let mut checks = 0;
    let mut check = |ok: bool, what: &str| {
        checks += 1;
        if !ok {
            eprintln!("FAIL: {what}");
            std::process::exit(1);
        }
    };

    // Scalars.
    check(embed::add(&rt, 40, 2) == 42, "add");
    check(embed::mul_f(&rt, 1.5, 2.0) == 3.0, "mul_f");
    check(embed::negate(&rt, false), "negate false");
    check(!embed::negate(&rt, true), "negate true");
    check(embed::small(&rt, 200, -100) == 100, "small");

    // Slices, read and written in place.
    check(embed::sum_bytes(&rt, b"abc") == 97 + 98 + 99, "sum_bytes");
    let mut xs = [1, 2, 3, 4, 5];
    embed::reverse_in_place(&rt, &mut xs);
    check(xs == [5, 4, 3, 2, 1], "reverse_in_place");

    // Structs by value, by reference, through &mut.
    let v = embed::make_vec(&rt, 1.0, 2.0, 3.0);
    check(embed::dot(&rt, &v, &v) == 14.0, "dot");
    let mut p = embed::Particle {
        pos: embed::Vec3 { x: 0.0, y: 0.0, z: 0.0 },
        vel: embed::Vec3 { x: 1.0, y: 2.0, z: 3.0 },
        tag: 7,
    };
    embed::step(&rt, &mut p, 0.5);
    let (px, py, tag) = (p.pos.x, p.pos.y, p.tag);
    check(px == 0.5 && py == 1.0 && tag == 8, "step");
    let mut n = 41i64;
    embed::bump(&rt, &mut n);
    check(n == 42, "bump");

    // Borrowed results: static data, a view of the argument.
    check(embed::greeting(&rt) == b"hello from goose", "greeting");
    let text = b"   padded   ".to_vec();
    check(embed::trim(&rt, &text) == b"padded", "trim");

    // Enums, globals.
    let m = embed::next_mode(&rt, embed::Mode { tag: embed::Mode::FAST, payload: [] });
    check(m.variant() == "Off", "next_mode");
    check(embed::count_call(&rt) == 1 && embed::count_call(&rt) == 2, "globals between calls");

    #[cfg(goose_full)]
    {
        // Several results (goose-patches/0002).
        let (cfg, ok) = embed::parse_config(&rt, b"verbose");
        let (level, verbose) = (cfg.level, cfg.verbose);
        check(ok && level == 7 && verbose, "parse_config");
        let (_, ok) = embed::parse_config(&rt, b"");
        check(!ok, "parse_config failure");
        let (a, b) = embed::split_half(&rt, b"abcdef");
        check(a == b"abc" && b == b"def", "split_half");
        let (lo, hi, ok) = embed::minmax(&rt, &[4, -2, 9, 0]);
        check(ok && lo == -2 && hi == 9, "minmax");

        // Builders: the runtime's scratch, held while the runtime is borrowed.
        let text = embed::render_config(&mut rt, &cfg);
        check(text == b"level=7 verbose=true", "render_config");
        let (count, sq) = embed::squares(&mut rt, 5);
        check(count == 5 && sq == [0, 1, 4, 9, 16], "squares");
        let (words, nums) = embed::two_builders(&mut rt, 3);
        check(words == b"w0 w1 w2 " && nums == [0, 10, 20], "two builders");

        // Aligned structs (goose-patches/0007): Rust's own alignment.
        check(std::mem::align_of::<embed::Hit>() == 8 && std::mem::size_of::<embed::Hit>() == 24, "Hit layout");
        let mk = |t: f32, id: u64, hit: bool| embed::Hit { t, id, hit, ..Default::default() };
        let hits = [mk(3.5, 1, true), mk(1.25, 2, true), mk(0.5, 3, false)];
        let (best, found) = embed::best_hit(&rt, &hits);
        check(found && best.id == 2, "best_hit");
        let above = embed::hits_above(&mut rt, &hits, 1.0);
        check(above.len() == 2 && above[0].id == 1 && above[1].id == 2, "hits_above");
    }
    let _ = &mut rt;
    println!("embed conformance: {checks} checks ok{}", if cfg!(goose_full) { " (full)" } else { " (basic)" });
}
