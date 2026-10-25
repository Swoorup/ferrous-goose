//! `cargo goose <command>`; see `cargo goose help`.

use cargo_goose::{gen, package, scaffold, sidecar, toolchain, write_if_changed, write_module};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const HELP: &str = "\
cargo goose: Goose bindings for Rust crates that use #[goose::export]

usage: cargo goose <command> [options]

commands:
  bindings            build the crate and write its Goose module
  build <file.goose>  bindings beside the program, the sidecar mapped, then goose build
  run <file.goose>    bindings beside the program, then goose run [-- args]
  test                run the crate's Goose test programs (goose-tests/*.goose)
  new <path>          a crate with the attributes in place and a Goose example
  package <dir>       a Goose package: the module and prebuilt libraries
  native              native-build provider: build a crate and write its module
  combine             native-build combiner: one static library for several crates
  help                this text

options:
  --manifest-path P   the crate's Cargo.toml (default: the one here)
  --lib FILE          read the records from this built library, building nothing
  -o, --out FILE      where the module goes (bindings: default <namespace>.goose)
  --debug             a debug build (default: release)
  --features F,G      Cargo features
  --static            plain extern fns, for a program the library is linked into
  --typed-builders, --no-typed-builders
                      whether the compiler takes T[>..]& (default: ask goose version)
  --goose PATH        the Goose compiler (default: $GOOSE, .toolchain/, PATH)
  --allow-runtime H   accept a Goose runtime the bridge was not checked against
  --no-verify         skip comparing the module's layouts with the compiler's
                      (done where goose has --emit-interface)
";

#[derive(Default)]
struct Args {
    cmd: String,
    positional: Vec<String>,
    rest: Vec<String>,
    manifest: Option<PathBuf>,
    lib: Option<PathBuf>,
    out: Option<PathBuf>,
    debug: bool,
    features: Vec<String>,
    static_link: bool,
    typed: Option<bool>,
    goose: Option<String>,
    allow_runtime: Option<String>,
    namespace: Option<String>,
    targets: Vec<String>,
    target_dir: Option<PathBuf>,
    cache_dir: Option<PathBuf>,
    goose_path: Option<PathBuf>,
    zig: bool,
    no_verify: bool,
    crates: Vec<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1).peekable();
    // `cargo goose ...` runs `cargo-goose goose ...`.
    if it.peek().map(|s| s == "goose").unwrap_or(false) {
        it.next();
    }
    let mut a = Args { cmd: it.next().unwrap_or_else(|| "help".into()), ..Default::default() };
    while let Some(x) = it.next() {
        let mut val = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        match x.as_str() {
            "--" => {
                a.rest = it.by_ref().collect();
                break;
            }
            "--manifest-path" => a.manifest = Some(PathBuf::from(val("--manifest-path")?)),
            "--lib" => a.lib = Some(PathBuf::from(val("--lib")?)),
            "-o" | "--out" | "--out-dir" => a.out = Some(PathBuf::from(val("--out")?)),
            "--debug" => a.debug = true,
            "--release" => a.debug = false,
            "--features" => a.features.extend(val("--features")?.split(',').map(String::from)),
            "--static" => a.static_link = true,
            "--typed-builders" => a.typed = Some(true),
            "--no-typed-builders" => a.typed = Some(false),
            "--goose" | "--compiler" => a.goose = Some(val("--goose")?),
            "--allow-runtime" => a.allow_runtime = Some(val("--allow-runtime")?),
            "--namespace" => a.namespace = Some(val("--namespace")?),
            "--target" | "--targets" => a.targets.extend(val("--target")?.split(',').map(String::from)),
            "--target-dir" => a.target_dir = Some(PathBuf::from(val("--target-dir")?)),
            "--goose-crate-path" => a.goose_path = Some(PathBuf::from(val("--goose-crate-path")?)),
            "--zig" => a.zig = true,
            "--no-verify" => a.no_verify = true,
            "--include-dir" => {
                // The Rust provider uses its own ABI declarations; C providers use this header.
                val("--include-dir")?;
            }
            "--cache-dir" => a.cache_dir = Some(PathBuf::from(val("--cache-dir")?)),
            "--crate" | "--input" => a.crates.push(val("--input")?),
            "-h" | "--help" => a.cmd = "help".into(),
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            _ => a.positional.push(x),
        }
    }
    Ok(a)
}

