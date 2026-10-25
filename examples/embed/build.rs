fn main() {
    // Compiles goose/kernels.goose (and what it imports) into one program,
    // builds its C with the cc crate, and writes typed Rust bindings to
    // OUT_DIR.
    goose_build::Program::new("goose/kernels.goose").compile().unwrap();
}
