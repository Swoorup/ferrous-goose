//! Editing a program's sidecar: the dylibs entry cargo goose maps.

use cargo_goose::sidecar::set_dylib;

fn temp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("cargo-goose-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join("prog.build")
}

#[test]
fn creates_then_updates_keeping_the_rest() {
    let p = temp("sidecar");
    let _ = std::fs::remove_file(&p);
    assert!(set_dylib(&p, "macos", "goose_rx", "/a/libgoose_rx.dylib").unwrap());
    let t = std::fs::read_to_string(&p).unwrap();
    assert!(t.contains("dylibs_macos = [\"goose_rx=/a/libgoose_rx.dylib\"];"), "{t}");

    std::fs::write(&p, "// mine\nc_libs = [\"m\"];\ndylibs_macos = [\"other=x.dylib\", \"goose_rx=/old\"];\n").unwrap();
    assert!(set_dylib(&p, "macos", "goose_rx", "/new/lib.dylib").unwrap());
    let t = std::fs::read_to_string(&p).unwrap();
    assert!(t.starts_with("// mine\nc_libs = [\"m\"];\n"), "{t}");
    assert!(t.contains("dylibs_macos = [\"other=x.dylib\", \"goose_rx=/new/lib.dylib\"];"), "{t}");
    assert!(!set_dylib(&p, "macos", "goose_rx", "/new/lib.dylib").unwrap(), "unchanged the second time");
    assert!(set_dylib(&p, "linux", "goose_rx", "/l.so").unwrap());
    assert!(std::fs::read_to_string(&p).unwrap().contains("dylibs_linux = [\"goose_rx=/l.so\"];"));
}

#[test]
fn a_multi_line_list_is_left_to_the_user() {
    let p = temp("multiline");
    std::fs::write(&p, "dylibs_macos = [\n    \"a=b\",\n];\n").unwrap();
    assert!(set_dylib(&p, "macos", "goose_rx", "/x").is_err());
}
