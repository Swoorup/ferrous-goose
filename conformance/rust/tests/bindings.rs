//! cargo goose's side, on this crate's own records: reading them, the
//! version checks, and the module written for stock and patched compilers.

use cargo_goose::gen::{generate, Link, Options};
use cargo_goose::meta;

/// The crate's shared library, built for this test (a test binary links only
/// the parts of the rlib it calls, and with them only their records; and
/// `cargo test` leaves the cdylib in target/ as the last `cargo build` made it).
fn cdylib() -> std::path::PathBuf {
    static PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    PATH.get_or_init(|| {
        let target = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cdylib");
        let st = std::process::Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
            .args(["build", "-q", "--lib", "-p", "goose-conformance", "--target-dir"])
            .arg(&target)
            .status()
            .expect("cargo");
        assert!(st.success(), "building the cdylib");
        let file = if cfg!(windows) {
            "goose_conformance.dll"
        } else if cfg!(target_os = "macos") {
            "libgoose_conformance.dylib"
        } else {
            "libgoose_conformance.so"
        };
        target.join("debug").join(file)
    })
    .clone()
}

fn library() -> meta::Library {
    meta::read_library(&cdylib()).expect("the records parse")
}

fn module(typed: bool, align: bool) -> String {
    let o = Options { link: Link::Dynamic("goose_conformance".into()), typed_builders: typed, align, reentrant: align, emit_interface: align };
    generate(&library(), &o).expect("a module")
}

#[test]
fn versions_are_checked() {
    let lib = libloading_json();
    let json = lib.replacen("\"abi\":1", "\"abi\":99", 1);
    let err = meta::parse(&json, "libx.dylib").unwrap_err();
    assert!(err.contains("bridge ABI 99"), "{err}");
    assert!(err.contains("Rebuild the crate"), "{err}");
}

#[test]
fn layouts_carry_rustc_pads() {
    let m = module(false, false);
    assert!(m.contains("struct Hit { t: f32, pad 4, id: u64, hit: bool, pad 7 }"), "{m}");
    assert!(m.contains("struct Particle { pos: Vec3, vel: Vec3, tag: u8, pad 3, history: f32[3] }"));
    assert!(m.contains("enum Shape { Circle { r: f64 }, Rect { w: f64, h: f64 }, Dot }"));
    assert!(m.contains("struct Tagged { id: u16, shape: Shape }"), "a converted struct is packed");
    let aligned = module(true, true);
    assert!(aligned.contains("struct Hit { align 8, t: f32, pad 4, id: u64, hit: bool, pad 7 }"));
    assert!(aligned.contains("enum AShape { align 8, Circle { r: f64, pad 8 }, Rect { w: f64, h: f64 }, Dot }"));
}

#[test]
fn wrappers_take_goose_shapes() {
    let m = module(false, false);
    // A borrowed result re-slices the argument.
    assert!(m.contains("return s[r_at..r_at + r_len];"), "trim");
    // Result: a value and a bool, with and without the error text.
    assert!(m.contains("fn parse_int(s: const u8[:], err: u8[>..]&) -> i64, bool {"));
    assert!(m.contains("fn parse_int(s: const u8[:]) -> i64, bool {"));
    // An instantiation list: overloads bound to their own symbols.
    assert!(m.contains("fn dot(a: const f32[:], b: const f32[:]) -> f32 = \"cf_dot_f32\";"));
    // An optional parameter: one entry point per case, and a narrowing wrapper.
    assert!(m.contains("fn cf_describe_config_some(c: const Config&, out: u8[>..]&);"));
    assert!(m.contains("fn describe_config(c: Config?, out: u8[>..]&) {"));
    // A callback: the block is called back.
    assert!(m.contains("fn filter_sum<F>(xs: const i64[:]) -> i64 {"));
    // throws: generic, so it is checked only inside a try.
    assert!(m.contains("fn strict_parse(s) -> i64 {"));
}

#[test]
fn typed_builders_or_counted_copies() {
    let stock = module(false, false);
    assert!(stock.contains("fn cf_primes__len(n: i64) -> i64;"));
    assert!(stock.contains("cf_primes__take(r_out);"));
    assert!(stock.contains("// fill_squares: takes a typed builder"));
    assert!(stock.contains("// AShape: #[goose(align)]"), "an aligned enum needs the patch");
    let patched = module(true, true);
    assert!(patched.contains("fn cf_primes(n: i64, out: i64[>..]&);"));
    assert!(patched.contains("fn fill_squares(n: i64, out: i64[>..]&) = \"cf_fill_squares\";"));
}

#[test]
fn static_linking_uses_plain_externs() {
    let o = Options { link: Link::Static, typed_builders: true, align: true, reentrant: true, emit_interface: true };
    let m = generate(&library(), &o).unwrap();
    assert!(m.contains("extern fn goose_abi_1() -> u32;"));
    assert!(!m.contains("extern \"goose_conformance\""));
}

