//! What the macros refuse, and the messages they refuse it with.
//! TRYBUILD=overwrite cargo test -p ferrous-goose --test ui  writes the .stderr files.

#[test]
fn rejections() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
