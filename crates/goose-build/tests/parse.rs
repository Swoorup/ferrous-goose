//! Reading a program's interface from its sources, for a compiler without
//! --emit-interface, and the Rust written from it.

use goose_build::iface::{parse_sources, Field, GTy};

#[test]
fn exports_and_layouts_from_the_sources() {
    let dir = std::env::temp_dir().join(format!("goose-build-test-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(
        dir.join("lib/shapes.goose"),
        "namespace shapes;\nstruct Point { x: f32, y: f32 }\nstruct Tagged { tag: u8, pad, p: f64, q: Point }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("main.goose"),
        r#"import lib.shapes;
// Adds.
export fn add(a: i64, b: i64) -> i64 { return a + b; }
export "c_len" fn len(s: const u8[:]) -> i64 { return s.len; }
export fn move_by(t: shapes::Tagged&, d: const shapes::Point&) { t.q.x += d.x; }
export fn banner() -> const u8[:] { return "hi"; }
fn helper() { }
"#,
    )
    .unwrap();
    let iface = parse_sources(&dir.join("main.goose")).unwrap();
    assert_eq!(iface.exports.len(), 4);
    let len = &iface.exports[1];
    assert_eq!((len.name.as_str(), len.symbol.as_str()), ("len", "c_len"));
    assert!(matches!(&len.params[0].ty, GTy::Slice { constant: true, .. }));
    let tagged = iface.structs.iter().find(|s| s.qname == "shapes::Tagged").expect("reached through a reference");
    // tag at 0, a bare pad aligning p to 8, p at 8, q at 16: 24 bytes.
    assert_eq!(tagged.size, 24);
    let offsets: Vec<usize> = tagged
        .fields
        .iter()
        .map(|f| match f {
            Field::Pad { offset, .. } | Field::Named { offset, .. } => *offset,
        })
        .collect();
    assert_eq!(offsets, vec![0, 1, 8, 16]);
    assert!(iface.structs.iter().any(|s| s.qname == "shapes::Point" && s.size == 8));

    let rust = goose_build::rustgen::generate(&iface, "main").unwrap();
    assert!(rust.contains("pub fn add<'a>(rt: &'a Runtime, a: i64, b: i64) -> i64 {"), "{rust}");
    assert!(rust.contains("pub fn c_len(s: GooseSlice<u8>) -> i64;"));
    assert!(rust.contains("pub fn banner<'a>(rt: &'a Runtime) -> &'a [u8] {"));
    assert!(rust.contains("const _: () = assert!(::core::mem::size_of::<Tagged>() == 24);"));
}
