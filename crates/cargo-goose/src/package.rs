//! `cargo goose package`: a Goose package over a crate, holding its
//! generated module and a prebuilt library for every platform it was built
//! for, so a program using it needs no Rust toolchain.
//!
//! ```text
//! <out>/
//!   rx.goose            the module (namespace rx)
//!   rx.build            dylibs_<platform> = ["goose_regex=./lib/<platform>/..."]
//!   lib/macos/libgoose_regex.dylib   (universal when both macOS targets are built)
//!   lib/linux/libgoose_regex.so
//!   lib/windows/goose_regex.dll
//!   ferrous.json        versions, for checking a package against a compiler
//!   examples/           programs using it, `import rx from "..";`
//!   README.md
//! ```
//!
//! A `./` spelling in a package's sidecar is relative to the package's own
//! directory with goose-patches/0005; a stock compiler reads it relative to
//! the program instead.

use crate::{gen, meta, toolchain, write_if_changed};
use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Plan {
    pub manifest: Option<PathBuf>,
    pub out: PathBuf,
    /// Target triples; the host when empty.
    pub targets: Vec<String>,
    pub features: Vec<String>,
    /// Cross targets through `cargo zigbuild`.
    pub zig: bool,
    pub goose: PathBuf,
    pub info: toolchain::GooseInfo,
    pub allow_runtime: Option<String>,
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(d) = to.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("cannot create {}: {e}", d.display()))?;
    }
    std::fs::copy(from, to).map_err(|e| format!("cannot copy {} to {}: {e}", from.display(), to.display()))?;
    Ok(())
}

