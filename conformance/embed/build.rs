fn main() {
    // The full program where the compiler takes several results and
    // builders at the export boundary (goose-patches/0001), else the basic.
    let dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let goose = goose_build::find_goose(&dir).unwrap();
    let features = goose_build::goose_features(&goose).unwrap();
    let full = features.iter().any(|f| f == "export-results") && features.iter().any(|f| f == "align");
    let root = if full { "goose/full.goose" } else { "goose/basic.goose" };
    println!("cargo:rerun-if-changed=goose/basic.goose");
    println!("cargo:rerun-if-changed=goose/full.goose");
    println!("cargo::rustc-check-cfg=cfg(goose_full)");
    if full {
        println!("cargo:rustc-cfg=goose_full");
    }
    goose_build::Program::new(root).name("embed").goose(goose).compile().unwrap();
}