fn main() -> ExitCode {
    let a = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("cargo goose: {e}");
            return ExitCode::from(2);
        }
    };
    let r = match a.cmd.as_str() {
        "bindings" => cmd_bindings(&a),
        "build" => cmd_build_run(&a, false),
        "run" => cmd_build_run(&a, true),
        "test" => cmd_test(&a),
        "new" => cmd_new(&a),
        "package" => cmd_package(&a),
        "native" => cmd_native(&a),
        "combine" => cmd_combine(&a),
        "help" => {
            print!("{HELP}");
            Ok(0)
        }
        other => Err(format!("unknown command {other}; cargo goose help lists them")),
    };
    match r {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("cargo goose: {e}");
            ExitCode::from(1)
        }
    }
}

fn build_opts(a: &Args) -> toolchain::BuildOpts {
    toolchain::BuildOpts {
        release: !a.debug,
        features: a.features.clone(),
        target: None,
        target_dir: a.target_dir.clone(),
        zig: false,
    }
}

/// The compiler, its capabilities, and the options the module is written with.
fn gen_options(a: &Args, link: gen::Link) -> Result<(PathBuf, toolchain::GooseInfo, gen::Options), String> {
    let goose = toolchain::find_goose(a.goose.as_deref())?;
    let info = toolchain::goose_info(&goose)?;
    let typed = a.typed.unwrap_or_else(|| info.has("typed-builders"));
    let align = info.has("align");
    let reentrant = info.has("reentrant");
    Ok((goose, info, gen::Options { link, typed_builders: typed, align, reentrant }))
}

fn check_versions(a: &Args, info: &toolchain::GooseInfo, b: &cargo_goose::Bindings) -> Result<(), String> {
    let lib = &b.library;
    let desc = format!("{} {} (Ferrous Goose {})", lib.crate_name, lib.version, lib.bridge);
    let allow = a.allow_runtime.as_deref() == Some(info.runtime.as_str());
    toolchain::check_runtime(info, &lib.goose_runtime, &desc, allow)?;
    if info.has("emit-interface") && !a.no_verify {
        let goose = toolchain::find_goose(a.goose.as_deref())?;
        cargo_goose::verify::verify(&goose, lib, &b.module)?;
    }
    Ok(())
}

fn cmd_bindings(a: &Args) -> Result<u8, String> {
    let link = if a.static_link { gen::Link::Static } else { gen::Link::Dynamic(String::new()) };
    let (_, info, opts) = gen_options(a, link)?;
    let b = cargo_goose::bindings(a.manifest.as_deref(), a.lib.as_deref(), &build_opts(a), &opts)?;
    check_versions(a, &info, &b)?;
    let out = a.out.clone().unwrap_or_else(|| PathBuf::from(format!("{}.goose", b.library.namespace)));
    let wrote = write_module(&out, &b.module)?;
    eprintln!(
        "{} {} ({} functions, {} types) from {}",
        if wrote { "wrote" } else { "unchanged:" },
        out.display(),
        b.library.functions.len(),
        b.library.structs.len() + b.library.enums.len() + b.library.handles.len(),
        b.cdylib.display()
    );
    Ok(0)
}

/// Bindings beside a program, then goose build (the sidecar mapped to the
/// built library) or goose run (the library found through the loader's
/// search path).
fn cmd_build_run(a: &Args, run: bool) -> Result<u8, String> {
    let prog = a.positional.first().ok_or("which Goose program? cargo goose run file.goose")?;
    let prog = PathBuf::from(prog);
    let dir = prog.parent().map(Path::to_path_buf).unwrap_or_default();
    let (goose, info, opts) = gen_options(a, gen::Link::Dynamic(String::new()))?;
    let b = cargo_goose::bindings(a.manifest.as_deref(), a.lib.as_deref(), &build_opts(a), &opts)?;
    check_versions(a, &info, &b)?;
    let module = dir.join(format!("{}.goose", b.library.namespace));
    write_module(&module, &b.module)?;
    let libdir = b.cdylib.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut cmd = Command::new(&goose);
    if run {
        cmd.arg("run").arg(&prog);
        if !a.rest.is_empty() {
            cmd.arg("--").args(&a.rest);
        }
        cmd.env(toolchain::loader_path_var(), toolchain::loader_path_with(&libdir));
    } else {
        let stem = prog.file_stem().and_then(|s| s.to_str()).unwrap_or("main");
        let sc = dir.join(format!("{stem}.build"));
        let abs = std::fs::canonicalize(&b.cdylib).unwrap_or(b.cdylib.clone());
        sidecar::set_dylib(&sc, toolchain::platform_of(None), &b.crate_info.lib_name, &abs.to_string_lossy())?;
        cmd.arg("build").arg(&prog).args(&a.rest);
    }
    let st = cmd.status().map_err(|e| format!("cannot run {}: {e}", goose.display()))?;
    Ok(st.code().unwrap_or(1) as u8)
}

