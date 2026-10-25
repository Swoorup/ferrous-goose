//! Finding the Goose compiler and what it can do, building a crate with
//! Cargo, and where Cargo puts what it builds.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The Goose compiler: `--goose`, `$GOOSE`, a `.toolchain/goose/build/goose`
/// in this directory or above it (scripts/toolchain.sh builds one), or
/// `goose` on PATH.
pub fn find_goose(explicit: Option<&str>) -> Result<PathBuf, String> {
    if let Some(p) = explicit {
        return Ok(PathBuf::from(p));
    }
    if let Some(p) = std::env::var_os("GOOSE") {
        return Ok(PathBuf::from(p));
    }
    let exe = if cfg!(windows) { "goose.exe" } else { "goose" };
    if let Ok(mut dir) = std::env::current_dir() {
        loop {
            let c = dir.join(".toolchain").join("goose").join("build").join(exe);
            if c.is_file() {
                return Ok(c);
            }
            if !dir.pop() {
                break;
            }
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
    Err("cannot find the Goose compiler: pass --goose, set GOOSE, or put goose on PATH (scripts/toolchain.sh builds one)".into())
}

/// What `goose version` says.
#[derive(Clone, Debug, Default)]
pub struct GooseInfo {
    pub runtime: String,
    /// Native-interface capabilities advertised by the compiler.
    pub features: Vec<String>,
    /// Version of the public native value layouts; independent of features.
    pub native_abi: Option<u32>,
}

impl GooseInfo {
    pub fn has(&self, f: &str) -> bool {
        self.features.iter().any(|x| x == f)
    }
}

pub fn goose_info(goose: &Path) -> Result<GooseInfo, String> {
    let out = Command::new(goose)
        .arg("version")
        .output()
        .map_err(|e| format!("cannot run {}: {e}", goose.display()))?;
    if !out.status.success() {
        return Err(format!("{} version failed: {}", goose.display(), String::from_utf8_lossy(&out.stderr)));
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    parse_goose_info(&text).map_err(|e| format!("{} version: {e}", goose.display()))
}

/// Parse the neutral compiler capability report. Stock compilers advertise no features.
pub fn parse_goose_info(text: &str) -> Result<GooseInfo, String> {
    let mut info = GooseInfo::default();
    for line in text.lines() {
        if let Some(r) = line.trim().strip_prefix("goose runtime ") {
            info.runtime = r.trim().to_string();
        }
        if let Some(f) = line.trim().strip_prefix("features:") {
            info.features = f.split_whitespace().map(|s| s.to_string()).collect();
        }
        if let Some(abi) = line.trim().strip_prefix("native-abi:") {
            info.native_abi = Some(abi.trim().parse().map_err(|_| "invalid native ABI version")?);
        }
    }
    if info.runtime.is_empty() {
        return Err("printed no runtime: is it the Goose compiler?".into());
    }
    Ok(info)
}

/// The Goose runtimes whose value layouts (slices, builders, stacks) bridge
/// ABI 1 was checked against: the pinned commit, stock and patched.
pub const KNOWN_RUNTIMES: &[&str] = &["a5dbd11b6309"];

/// Refuses a compiler whose runtime this bridge was not checked against,
/// naming both, unless `allow` (a newer compiler that kept the layouts).
pub fn check_runtime(info: &GooseInfo, lib_runtime: &str, lib_desc: &str, allow: bool) -> Result<(), String> {
    // Capabilities alone do not establish layout compatibility. An explicit
    // native ABI version does; older compilers require a known runtime hash.
    let known = match info.native_abi {
        Some(abi) => abi == 1,
        None => KNOWN_RUNTIMES.contains(&info.runtime.as_str()) || info.runtime == lib_runtime,
    };
    if known || allow {
        return Ok(());
    }
    Err(format!(
        "{lib_desc} was built for the Goose runtime {lib_runtime}, and this goose's runtime is {}: \
         the bridge has not been checked against it. Pass --allow-runtime {} once you have run the conformance suite with it.",
        info.runtime, info.runtime
    ))
}

/// A crate, as `cargo metadata` describes it.
#[derive(Clone, Debug)]
pub struct CrateInfo {
    pub name: String,
    pub version: String,
    pub lib_name: String,
    pub manifest: PathBuf,
    pub dir: PathBuf,
    pub target_dir: PathBuf,
    pub crate_types: Vec<String>,
    pub namespace: Option<String>,
}

pub fn crate_info(manifest: Option<&Path>) -> Result<CrateInfo, String> {
    let mut cmd = Command::new(cargo());
    cmd.args(["metadata", "--format-version", "1", "--no-deps"]);
    if let Some(m) = manifest {
        cmd.arg("--manifest-path").arg(m);
    }
    let out = cmd.output().map_err(|e| format!("cannot run cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!("cargo metadata failed:\n{}", String::from_utf8_lossy(&out.stderr)));
    }
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata printed bad JSON: {e}"))?;
    let target_dir = PathBuf::from(v["target_directory"].as_str().unwrap_or("target"));
    // The package whose manifest was named, else the one in this directory.
    let want = match manifest {
        Some(m) => std::fs::canonicalize(m).ok(),
        None => std::env::current_dir().ok().map(|d| d.join("Cargo.toml")).and_then(|p| std::fs::canonicalize(p).ok()),
    };
    let pkgs = v["packages"].as_array().cloned().unwrap_or_default();
    let pkg = pkgs
        .iter()
        .find(|p| {
            let mp = p["manifest_path"].as_str().map(PathBuf::from).and_then(|p| std::fs::canonicalize(p).ok());
            want.is_some() && mp == want
        })
        .or_else(|| if pkgs.len() == 1 { pkgs.first() } else { None })
        .ok_or("cannot tell which package to use: pass --manifest-path to the crate's Cargo.toml")?;
    let lib = pkg["targets"]
        .as_array()
        .and_then(|ts| ts.iter().find(|t| t["kind"].as_array().map(|k| k.iter().any(|k| k == "lib" || k == "cdylib" || k == "staticlib" || k == "rlib")).unwrap_or(false)))
        .ok_or("the package has no library target")?;
    let manifest = PathBuf::from(pkg["manifest_path"].as_str().unwrap_or_default());
    Ok(CrateInfo {
        name: pkg["name"].as_str().unwrap_or_default().to_string(),
        version: pkg["version"].as_str().unwrap_or_default().to_string(),
        lib_name: lib["name"].as_str().unwrap_or_default().replace('-', "_"),
        dir: manifest.parent().map(Path::to_path_buf).unwrap_or_default(),
        manifest,
        target_dir,
        crate_types: lib["crate_types"]
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default(),
        namespace: pkg["metadata"]["goose"]["namespace"].as_str().map(String::from),
    })
}

pub fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".into())
}

/// How to build.
#[derive(Clone, Debug, Default)]
pub struct BuildOpts {
    pub release: bool,
    pub features: Vec<String>,
    pub target: Option<String>,
    pub target_dir: Option<PathBuf>,
    /// Build with `cargo zigbuild` (cross targets).
    pub zig: bool,
}

pub fn cargo_build(c: &CrateInfo, o: &BuildOpts) -> Result<(), String> {
    let mut cmd = Command::new(cargo());
    if o.zig {
        cmd.arg("zigbuild");
    } else {
        cmd.arg("build");
    }
    cmd.arg("--lib").arg("--manifest-path").arg(&c.manifest);
    if o.release {
        cmd.arg("--release");
    }
    if !o.features.is_empty() {
        cmd.arg("--features").arg(o.features.join(","));
    }
    if let Some(t) = &o.target {
        cmd.arg("--target").arg(t);
    }
    if let Some(d) = &o.target_dir {
        cmd.arg("--target-dir").arg(d);
    }
    let st = cmd.status().map_err(|e| format!("cannot run cargo: {e}"))?;
    if !st.success() {
        return Err(format!("cargo build of {} failed", c.name));
    }
    Ok(())
}

/// The platform a target triple (or the host) is, in sidecar terms.
pub fn platform_of(target: Option<&str>) -> &'static str {
    match target {
        Some(t) if t.contains("windows") => "windows",
        Some(t) if t.contains("apple") => "macos",
        Some(_) => "linux",
        None => {
            if cfg!(windows) {
                "windows"
            } else if cfg!(target_os = "macos") {
                "macos"
            } else {
                "linux"
            }
        }
    }
}

pub fn host_triple() -> String {
    let out = Command::new("rustc").arg("-vV").output().ok();
    if let Some(o) = out {
        for l in String::from_utf8_lossy(&o.stdout).lines() {
            if let Some(h) = l.strip_prefix("host: ") {
                return h.trim().to_string();
            }
        }
    }
    String::new()
}

pub fn out_dir(c: &CrateInfo, o: &BuildOpts) -> PathBuf {
    let mut d = o.target_dir.clone().unwrap_or_else(|| c.target_dir.clone());
    if let Some(t) = &o.target {
        d.push(t);
    }
    d.push(if o.release { "release" } else { "debug" });
    d
}

/// The shared library Cargo built.
pub fn cdylib_path(c: &CrateInfo, o: &BuildOpts) -> PathBuf {
    let n = &c.lib_name;
    let file = match platform_of(o.target.as_deref()) {
        "windows" => format!("{n}.dll"),
        "macos" => format!("lib{n}.dylib"),
        _ => format!("lib{n}.so"),
    };
    out_dir(c, o).join(file)
}

/// The static library Cargo built.
pub fn staticlib_path(c: &CrateInfo, o: &BuildOpts) -> PathBuf {
    let n = &c.lib_name;
    let file = if platform_of(o.target.as_deref()) == "windows" && o.target.as_deref().map(|t| t.contains("msvc")).unwrap_or(cfg!(target_env = "msvc")) {
        format!("{n}.lib")
    } else {
        format!("lib{n}.a")
    };
    out_dir(c, o).join(file)
}

/// The system libraries a Rust staticlib needs on the link line, as rustc
/// reports them where it can, else the usual ones.
pub fn native_static_libs(c: &CrateInfo, o: &BuildOpts) -> Vec<String> {
    let cache = out_dir(c, o).join(format!("{}-native-static-libs.txt", c.lib_name));
    let mut cmd = Command::new(cargo());
    cmd.args(["rustc", "--lib", "--crate-type", "staticlib"]).arg("--manifest-path").arg(&c.manifest);
    if o.release {
        cmd.arg("--release");
    }
    if !o.features.is_empty() {
        cmd.arg("--features").arg(o.features.join(","));
    }
    if let Some(t) = &o.target {
        cmd.arg("--target").arg(t);
    }
    if let Some(d) = &o.target_dir {
        cmd.arg("--target-dir").arg(d);
    }
    cmd.args(["--", "--print", "native-static-libs"]);
    if let Ok(out) = cmd.output() {
        for l in String::from_utf8_lossy(&out.stderr).lines() {
            if let Some(i) = l.find("native-static-libs:") {
                let libs = l[i + "native-static-libs:".len()..].trim().to_string();
                let _ = std::fs::write(&cache, &libs);
                return libs.split_whitespace().map(String::from).collect();
            }
        }
    }
    if let Ok(s) = std::fs::read_to_string(&cache) {
        return s.split_whitespace().map(String::from).collect();
    }
    match platform_of(o.target.as_deref()) {
        "macos" => ["-liconv", "-lSystem", "-lc", "-lm"].map(String::from).to_vec(),
        "windows" => ["kernel32.lib", "advapi32.lib", "ntdll.lib", "userenv.lib", "ws2_32.lib", "dbghelp.lib", "bcrypt.lib"]
            .map(String::from)
            .to_vec(),
        _ => ["-lgcc_s", "-lutil", "-lrt", "-lpthread", "-lm", "-ldl", "-lc"].map(String::from).to_vec(),
    }
}

/// The environment variable the platform's loader searches for a bare
/// library name.
pub fn loader_path_var() -> &'static str {
    if cfg!(windows) {
        "PATH"
    } else if cfg!(target_os = "macos") {
        "DYLD_LIBRARY_PATH"
    } else {
        "LD_LIBRARY_PATH"
    }
}

/// `dir` prepended to the loader's search path.
pub fn loader_path_with(dir: &Path) -> std::ffi::OsString {
    let var = loader_path_var();
    let mut paths = vec![dir.to_path_buf()];
    if let Some(old) = std::env::var_os(var) {
        paths.extend(std::env::split_paths(&old));
    }
    std::env::join_paths(paths).unwrap_or_default()
}
