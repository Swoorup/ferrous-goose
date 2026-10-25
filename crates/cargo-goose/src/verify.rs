//! Checking a generated module against what the compiler makes of it: the
//! size of every struct and enum, and every field's offset, as the Goose
//! compiler lays them out (`--emit-interface`, goose-patches/0003) against
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
    fields: Vec<serde_json::Value>,
}

#[derive(serde::Deserialize)]
struct IEnum {
    name: String,
    size: usize,
}

/// Compiles a probe exporting a function per type of the module, asks the
/// compiler for its interface, and compares.
pub fn verify(goose: &Path, lib: &Library, module: &str) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("cargo-goose-verify-{}-{}", lib.namespace, std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
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
    if n == 0 {
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
        let Some(g) = iface.structs.iter().find(|x| x.name == s.name) else { continue };
        if g.size != s.goose_size {
            problems.push(format!("{}: Goose lays it out in {} bytes, the bridge expects {}", s.name, g.size, s.goose_size));
            continue;
        }
        for f in &s.fields {
            let goff = g.fields.iter().find(|x| x["name"] == f.name.as_str()).and_then(|x| x["offset"].as_u64());
            if goff != Some(f.goose_offset as u64) {
                problems.push(format!(
                    "{}.{}: Goose puts it at {:?}, the bridge at {}",
                    s.name, f.name, goff, f.goose_offset
                ));
            }
        }
    }
    for e in &lib.enums {
        let Some(g) = iface.enums.iter().find(|x| x.name == e.name) else { continue };
        if g.size != e.goose_size {
            problems.push(format!("{}: Goose lays it out in {} bytes, the bridge expects {}", e.name, g.size, e.goose_size));
        }
    }
    let _ = std::fs::remove_file(&pf);
    let _ = std::fs::remove_file(dir.join(format!("{ns}.goose")));
    let _ = std::fs::remove_dir(&dir);
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("the generated module's layouts differ from Rust's:\n  {}", problems.join("\n  ")))
    }
}