/// Runs each Goose program in the crate's goose-tests/ (or the directory
/// given). A test passes when it exits with 0 and, where `<name>.expected`
/// exists, prints exactly that; or, where `<name>.fails` exists, when it
/// exits with an error whose stderr holds every line of it. A first-line
/// comment `// test: build` builds an executable rather than running in the
/// compiler (threads need one); `// test: needs <feature>` skips the test on
/// a compiler without that capability.
fn cmd_test(a: &Args) -> Result<u8, String> {
    let (goose, info, opts) = gen_options(a, gen::Link::Dynamic(String::new()))?;
    let b = cargo_goose::bindings(a.manifest.as_deref(), a.lib.as_deref(), &build_opts(a), &opts)?;
    check_versions(a, &info, &b)?;
    let dir = a.positional.first().map(PathBuf::from).unwrap_or_else(|| b.crate_info.dir.join("goose-tests"));
    let module = dir.join(format!("{}.goose", b.library.namespace));
    write_module(&module, &b.module)?;
    let libdir = b.cdylib.parent().map(Path::to_path_buf).unwrap_or_default();
    let filter = a.positional.get(1).cloned();
    let mut tests: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "goose").unwrap_or(false) && *p != module)
        .filter(|p| filter.as_ref().map(|f| p.to_string_lossy().contains(f.as_str())).unwrap_or(true))
        .collect();
    tests.sort();
    let (mut pass, mut fail, mut skip) = (0, 0, 0);
    let bindir = b.crate_info.target_dir.join("goose-tests");
    let _ = std::fs::create_dir_all(&bindir);
    for t in &tests {
        let name = t.file_name().unwrap().to_string_lossy().into_owned();
        let src = std::fs::read_to_string(t).unwrap_or_default();
        let directives: Vec<&str> = src.lines().take_while(|l| l.starts_with("//")).collect();
        let build = directives.iter().any(|l| l.trim() == "// test: build");
        let envs: Vec<(String, String)> = directives
            .iter()
            .filter_map(|l| l.trim().strip_prefix("// test: env "))
            .filter_map(|kv| kv.split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
            .collect();
        let missing: Vec<&str> = directives
            .iter()
            .filter_map(|l| l.trim().strip_prefix("// test: needs "))
            .flat_map(|f| f.split_whitespace())
            .filter(|f| !info.has(f))
            .collect();
        if !missing.is_empty() {
            skip += 1;
            println!("skip  {name} (needs {})", missing.join(", "));
            continue;
        }
        let out = if build {
            let exe = bindir.join(t.file_stem().unwrap());
            let st = Command::new(&goose)
                .arg("build")
                .arg(t)
                .arg("-o")
                .arg(&exe)
                .env("GOOSE_PROGRESS", "plain")
                .output()
                .map_err(|e| format!("cannot run {}: {e}", goose.display()))?;
            if !st.status.success() {
                fail += 1;
                println!("FAIL  {name} (goose build)\n{}", indent(&String::from_utf8_lossy(&st.stderr)));
                continue;
            }
            Command::new(&exe)
                .env(toolchain::loader_path_var(), toolchain::loader_path_with(&libdir))
                .envs(envs.iter().map(|(k, v)| (k, v)))
                .output()
                .map_err(|e| format!("cannot run {}: {e}", exe.display()))?
        } else {
            Command::new(&goose)
                .arg("run")
                .arg(t)
                .env(toolchain::loader_path_var(), toolchain::loader_path_with(&libdir))
                .env("GOOSE_PROGRESS", "plain")
                .envs(envs.iter().map(|(k, v)| (k, v)))
                .output()
                .map_err(|e| format!("cannot run {}: {e}", goose.display()))?
        };
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let expected = std::fs::read_to_string(t.with_extension("expected")).ok();
        let fails = std::fs::read_to_string(t.with_extension("fails")).ok();
        let want_err = std::fs::read_to_string(t.with_extension("stderr")).ok();
        let stdout_ok = expected.as_ref().map(|e| e.trim_end() == stdout.trim_end()).unwrap_or(true)
            && want_err
                .as_ref()
                .map(|w| w.lines().filter(|l| !l.trim().is_empty()).all(|l| stderr.contains(l.trim())))
                .unwrap_or(true);
        let ok = match &fails {
            Some(f) => {
                !out.status.success()
                    && stdout_ok
                    && f.lines().filter(|l| !l.trim().is_empty()).all(|l| stderr.contains(l.trim()))
            }
            None => out.status.success() && stdout_ok,
        };
        if ok {
            pass += 1;
            println!("ok    {name}");
        } else {
            fail += 1;
            println!("FAIL  {name} (exit {:?})", out.status.code());
            if let Some(e) = &expected {
                if !stdout_ok {
                    println!("  expected:\n{}\n  got:\n{}", indent(e), indent(&stdout));
                }
            } else if !stdout.is_empty() {
                println!("  stdout:\n{}", indent(&stdout));
            }
            let err: Vec<&str> = stderr
                .lines()
                .filter(|l| !l.starts_with("parsed ok") && !l.starts_with("typechecked ok") && !l.starts_with("bce:"))
                .collect();
            if let Some(f) = &fails {
                println!("  expected an error with:\n{}", indent(f));
            }
            if let Some(w) = &want_err {
                println!("  expected stderr with:\n{}", indent(w));
            }
            if !err.is_empty() {
                println!("  stderr:\n{}", indent(&err.join("\n")));
            }
        }
    }
    println!("{pass} passed, {fail} failed, {skip} skipped");
    Ok(if fail == 0 { 0 } else { 1 })
}