/// The records' JSON, as the library gives it.
fn libloading_json() -> String {
    let lib = unsafe { libloading::Library::new(cdylib()) }.unwrap();
    let f: libloading::Symbol<unsafe extern "C" fn(*mut usize) -> *const u8> = unsafe { lib.get(b"goose_metadata\0") }.unwrap();
    let mut len = 0usize;
    let p = unsafe { f(&mut len) };
    let json = String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(p, len) }).into_owned();
    std::mem::forget(lib);
    json
}

/// The patched compiler, where scripts/toolchain.sh built it.
fn patched_goose() -> Option<std::path::PathBuf> {
    let g = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.toolchain/goose/build/goose");
    let info = cargo_goose::toolchain::goose_info(&g).ok()?;
    info.has("emit-interface").then_some(g)
}

#[test]
fn the_compiler_agrees_with_the_layouts() {
    let Some(goose) = patched_goose() else {
        eprintln!("skipped: no compiler with --emit-interface in .toolchain/");
        return;
    };
    let lib = library();
    let m = module(true, true);
    cargo_goose::verify::verify(&goose, &lib, &m).expect("the module's layouts are rustc's");
    // A declaration that lost a pad no longer matches.
    let broken = m.replace("struct Hit { align 8, t: f32, pad 4, id: u64,", "struct Hit { align 8, t: f32, id: u64, pad 4,");
    assert_ne!(broken, m);
    let err = cargo_goose::verify::verify(&goose, &lib, &broken).unwrap_err();
    assert!(err.contains("Hit.id: Goose puts it at Some(4), the bridge at 8"), "{err}");
}

#[test]
fn calling_back_is_reentrant() {
    let patched = module(true, true);
    assert!(patched.contains("reentrant fn cf_walk(depth: i64, out: i64&, err: u8[>..]&) -> bool;"), "{patched}");
    let stock = module(false, false);
    assert!(stock.contains("// walk: calls the program back"));
}

#[test]
fn strict_bindings_require_verification_and_never_use_copy_fallbacks() {
    let lib = library();
    let strict = lib.functions.iter().find(|f| f.name == "zc_sum").unwrap();
    assert!(strict.zero_copy);
    let stock = module(false, false);
    assert!(stock.contains("zc_sum: zero_copy needs compiler layout verification"));
    assert!(!stock.contains("fn cf_zc_sum("));
    let patched = module(true, true);
    assert!(patched.contains("// zero_copy: cf_zc_sum\n"));
    assert!(!patched.contains("cf_zc_words__len"));
    assert!(!patched.contains("cf_zc_words__take"));
    let opts = Options { link: Link::Static, typed_builders: false, align: true, reentrant: true, emit_interface: true };
    let no_builders = generate(&lib, &opts).unwrap();
    assert!(no_builders.contains("zc_words: zero_copy needs typed builders"));
    assert!(!no_builders.contains("fn cf_zc_words("));
    // Other exports still have their ordinary stock-compiler fallback.
    assert!(no_builders.contains("cf_primes__len"));
}

#[test]
fn strict_metadata_cannot_claim_a_converted_type() {
    let mut lib = library();
    lib.structs.iter_mut().find(|s| s.name == "Hit").unwrap().same_layout = false;
    let opts = Options { link: Link::Static, typed_builders: true, align: true, reentrant: true, emit_interface: true };
    let err = generate(&lib, &opts).unwrap_err();
    assert!(err.contains("zero_copy requires Hit to have the same layout"), "{err}");
}

#[test]
fn strict_enum_layouts_are_checked_beyond_total_size() {
    let Some(goose) = patched_goose() else { return };
    let lib = library();
    let m = module(true, true);
    cargo_goose::verify::verify(&goose, &lib, &m).unwrap();
    // Keep the union size and all field offsets, but exchange the variant tags.
    let broken = m.replace(
        "enum AShape { align 8, Circle { r: f64, pad 8 }, Rect { w: f64, h: f64 }, Dot }",
        "enum AShape { align 8, Rect { w: f64, h: f64 }, Circle { r: f64, pad 8 }, Dot }",
    );
    assert_ne!(broken, m);
    let err = cargo_goose::verify::verify(&goose, &lib, &broken).unwrap_err();
    assert!(err.contains("zero_copy variant tag differs"), "{err}");
    // Keep the total size while moving the fields of a strict payload.
    let broken = m.replace("Rect { w: f64, h: f64 }, Dot", "Rect { h: f64, w: f64 }, Dot");
    let err = cargo_goose::verify::verify(&goose, &lib, &broken).unwrap_err();
    assert!(err.contains("AShape::Rect.w"), "{err}");
}

#[test]
fn metadata_versions_preserve_the_strict_contract() {
    let mut json: serde_json::Value = serde_json::from_str(&libloading_json()).unwrap();
    json["format"] = 1.into();
    assert!(meta::parse(&json.to_string(), "legacy").unwrap_err().contains("zero_copy exports require metadata format 2"));
    for f in json["functions"].as_array_mut().unwrap() {
        f.as_object_mut().unwrap().remove("zero_copy");
    }
    let old = meta::parse(&json.to_string(), "legacy").unwrap();
    assert!(old.functions.iter().all(|f| !f.zero_copy));
}
