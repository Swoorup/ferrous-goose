//! Checking a generated module against what the compiler makes of it: the
//! size of every struct and enum, and every field's offset, as the Goose
//! compiler lays them out (`--emit-interface`, goose-patches/0002) against
//! what rustc said. Catches a declaration that does not mean in Goose what
//! the bridge assumes it does.

use crate::meta::Library;
use std::fmt::Write;
use std::path::Path;
use std::process::Command;

#[derive(serde::Deserialize)]
struct Iface {
    structs: Vec<IStruct>,
    enums: Vec<IEnum>,
}

#[derive(serde::Deserialize)]
struct IStruct {
    name: String,
    size: usize,
    #[serde(default = "one")]
    align: usize,
    fields: Vec<serde_json::Value>,
}

#[derive(serde::Deserialize)]
struct IEnum {
    name: String,
    size: usize,
    #[serde(default = "one")]
    align: usize,
    #[serde(default)]
    tag_size: usize,
    #[serde(default)]
    variants: Vec<IVariant>,
}

#[derive(serde::Deserialize)]
struct IVariant {
    name: String,
    tag: usize,
    fields: Vec<serde_json::Value>,
}

// Remove probe files on success and on compiler/layout errors.
struct ProbeDir(std::path::PathBuf);
impl Drop for ProbeDir {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}

fn one() -> usize { 1 }

fn fields_match(label: &str, expected: &[crate::meta::Field], actual: &[serde_json::Value], strict: bool, problems: &mut Vec<String>) {
    for f in expected {
        let g = actual.iter().find(|x| x["name"] == crate::gen::name(&f.name));
        let offset = g.and_then(|x| x["offset"].as_u64());
        if offset != Some(f.goose_offset as u64) {
            problems.push(format!("{label}.{}: Goose puts it at {:?}, the bridge at {}", f.name, offset, f.goose_offset));
        }
        if strict && g.and_then(|x| x["size"].as_u64()) != Some(f.goose_size as u64) {
            problems.push(format!("{label}.{}: zero_copy field size differs from Rust", f.name));
        }
    }
}

/// Compiles a probe exporting a function per type of the module, asks the
/// compiler for its interface, and compares.
pub fn verify(goose: &Path, lib: &Library, module: &str) -> Result<(), String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("cargo-goose-verify-{}-{}-{id}", lib.namespace, std::process::id()));
    let strict = crate::zero_copy::required_types(lib, module);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let _cleanup = ProbeDir(dir.clone());
    let ns = &lib.namespace;
    std::fs::write(dir.join(format!("{ns}.goose")), module).map_err(|e| e.to_string())?;
    let mut probe = format!("// cargo goose's layout probe for {ns}.\nimport {ns};\n");
    let mut n = 0;
    for s in &lib.structs {
        if module.contains(&format!("\nstruct {} {{", s.name)) {
            let _ = writeln!(probe, "export fn ferrous_probe_{n}(x: const {ns}::{}&) {{}}", s.name);
            n += 1;
        }
    }
    for e in &lib.enums {
        if module.contains(&format!("\nenum {} {{", e.name)) {
            let _ = writeln!(probe, "export fn ferrous_probe_{n}(x: const {ns}::{}&) {{}}", e.name);
            n += 1;
        }
    }
    let has_strict = lib.functions.iter().any(|f| f.zero_copy && module.contains(&format!("// zero_copy: {}\n", f.symbol)));
    if n == 0 && !has_strict {
        return Ok(());
    }
    let pf = dir.join("probe.goose");
    std::fs::write(&pf, probe).map_err(|e| e.to_string())?;
    let out = Command::new(goose)
        .args(["check", "--emit-interface", "json"])
        .arg(&pf)
        .env("GOOSE_PROGRESS", "plain")
        .output()
        .map_err(|e| format!("cannot run {}: {e}", goose.display()))?;
    if !out.status.success() {
        return Err(format!(
            "the generated module does not compile:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let start = text.find('{').ok_or("the compiler printed no interface")?;
    let iface: Iface = serde_json::from_str(&text[start..]).map_err(|e| format!("the compiler's interface: {e}"))?;
    let mut problems = Vec::new();
    for s in &lib.structs {
        let Some(g) = iface.structs.iter().find(|x| x.name == s.name) else {
            if strict.contains(&s.name) { problems.push(format!("{}: zero_copy type is missing from the compiler interface", s.name)); }
            continue;
        };
        if g.size != s.goose_size {
            problems.push(format!("{}: Goose lays it out in {} bytes, the bridge expects {}", s.name, g.size, s.goose_size));
            continue;
        }
        fields_match(&s.name, &s.fields, &g.fields, strict.contains(&s.name), &mut problems);
        if strict.contains(&s.name) {
            // Packed declarations can still borrow an actually aligned address.
            // The strict holder always checks that address before creating &T.
            if g.align != 1 && g.align != s.align {
                problems.push(format!("{}: zero_copy declaration alignment is {}, expected 1 or {}", s.name, g.align, s.align));
            }
        }
    }
    for e in &lib.enums {
        let Some(g) = iface.enums.iter().find(|x| x.name == e.name) else {
            if strict.contains(&e.name) { problems.push(format!("{}: zero_copy enum is missing from the compiler interface", e.name)); }
            continue;
        };
        if g.size != e.goose_size {
            problems.push(format!("{}: Goose lays it out in {} bytes, the bridge expects {}", e.name, g.size, e.goose_size));
        }
        if strict.contains(&e.name) {
            let tag = if e.align > 1 { e.align } else if e.variants.len() <= 256 { 1 } else { 2 };
            if g.align != e.align.max(1) || g.tag_size != tag {
                problems.push(format!("{}: zero_copy enum tag/payload alignment differs from Rust", e.name));
            }
            if g.variants.len() != e.variants.len() {
                problems.push(format!("{}: zero_copy enum variant count differs from Rust", e.name));
            }
            for (tag, v) in e.variants.iter().enumerate() {
                let Some(actual) = g.variants.iter().find(|x| x.name == v.name) else {
                    problems.push(format!("{}::{}: zero_copy variant is missing", e.name, v.name));
                    continue;
                };
                if actual.tag != tag {
                    problems.push(format!("{}::{}: zero_copy variant tag differs from Rust", e.name, v.name));
                }
                fields_match(&format!("{}::{}", e.name, v.name), &v.fields, &actual.fields, true, &mut problems);
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("the generated module's layouts differ from Rust's:\n  {}", problems.join("\n  ")))
    }
}