fn indent(s: &str) -> String {
    s.lines().map(|l| format!("    {l}")).collect::<Vec<_>>().join("\n")
}

fn cmd_new(a: &Args) -> Result<u8, String> {
    let path = a.positional.first().ok_or("cargo goose new <path>")?;
    let created = scaffold::new_crate(Path::new(path), a.namespace.as_deref(), a.goose_path.as_deref())?;
    for f in created {
        println!("created {}", f.display());
    }
    Ok(0)
}

fn cmd_package(a: &Args) -> Result<u8, String> {
    let out = a.positional.first().map(PathBuf::from).or(a.out.clone()).ok_or("cargo goose package <dir>")?;
    let (goose, info, _) = gen_options(a, gen::Link::Dynamic(String::new()))?;
    let p = package::Plan {
        manifest: a.manifest.clone(),
        out,
        targets: a.targets.clone(),
        features: a.features.clone(),
        zig: a.zig,
        goose: goose.clone(),
        info,
        allow_runtime: a.allow_runtime.clone(),
    };
    package::package(&p)?;
    Ok(0)
}

/// A native-build provider (goose-patches/0005): builds the crate's
/// static and shared libraries, writes the module for linking, and prints
/// what the compiler needs as JSON.
fn cmd_native(a: &Args) -> Result<u8, String> {
    let out_dir = a.out.clone().ok_or("native needs --out-dir")?;
    let (_, info, mut opts) = gen_options(a, gen::Link::Static)?;
    opts.link = gen::Link::Static;
    let mut bo = build_opts(a);
    if bo.target_dir.is_none() {
        bo.target_dir = a.cache_dir.clone();
    }
    let b = cargo_goose::bindings(a.manifest.as_deref(), None, &bo, &opts)?;
    check_versions(a, &info, &b)?;
    let module = out_dir.join(format!("{}.goose", b.library.namespace));
    write_module(&module, &b.module)?;
    let staticlib = toolchain::staticlib_path(&b.crate_info, &bo);
    if !staticlib.is_file() {
        return Err(format!(
            "{} has no staticlib: add \"staticlib\" to its crate-type",
            b.crate_info.manifest.display()
        ));
    }
    let libs = toolchain::native_static_libs(&b.crate_info, &bo);
    let j = serde_json::json!({
        "version": 1,
        "runtime_hooks": "goose_runtime_hooks",
        "combine": [std::env::current_exe().map_err(|e| format!("cannot locate cargo-goose: {e}"))?, "combine"],
        "combine_input": serde_json::json!({"manifest": b.crate_info.manifest, "features": bo.features}).to_string(),
        "namespace": b.library.namespace,
        "module": module,
        "staticlib": staticlib,
        "cdylib": b.cdylib,
        "native_libs": libs,
    });
    println!("goose-native: {j}");
    Ok(0)
}

