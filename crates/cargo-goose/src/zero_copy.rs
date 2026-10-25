//! Copy policy checks on library metadata, before a strict binding is emitted.

use crate::meta::{Function, Library, ParamKind, RetKind, Ty};
use std::collections::BTreeSet;

pub fn types(f: &Function, lib: &Library) -> BTreeSet<String> {
    fn visit(t: &Ty, lib: &Library, names: &mut BTreeSet<String>) {
        match t {
            Ty::Array { elem, .. } => visit(elem, lib, names),
            Ty::Named(n) if names.insert(n.clone()) => {
                if let Some(s) = lib.structs.iter().find(|s| s.name == *n) {
                    for f in &s.fields { visit(&f.ty, lib, names); }
                }
                if let Some(e) = lib.enums.iter().find(|e| e.name == *n) {
                    for v in &e.variants {
                        for f in &v.fields { visit(&f.ty, lib, names); }
                    }
                }
            }
            _ => {}
        }
    }
    fn param(p: &ParamKind, lib: &Library, names: &mut BTreeSet<String>) {
        match p {
            ParamKind::Ref(t) | ParamKind::RefMut(t) | ParamKind::Slice(t) |
            ParamKind::SliceMut(t) | ParamKind::TypedBuf(t) => visit(t, lib, names),
            ParamKind::Optional(p) => param(p, lib, names),
            _ => {}
        }
    }
    fn ret(r: &RetKind, lib: &Library, names: &mut BTreeSet<String>) {
        match r {
            RetKind::Borrowed { elem: Some(t), .. } | RetKind::Spans { elem: Some(t), .. } => visit(t, lib, names),
            RetKind::Option(r) | RetKind::Result(r) => ret(r, lib, names),
            _ => {}
        }
    }
    let mut names = BTreeSet::new();
    for p in &f.params { param(&p.kind, lib, &mut names); }
    ret(&f.ret, lib, &mut names);
    names
}

pub fn typed_builder(p: &ParamKind) -> bool {
    match p {
        ParamKind::TypedBuf(_) => true,
        ParamKind::Optional(p) => typed_builder(p),
        _ => false,
    }
}

pub fn validate(f: &Function, lib: &Library) -> Result<(), String> {
    fn param(p: &ParamKind) -> bool {
        match p {
            ParamKind::Value(t) => matches!(t, Ty::Prim(_)),
            ParamKind::Callback { .. } => false,
            ParamKind::Optional(p) => param(p),
            _ => true,
        }
    }
    fn ret(r: &RetKind) -> bool {
        match r {
            RetKind::Bytes | RetKind::Vec(_) | RetKind::Iter { .. } => false,
            RetKind::Value(t) => matches!(t, Ty::Prim(_)),
            RetKind::Option(r) | RetKind::Result(r) => ret(r),
            _ => true,
        }
    }
    if !f.params.iter().all(|p| param(&p.kind)) || !ret(&f.ret) {
        return Err(format!("{}: zero_copy signature requires a payload copy", f.name));
    }
    for n in types(f, lib) {
        let valid = if let Some(s) = lib.structs.iter().find(|s| s.name == n) {
            s.same_layout && s.size == s.goose_size && s.fields.iter().all(|f| f.offset == f.goose_offset)
        } else if let Some(e) = lib.enums.iter().find(|e| e.name == n) {
            e.same_layout && e.size == e.goose_size && e.variants.iter().enumerate().all(|(i, v)| {
                v.discriminant == i as i64 && v.fields.iter().all(|f| f.offset == f.goose_offset)
            })
        } else { false };
        if !valid {
            return Err(format!("{}: zero_copy requires {n} to have the same layout as Rust", f.name));
        }
    }
    Ok(())
}

/// Only emitted strict exports impose layout requirements on the probe.
pub fn required_types(lib: &Library, module: &str) -> BTreeSet<String> {
    lib.functions.iter().filter(|f| f.zero_copy && module.contains(&format!("// zero_copy: {}\n", f.symbol)))
        .flat_map(|f| types(f, lib)).collect()
}
