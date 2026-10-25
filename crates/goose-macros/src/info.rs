//! Which crate the macros are expanding in: its name, version, the Goose
//! namespace its exports land in, and the library file Goose opens.
//!
//! The namespace is `[package.metadata.goose] namespace = "..."` in the
//! crate's Cargo.toml, else the crate's name without a `goose-` prefix.

use proc_macro2::TokenStream;
use quote::quote;
use std::sync::OnceLock;

pub struct CrateInfo {
    pub name: String,
    pub version: String,
    pub namespace: String,
    pub library: String,
}

pub fn crate_info() -> &'static CrateInfo {
    static INFO: OnceLock<CrateInfo> = OnceLock::new();
    INFO.get_or_init(|| {
        let name = std::env::var("CARGO_PKG_NAME").unwrap_or_else(|_| "crate".into());
        let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
        let library = std::env::var("CARGO_CRATE_NAME").unwrap_or_else(|_| name.replace('-', "_"));
        let namespace = std::env::var("CARGO_MANIFEST_DIR")
            .ok()
            .and_then(|dir| std::fs::read_to_string(std::path::Path::new(&dir).join("Cargo.toml")).ok())
            .and_then(|toml| metadata_namespace(&toml))
            .unwrap_or_else(|| default_namespace(&name));
        CrateInfo { name, version, namespace, library }
    })
}

fn default_namespace(name: &str) -> String {
    let n = name.replace('-', "_");
    let n = n.strip_prefix("goose_").unwrap_or(&n);
    let n = n.strip_suffix("_goose").unwrap_or(n);
    n.to_string()
}

/// `namespace = "..."` under `[package.metadata.goose]`: a line scan, which
/// is all this one key needs.
fn metadata_namespace(toml: &str) -> Option<String> {
    let mut in_section = false;
    for line in toml.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_section = l == "[package.metadata.goose]";
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some(rest) = l.strip_prefix("namespace") {
            let rest = rest.trim_start();
            if let Some(v) = rest.strip_prefix('=') {
                let v = v.trim();
                let v = v.split('#').next().unwrap_or(v).trim();
                if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
                    return Some(v[1..v.len() - 1].to_string());
                }
            }
        }
    }
    None
}

/// The record of which crate this is, submitted with every item.
pub fn library_info() -> TokenStream {
    let i = crate_info();
    let (name, version, ns, lib) = (&i.name, &i.version, &i.namespace, &i.library);
    quote! {
        ::goose::__private::inventory::submit! {
            ::goose::__private::LibraryInfo {
                crate_name: #name,
                version: #version,
                namespace: #ns,
                library: #lib,
            }
        }
    }
}