/// The native-build combiner for Rust libraries (goose-patches/0005): an
/// umbrella crate depending on each (`--crate dir[:feature,...]`), built as
/// one static library, since two Rust static libraries would each define
/// Rust's own symbols. Prints what the compiler needs as JSON.
fn cmd_combine(a: &Args) -> Result<u8, String> {
    let out_dir = a.out.clone().ok_or("combine needs --out-dir")?;
    if a.crates.len() < 2 {
        return Err("combine takes two --crate or more".into());
    }
    let mut deps = String::new();
    let mut uses = String::new();
    for (i, c) in a.crates.iter().enumerate() {
        let (manifest, features) = if c.starts_with('{') {
            let input: serde_json::Value = serde_json::from_str(c).map_err(|e| format!("invalid combine input: {e}"))?;
            let manifest = input["manifest"].as_str().ok_or("combine input needs a manifest")?;
            let features = input["features"].as_array().ok_or("combine input needs a features list")?
                .iter().map(|v| v.as_str().map(String::from).ok_or("combine features must be strings"))
                .collect::<Result<Vec<_>, _>>()?;
            (PathBuf::from(manifest), features)
        } else {
            let (dir, features) = match c.split_once(':') {
                Some((d, f)) if !d.is_empty() && !f.contains('/') && !f.contains('\\') => (d.to_string(), f.to_string()),
                _ => (c.clone(), String::new()),
            };
            (Path::new(&dir).join("Cargo.toml"), features.split(',').filter(|f| !f.is_empty()).map(String::from).collect())
        };
        let ci = toolchain::crate_info(Some(&manifest))?;
        if !ci.crate_types.iter().any(|t| t == "rlib" || t == "lib") {
            return Err(format!("{}: add \"rlib\" to the crate-type, so the program's crates can link as one library", ci.manifest.display()));
        }
        let feats: Vec<String> = features.iter().map(|f| format!("{f:?}")).collect();
        deps.push_str(&format!(
            "c{i} = {{ package = {:?}, path = {:?}, features = [{}] }}\n",
            ci.name,
            ci.dir.display().to_string(),
            feats.join(", ")
        ));
        uses.push_str(&format!("pub extern crate c{i};\n"));
    }
    let umbrella = out_dir.join("umbrella");
    let manifest = format!(
        "# Written by cargo goose combine: the program's crates as one library.\n[package]\nname = \"goose-combined\"\nversion = \"0.0.0\"\nedition = \"2021\"\npublish = false\n\n[lib]\npath = \"lib.rs\"\ncrate-type = [\"staticlib\"]\n\n[dependencies]\n{deps}\n[profile.release]\npanic = \"unwind\"\n\n[workspace]\n"
    );
    write_if_changed(&umbrella.join("Cargo.toml"), &manifest)?;
    write_if_changed(&umbrella.join("lib.rs"), &format!("//! Written by cargo goose combine.\n{uses}"))?;
    let ci = toolchain::crate_info(Some(&umbrella.join("Cargo.toml")))?;
    let bo = toolchain::BuildOpts { release: true, target_dir: Some(out_dir.join("target")), ..Default::default() };
    toolchain::cargo_build(&ci, &bo)?;
    let staticlib = toolchain::staticlib_path(&ci, &bo);
    let libs = toolchain::native_static_libs(&ci, &bo);
    let j = serde_json::json!({ "version": 1, "staticlib": staticlib, "native_libs": libs, "runtime_hooks": "goose_runtime_hooks" });
    println!("goose-native: {j}");
    Ok(0)
}