pub fn package(p: &Plan) -> Result<(), String> {
    let ci = toolchain::crate_info(p.manifest.as_deref())?;
    let host = toolchain::host_triple();
    let targets: Vec<String> = if p.targets.is_empty() { vec![host.clone()] } else { p.targets.clone() };
    // Read the host records first: cross-target layout claims cannot be made
    // from them for strict exports (target cfg can change fields and alignment).
    let host_build = toolchain::BuildOpts { release: true, features: p.features.clone(), ..Default::default() };
    toolchain::cargo_build(&ci, &host_build)?;
    let host_lib = toolchain::cdylib_path(&ci, &host_build);
    let library = meta::read_library(&host_lib)?;
    let desc = format!("{} {}", library.crate_name, library.version);
    let allow = p.allow_runtime.as_deref() == Some(p.info.runtime.as_str());
    toolchain::check_runtime(&p.info, &library.goose_runtime, &desc, allow)?;
    let ns = library.namespace.clone();
    // Portable packages omit typed-builder/aligned-enum/reentrant exports;
    // ordinary vector results keep the existing count/copy protocol.
    let opts = gen::Options { link: gen::Link::Dynamic(ci.lib_name.clone()), typed_builders: false, align: false, reentrant: false, emit_interface: p.info.has("emit-interface") };
    let module = gen::generate(&library, &opts)?;
    let strict = library.functions.iter().any(|f| f.zero_copy && module.contains(&format!("// zero_copy: {}\n", f.symbol)));
    if strict && targets.iter().any(|t| *t != host) {
        return Err(format!("zero_copy package exports require target layout verification; only the host target {host} is supported with these metadata records"));
    }
    if strict { crate::verify::verify(&p.goose, &library, &module)?; }
    let mut built: BTreeMap<&'static str, Vec<(String, PathBuf)>> = BTreeMap::new();
    for t in &targets {
        let lib = if *t == host { host_lib.clone() } else {
            let bo = toolchain::BuildOpts {
                release: true, features: p.features.clone(), target: Some(t.clone()),
                target_dir: None, zig: p.zig && !t.contains("apple"),
            };
            toolchain::cargo_build(&ci, &bo)?;
            toolchain::cdylib_path(&ci, &bo)
        };
        if !lib.is_file() { return Err(format!("cargo built no {}", lib.display())); }
        built.entry(toolchain::platform_of(Some(t))).or_default().push((t.clone(), lib));
    }
    crate::write_module(&p.out.join(format!("{ns}.goose")), &module)?;

    let mut sidecar = String::new();
    let _ = writeln!(sidecar, "// Prebuilt libraries of {} {}, one per platform (cargo goose package).", ci.name, ci.version);
    let _ = writeln!(sidecar, "// A ./ path is the package's own directory (goose-patches/0005).");
    let mut platforms = Vec::new();
    for (platform, libs) in &built {
        let file = match *platform {
            "windows" => format!("{}.dll", ci.lib_name),
            "macos" => format!("lib{}.dylib", ci.lib_name),
            _ => format!("lib{}.so", ci.lib_name),
        };
        let arch_of = |t: &str| if t.starts_with("aarch64") { "arm64" } else { "x86_64" };
        if *platform == "macos" || libs.len() == 1 {
            // One file: macOS's universal library holds every architecture.
            let dest = p.out.join("lib").join(platform).join(&file);
            if libs.len() > 1 {
                if let Some(d) = dest.parent() {
                    std::fs::create_dir_all(d).map_err(|e| format!("cannot create {}: {e}", d.display()))?;
                }
                let mut cmd = Command::new("lipo");
                cmd.arg("-create").arg("-output").arg(&dest);
                for (_, l) in libs {
                    cmd.arg(l);
                }
                let st = cmd.status().map_err(|e| format!("cannot run lipo: {e}"))?;
                if !st.success() {
                    return Err("lipo could not merge the macOS libraries".into());
                }
            } else {
                copy(&libs[0].1, &dest)?;
            }
            let _ = writeln!(sidecar, "dylibs_{platform} = [\"{}=./lib/{platform}/{file}\"];", ci.lib_name);
            platforms.push(serde_json::json!({
                "platform": platform,
                "targets": libs.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>(),
                "file": format!("lib/{platform}/{file}"),
            }));
            continue;
        }
        // One file per architecture, each under its own key; the x86_64 one
        // is the platform's default.
        let mut default = None;
        for (t, l) in libs {
            let arch = arch_of(t);
            let dir = format!("{platform}-{arch}");
            copy(l, &p.out.join("lib").join(&dir).join(&file))?;
            let _ = writeln!(sidecar, "dylibs_{platform}_{arch} = [\"{}=./lib/{dir}/{file}\"];", ci.lib_name);
            if arch == "x86_64" || default.is_none() {
                default = Some(dir.clone());
            }
            platforms.push(serde_json::json!({ "platform": platform, "targets": [t], "file": format!("lib/{dir}/{file}") }));
        }
        if let Some(dir) = default {
            let _ = writeln!(sidecar, "dylibs_{platform} = [\"{}=./lib/{dir}/{file}\"];", ci.lib_name);
        }
    }
    write_if_changed(&p.out.join(format!("{ns}.build")), &sidecar)?;
    let manifest = serde_json::json!({
        "crate": library.crate_name,
        "version": library.version,
        "namespace": ns,
        "library": ci.lib_name,
        "bridge": library.bridge,
        "metadata_format": library.format,
        "abi": library.abi,
        "goose_runtime": library.goose_runtime,
        "platforms": platforms,
    });
    write_if_changed(&p.out.join("ferrous.json"), &(serde_json::to_string_pretty(&manifest).unwrap() + "\n"))?;

    // Examples: goose/examples/ in the crate or beside it, importing the
    // package by path.
    let ex = [ci.dir.join("goose").join("examples"), ci.dir.join("..").join("goose").join("examples")]
        .into_iter()
        .find(|d| d.is_dir())
        .unwrap_or_default();
    if ex.is_dir() {
        for e in std::fs::read_dir(&ex).map_err(|e| e.to_string())?.flatten() {
            let path = e.path();
            if path.extension().map(|x| x == "goose" || x == "expected").unwrap_or(false) {
                copy(&path, &p.out.join("examples").join(path.file_name().unwrap()))?;
            }
        }
    }
    write_if_changed(&p.out.join("README.md"), &readme(&library, &ci, &built.keys().copied().collect::<Vec<_>>()))?;
    eprintln!("packaged {} {} into {} ({})", ci.name, ci.version, p.out.display(), built.keys().copied().collect::<Vec<_>>().join(", "));
    Ok(())
}

fn readme(lib: &meta::Library, ci: &toolchain::CrateInfo, platforms: &[&str]) -> String {
    let ns = &lib.namespace;
    let mut r = String::new();
    let _ = writeln!(r, "# {ns}: {} for Goose\n", lib.crate_name);
    let _ = writeln!(
        r,
        "Generated by `cargo goose package` from {} {} with Ferrous Goose {}. The Rust is\nprebuilt for {}; a Goose program needs no Rust toolchain:\n",
        ci.name,
        ci.version,
        lib.bridge,
        platforms.join(", ")
    );
    let _ = writeln!(r, "```goose\nimport {ns} from \"github.com/<owner>/<repo>@<tag>\";\n```\n");
    let _ = writeln!(r, "## Functions\n");
    for f in &lib.functions {
        let doc = f.doc.lines().next().unwrap_or("");
        let _ = writeln!(r, "- `{ns}::{}`: {doc}", f.name);
    }
    if !lib.handles.is_empty() {
        let _ = writeln!(r, "\n## Handles\n");
        for h in &lib.handles {
            let _ = writeln!(r, "- `{ns}::{}`: release with `{ns}::release`. {}", h.name, h.doc.lines().next().unwrap_or(""));
        }
    }
    r
}
