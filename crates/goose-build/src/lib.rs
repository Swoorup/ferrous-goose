//! Ferrous Goose for build.rs: compiles a Goose program into the crate and
//! writes safe Rust bindings for its export fns.
//!
//! ```ignore
//! // build.rs
//! fn main() {
//!     goose_build::Program::new("goose/kernels.goose").compile().unwrap();
//! }
//!
//! // src/main.rs
//! goose::include!(kernels);
//! fn main() {
//!     let rt = goose::Runtime::init().unwrap();
//!     println!("{} words", kernels::count_words(&rt, b"no heap, no gc"));
//! }
//! ```
//!
//! The compiler writes the program's C (`--standalone`, runtime included)
//! and its interface; the cc crate compiles the C; the interface becomes
//! `$OUT_DIR/<name>.rs`, which `goose::include!(<name>)` brings in. A
//! compiler with `--emit-interface` (goose-patches/0002) describes the
//! interface itself, layouts included; without it, the sources are read for
//! the shapes a stock export takes.

pub mod iface;
pub mod rustgen;

use std::path::{Path, PathBuf};
use std::process::Command;

/// A Goose program to build into the crate.
pub struct Program {
    root: PathBuf,
    name: String,
    goose: Option<PathBuf>,
    defines: Vec<String>,
    opt: String,
}

/// What a compile made.
#[derive(Debug)]
pub struct Built {
    pub c_file: PathBuf,
    pub rust_file: PathBuf,
    pub interface: iface::Interface,
    /// The native-interface capabilities the compiler advertises.
    pub features: Vec<String>,
}

impl Program {
    /// The program whose root file is `root`, relative to the crate. Its
    /// module is named for the file: `goose/kernels.goose` is `kernels`.
    pub fn new(root: impl AsRef<Path>) -> Program {
        let root = root.as_ref().to_path_buf();
        let name = root.file_stem().and_then(|s| s.to_str()).unwrap_or("goose").to_string();
        Program { root, name, goose: None, defines: Vec::new(), opt: "-O2".into() }
    }

    /// Another name for the module.
    pub fn name(mut self, name: &str) -> Program {
        self.name = name.to_string();
        self
    }

    /// The Goose compiler to use (default: $GOOSE, a `.toolchain/goose/build/goose`
    /// above the crate, `goose` on PATH).
    pub fn goose(mut self, path: impl AsRef<Path>) -> Program {
        self.goose = Some(path.as_ref().to_path_buf());
        self
    }

    /// A `-D NAME=VALUE` for the Goose program's constants.
    pub fn define(mut self, d: &str) -> Program {
        self.defines.push(d.to_string());
        self
    }

    /// Goose's inlining level: 0, 1 or 2 (the default).
    pub fn opt_level(mut self, level: u8) -> Program {
        self.opt = format!("-O{}", level.min(2));
        self
    }

    pub fn compile(self) -> Result<Built, String> {
        let out_dir = PathBuf::from(std::env::var("OUT_DIR").map_err(|_| "goose_build runs from build.rs (no OUT_DIR)")?);
        let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into()));
        let root = if self.root.is_absolute() { self.root.clone() } else { manifest_dir.join(&self.root) };
        let goose = match &self.goose {
            Some(g) => g.clone(),
            None => find_goose(&manifest_dir)?,
        };
        println!("cargo:rerun-if-env-changed=GOOSE");
        println!("cargo:rerun-if-changed={}", goose.display());
        let features = goose_features(&goose)?;
        let c_file = out_dir.join(format!("{}.c", self.name));
        let h_file = out_dir.join(format!("{}.h", self.name));
        let j_file = out_dir.join(format!("{}.json", self.name));
        let emit = features.iter().any(|f| f == "emit-interface");
        let mut cmd = Command::new(&goose);
        cmd.arg(&self.opt).arg("-o").arg(&c_file).arg("--standalone").arg("--header").arg(&h_file);
        if emit {
            cmd.arg("--emit-interface").arg(&j_file);
        }
        for d in &self.defines {
            cmd.arg("-D").arg(d);
        }
        cmd.arg(&root).env("GOOSE_PROGRESS", "plain");
        let out = cmd.output().map_err(|e| format!("cannot run {}: {e}", goose.display()))?;
        if !out.status.success() {
            return Err(format!(
                "goose could not compile {}:\n{}{}",
                root.display(),
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        let interface = if emit {
            let text = std::fs::read_to_string(&j_file).map_err(|e| format!("{}: {e}", j_file.display()))?;
            iface::parse_json(&text)?
        } else {
            iface::parse_sources(&root)?
        };
        for s in &interface.sources {
            let p = Path::new(s);
            let p = if p.is_absolute() { p.to_path_buf() } else { root.parent().unwrap_or(Path::new(".")).join(p.file_name().unwrap_or_default()) };
            println!("cargo:rerun-if-changed={}", if p.exists() { p.display().to_string() } else { s.clone() });
        }
        println!("cargo:rerun-if-changed={}", root.display());

        let mut build = cc::Build::new();
        build.file(&c_file).warnings(false).opt_level(2).flag_if_supported("-w");
        build.compile(&format!("goose_{}", self.name));
        let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
        if target == "linux" {
            println!("cargo:rustc-link-lib=m");
            println!("cargo:rustc-link-lib=pthread");
            println!("cargo:rustc-link-lib=dl");
        }

        let mut rust = rustgen::generate(&interface, &self.name)?;
        rust.push_str(rustgen::PRELUDE);
        let rust_file = out_dir.join(format!("{}.rs", self.name));
        std::fs::write(&rust_file, rust).map_err(|e| format!("{}: {e}", rust_file.display()))?;
        if features.iter().any(|f| f == "export-results") {
            println!("cargo:rustc-cfg=goose_export_results");
        }
        println!("cargo::rustc-check-cfg=cfg(goose_export_results)");
        Ok(Built { c_file, rust_file, interface, features })
    }
}

/// The Goose compiler: $GOOSE, `.toolchain/goose/build/goose` in `start` or
/// a directory above it, or `goose` on PATH.
pub fn find_goose(start: &Path) -> Result<PathBuf, String> {
    if let Some(g) = std::env::var_os("GOOSE") {
        return Ok(PathBuf::from(g));
    }
    let exe = if cfg!(windows) { "goose.exe" } else { "goose" };
    let mut dir = start.to_path_buf();
    loop {
        let c = dir.join(".toolchain").join("goose").join("build").join(exe);
        if c.is_file() {
            return Ok(c);
        }
        if !dir.pop() {
            break;
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            let c = d.join(exe);
            if c.is_file() {
                return Ok(c);
            }
        }
    }
    Err("cannot find the Goose compiler: set GOOSE or put goose on PATH".into())
}

/// The native-interface capabilities listed in `goose version`.
pub fn goose_features(goose: &Path) -> Result<Vec<String>, String> {
    let out = Command::new(goose).arg("version").output().map_err(|e| format!("cannot run {}: {e}", goose.display()))?;
    if !out.status.success() {
        return Err(format!("{} version failed: {}", goose.display(), String::from_utf8_lossy(&out.stderr)));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text
        .lines()
        .find_map(|l| l.strip_prefix("features:"))
        .map(|f| f.split_whitespace().map(String::from).collect())
        .unwrap_or_default())
}
