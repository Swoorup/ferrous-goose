//! Writes the Goose module for a library's records: struct and enum
//! declarations with rustc's layout, the extern declarations of the shims,
//! and the wrappers that give each Rust shape its natural Goose shape.
//!
//! The lowering here (which C parameters a shim takes) mirrors the
//! `#[goose::export]` macro's, and is checked against the shapes the macro
//! recorded for every function it could record them for.

use crate::meta::*;
use std::collections::BTreeSet;
use std::fmt::Write;

/// How the program reaches the library.
#[derive(Clone, Debug)]
pub enum Link {
    /// An `extern "lib" { ... }` block, opened when the program starts: what
    /// `goose run` needs, and what Phase 1 uses everywhere.
    Dynamic(String),
    /// Plain `extern fn` declarations, the library linked into the program
    /// (a `native_build` provider, goose-patches/0005).
    Static,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub link: Link,
    /// The compiler takes `T[>..]&` of any flat T at the boundary
    /// (goose-patches/0001). Without it, such results are counted, then
    /// copied into an array the wrapper sizes.
    pub typed_builders: bool,
    /// The compiler has `align n` (goose-patches/0007): structs are
    /// declared with Rust's alignment, so Goose keeps their values aligned
    /// and they are lent in place, and `#[goose(align)]` enums can cross.
    pub align: bool,
    /// The compiler has `reentrant` externs (goose-patches/0008), which a
    /// function calling Goose back is declared as.
    pub reentrant: bool,
    /// Compiler layout JSON is available; required before exposing strict exports.
    pub emit_interface: bool,
}

const KEYWORDS: &[&str] = &[
    "fn", "struct", "enum", "let", "var", "const", "if", "else", "while", "for", "in", "loop", "block", "break",
    "continue", "return", "from", "match", "guard", "import", "namespace", "extern", "export", "type", "pad", "true",
    "false", "null", "as", "is", "thread_fn", "simd", "inline", "recursive", "checkfree", "reusable", "upto",
    "default", "copy", "case", "self",
];

/// A Rust name as a Goose name.
pub fn name(n: &str) -> String {
    if KEYWORDS.contains(&n) {
        format!("{n}_")
    } else {
        n.to_string()
    }
}

pub fn ty(t: &Ty) -> String {
    match t {
        Ty::Prim(p) => p.clone(),
        Ty::Named(n) => n.clone(),
        Ty::Array { elem, len } => format!("{}[{len}]", ty(elem)),
    }
}

fn is_prim(t: &Ty) -> bool {
    matches!(t, Ty::Prim(_))
}

fn comment(out: &mut String, doc: &str, indent: &str) {
    for l in doc.lines() {
        let l = l.trim_end();
        if l.is_empty() {
            let _ = writeln!(out, "{indent}//");
        } else {
            let _ = writeln!(out, "{indent}// {l}");
        }
    }
}

fn camel(s: &str) -> String {
    s.split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn snake(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 && !out.ends_with('_') {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// One extern declaration's parameters, and what the wrapper passes.
#[derive(Default, Clone)]
struct Call {
    decl: Vec<String>,
    shapes: Vec<String>,
    args: Vec<String>,
}

impl Call {
    fn push(&mut self, decl: String, shape: &str, arg: String) {
        self.decl.push(decl);
        self.shapes.push(shape.to_string());
        self.args.push(arg);
    }
}

/// The parameter as the wrapper declares it.
fn wrapper_param(p: &Param) -> Result<String, String> {
    let n = name(&p.name);
    Ok(match &p.kind {
        ParamKind::Scalar(t) => format!("{n}: {}", ty(t)),
        ParamKind::Value(t) => format!("{n}: {}", ty(t)),
        ParamKind::Ref(t) => format!("{n}: const {}&", ty(t)),
        ParamKind::RefMut(t) => format!("{n}: {}&", ty(t)),
        ParamKind::Slice(t) => format!("{n}: const {}[:]", ty(t)),
        ParamKind::SliceMut(t) => format!("{n}: {}[:]", ty(t)),
        ParamKind::Str | ParamKind::Bytes => format!("{n}: const u8[:]"),
        ParamKind::Buf => format!("{n}: u8[>..]&"),
        ParamKind::TypedBuf(t) => format!("{n}: {}[>..]&", ty(t)),
        ParamKind::Handle(h) | ParamKind::HandleMut(h) | ParamKind::HandleOwned(h) => format!("{n}: {h}"),
        ParamKind::Optional(inner) => {
            let t = match &**inner {
                ParamKind::Scalar(t) | ParamKind::Value(t) | ParamKind::Ref(t) => ty(t),
                ParamKind::Handle(h) | ParamKind::HandleOwned(h) => h.clone(),
                ParamKind::Str | ParamKind::Bytes => "const u8[:]".to_string(),
                ParamKind::Slice(t) => format!("const {}[:]", ty(t)),
                other => return Err(format!("parameter {}: Option of {other:?} has no Goose shape", p.name)),
            };
            format!("{n}: {t}?")
        }
        ParamKind::Callback { .. } => return Err("callback parameters are lowered elsewhere".into()),
    })
}

/// Lowers a (present) parameter to the extern declaration.
fn lower_param(c: &mut Call, p: &Param, kind: &ParamKind) {
    let n = name(&p.name);
    match kind {
        ParamKind::Scalar(t) => c.push(format!("{n}: {}", ty(t)), &ty(t), n),
        ParamKind::Value(t) | ParamKind::Ref(t) => c.push(format!("{n}: const {}&", ty(t)), "ptr", n),
        ParamKind::RefMut(t) => c.push(format!("{n}: {}&", ty(t)), "ptr", n),
        ParamKind::Slice(t) => c.push(format!("{n}: const {}[:]", ty(t)), "slice", n),
        ParamKind::SliceMut(t) => c.push(format!("{n}: {}[:]", ty(t)), "slice", n),
        ParamKind::Str | ParamKind::Bytes => c.push(format!("{n}: const u8[:]"), "slice", n),
        ParamKind::Buf => c.push(format!("{n}: u8[>..]&"), "builder", n),
        ParamKind::TypedBuf(t) => c.push(format!("{n}: {}[>..]&", ty(t)), "builder", n),
        ParamKind::Handle(h) | ParamKind::HandleMut(h) | ParamKind::HandleOwned(h) => {
            c.push(format!("{n}: {h}"), "handle", n)
        }
        ParamKind::Optional(_) | ParamKind::Callback { .. } => unreachable!(),
    }
}

/// Whether a result needs a typed builder.
fn needs_typed(r: &RetKind) -> bool {
    match r {
        RetKind::Vec(_) | RetKind::Spans { .. } => true,
        RetKind::Option(i) | RetKind::Result(i) => needs_typed(i),
        _ => false,
    }
}

fn typed_elem(r: &RetKind) -> Option<String> {
    match r {
        RetKind::Vec(t) => Some(ty(t)),
        RetKind::Spans { .. } => Some("Span".to_string()),
        RetKind::Option(i) | RetKind::Result(i) => typed_elem(i),
        _ => None,
    }
}

/// The wrapper's result type(s) for a result.
fn wrapper_ret(r: &RetKind, params: &[Param]) -> Result<Vec<String>, String> {
    Ok(match r {
        RetKind::Unit => vec![],
        RetKind::Scalar(t) | RetKind::Value(t) => vec![ty(t)],
        RetKind::Handle(h) => vec![h.clone()],
        RetKind::Bytes => vec!["u8[>..]".to_string()],
        RetKind::Vec(t) => vec![format!("{}[>..]", ty(t))],
        RetKind::Borrowed { param, elem } => {
            let mutable = matches!(params.get(*param).map(|p| &p.kind), Some(ParamKind::SliceMut(_)));
            let e = elem.as_ref().map(ty).unwrap_or_else(|| "u8".into());
            vec![if mutable { format!("{e}[:]") } else { format!("const {e}[:]") }]
        }
        RetKind::Spans { .. } => vec!["Span[>..]".to_string()],
        RetKind::Iter { handle, .. } => vec![handle.clone()],
        RetKind::Option(i) | RetKind::Result(i) => {
            let mut v = wrapper_ret(i, params)?;
            v.push("bool".to_string());
            v
        }
    })
}

/// Out-parameters of a result, as declared and passed: (decl, shape, arg).
fn ret_outs(r: &RetKind, wrapped: bool, stash: bool, out: &mut Vec<(String, String, String)>) {
    match r {
        RetKind::Unit => {}
        RetKind::Scalar(t) => {
            if wrapped {
                out.push((format!("out: {}&", ty(t)), format!("out:{}", ty(t)), "r_out".into()));
            }
        }
        RetKind::Value(t) => out.push((format!("out: {}&", ty(t)), "out:ptr".into(), "r_out".into())),
        RetKind::Handle(h) => out.push((format!("out: {h}&"), "out:handle".into(), "r_out".into())),
        RetKind::Bytes => out.push(("out: u8[>..]&".into(), "builder".into(), "r_out".into())),
        RetKind::Vec(_) | RetKind::Spans { .. } => {
            if !stash {
                let e = typed_elem(r).unwrap();
                out.push((format!("out: {e}[>..]&"), "builder".into(), "r_out".into()));
            }
        }
        RetKind::Borrowed { .. } => {
            out.push(("at: i64&".into(), "out:i64".into(), "r_at".into()));
            out.push(("len: i64&".into(), "out:i64".into(), "r_len".into()));
        }
        RetKind::Iter { handle, .. } => out.push((format!("out: {handle}&"), "out:handle".into(), "r_out".into())),
        RetKind::Option(i) => ret_outs(i, true, stash, out),
        RetKind::Result(i) => {
            ret_outs(i, true, stash, out);
            out.push(("err: u8[>..]&".into(), "builder".into(), "err".into()));
        }
    }
}

fn c_ret(r: &RetKind, stash: bool) -> (String, String) {
    if stash {
        return (" -> i64".into(), "i64".into());
    }
    match r {
        RetKind::Scalar(t) => (format!(" -> {}", ty(t)), ty(t)),
        RetKind::Option(_) | RetKind::Result(_) => (" -> bool".into(), "bool".into()),
        _ => (String::new(), "void".into()),
    }
}

/// Declarations of the locals a result's out-parameters write.
fn ret_locals(r: &RetKind, wrapped: bool, out: &mut String, ind: &str) {
    match r {
        RetKind::Unit => {}
        RetKind::Scalar(_) if !wrapped => {}
        RetKind::Scalar(t) | RetKind::Value(t) => {
            let _ = writeln!(out, "{ind}var r_out = default<{}>();", ty(t));
        }
        RetKind::Handle(h) | RetKind::Iter { handle: h, .. } => {
            let _ = writeln!(out, "{ind}var r_out = default<{h}>();");
        }
        RetKind::Bytes => {
            let _ = writeln!(out, "{ind}var r_out: u8[>..] = [];");
        }
        RetKind::Vec(_) | RetKind::Spans { .. } => {
            let _ = writeln!(out, "{ind}var r_out: {}[>..] = [];", typed_elem(r).unwrap());
        }
        RetKind::Borrowed { .. } => {
            let _ = writeln!(out, "{ind}var r_at: i64 = 0;");
            let _ = writeln!(out, "{ind}var r_len: i64 = 0;");
        }
        RetKind::Option(i) | RetKind::Result(i) => ret_locals(i, true, out, ind),
    }
}

/// The value expression a result's locals make.
fn ret_value(r: &RetKind, params: &[Param]) -> Option<String> {
    match r {
        RetKind::Unit => None,
        RetKind::Borrowed { param, .. } => {
            let src = name(&params[*param].name);
            Some(format!("{src}[r_at..r_at + r_len]"))
        }
        RetKind::Option(i) | RetKind::Result(i) => ret_value(i, params),
        _ => Some("r_out".to_string()),
    }
}

pub struct Generator<'a> {
    lib: &'a Library,
    o: &'a Options,
    /// The function being declared calls Goose back: its externs are
    /// `reentrant`.
    reentrant_now: bool,
    /// Types this compiler cannot declare (an aligned enum without align),
    /// and everything that uses them, left out of the module.
    skipped: BTreeSet<String>,
    externs: Vec<String>,
    wrappers: String,
    extra_types: BTreeSet<String>,
    uses_exception: bool,
}

pub fn generate(lib: &Library, o: &Options) -> Result<String, String> {
    let mut g = Generator {
        lib,
        o,
        reentrant_now: false,
        skipped: BTreeSet::new(),
        externs: Vec::new(),
        wrappers: String::new(),
        extra_types: BTreeSet::new(),
        uses_exception: false,
    };
    g.run()
}

impl Generator<'_> {
    fn extern_fn(&mut self, sym: &str, decl: &[String], ret: &str) {
        let r = if self.reentrant_now { "reentrant " } else { "" };
        self.externs.push(format!("{r}fn {sym}({}){ret};", decl.join(", ")));
    }

    fn bind(&mut self, goose: &str, sym: &str, decl: &[String], ret: &str) {
        let r = if self.reentrant_now { "reentrant " } else { "" };
        self.externs.push(format!("{r}fn {goose}({}){ret} = \"{sym}\";", decl.join(", ")));
    }

    fn check_shapes(&self, f: &Function, shapes: &[String], ret_shape: &str) -> Result<(), String> {
        if f.c_params.is_empty() && f.c_ret.is_empty() {
            return Ok(());
        }
        if f.c_params != shapes || f.c_ret != ret_shape {
            return Err(format!(
                "{}: the library's shim takes ({}) -> {}, but this cargo-goose lowers it to ({}) -> {}: \
                 the crate was built with another version of the goose crate",
                f.symbol,
                f.c_params.join(", "),
                f.c_ret,
                shapes.join(", "),
                ret_shape
            ));
        }
        Ok(())
    }

    fn run(&mut self) -> Result<String, String> {
        let lib = self.lib;
        // What this compiler cannot declare, and what holds it.
        if !self.o.align {
            for e in &lib.enums {
                if e.align > 1 {
                    self.skipped.insert(e.name.clone());
                }
            }
            loop {
                let before = self.skipped.len();
                for st in &lib.structs {
                    if st.fields.iter().any(|f| ty_uses(&f.ty, &self.skipped)) {
                        self.skipped.insert(st.name.clone());
                    }
                }
                for e in &lib.enums {
                    if e.variants.iter().any(|v| v.fields.iter().any(|f| ty_uses(&f.ty, &self.skipped))) {
                        self.skipped.insert(e.name.clone());
                    }
                }
                if self.skipped.len() == before {
                    break;
                }
            }
        }
        for f in &lib.functions {
            if f.zero_copy {
                crate::zero_copy::validate(f, lib)?;
                if !self.o.emit_interface {
                    let _ = writeln!(self.wrappers, "// {}: zero_copy needs compiler layout verification (--emit-interface).\n", f.name);
                    continue;
                }
                if !self.o.typed_builders && (needs_typed(&f.ret) || f.params.iter().any(|p| crate::zero_copy::typed_builder(&p.kind))) {
                    let _ = writeln!(self.wrappers, "// {}: zero_copy needs typed builders; the count/copy fallback is disabled.\n", f.name);
                    continue;
                }
            }
            if f.reentrant && !self.o.reentrant {
                let _ = writeln!(
                    self.wrappers,
                    "// {}: calls the program back, which needs a compiler with reentrant externs (goose-patches/0008).\n",
                    f.name
                );
                continue;
            }
            if fn_uses(f, &self.skipped) {
                let _ = writeln!(
                    self.wrappers,
                    "// {}: uses a type this compiler cannot lay out as Rust does (align n, goose-patches/0007).\n",
                    f.name
                );
                continue;
            }
            if f.zero_copy {
                let _ = writeln!(self.wrappers, "// zero_copy: {}", f.symbol);
            }
            self.reentrant_now = f.reentrant;
            let r = self.function(f).map_err(|e| format!("{}::{}: {e}", lib.namespace, f.name));
            self.reentrant_now = false;
            r?;
        }
        for h in &lib.handles {
            self.bind("release", &h.release, &[format!("h: {}", h.name)], "");
            if !h.live_count.is_empty() {
                let n = format!("live_count_{}", snake(&h.name));
                self.bind(&n, &h.live_count, &[], " -> i64");
            }
        }
        self.bind("live_count", "goose_live_count", &[], " -> i64");
        self.bind("copy_count", "goose_copy_count", &[], " -> i64");
        self.bind("leak_report", "goose_leak_report", &["out: u8[>..]&".to_string()], " -> i64");
        self.extern_fn("goose_abi_1", &[], " -> u32");

        let mut out = String::new();
        let _ = writeln!(
            out,
            "// Generated by cargo goose {} from {} {} (Ferrous Goose {}, metadata format {}, bridge ABI {}).",
            env!("CARGO_PKG_VERSION"),
            lib.crate_name,
            lib.version,
            lib.bridge,
            lib.format,
            lib.abi
        );
        let _ = writeln!(out, "// Do not edit: run cargo goose bindings again.");
        let _ = writeln!(out, "namespace {};", lib.namespace);
        if self.uses_exception {
            let _ = writeln!(out, "\nimport exception;");
        }
        out.push('\n');
        for s in &lib.structs {
            if self.skipped.contains(&s.name) {
                continue;
            }
            comment(&mut out, &s.doc, "");
            out.push_str(&struct_decl(s, self.o.align));
            out.push('\n');
        }
        for e in &lib.enums {
            if self.skipped.contains(&e.name) {
                let _ = writeln!(out, "// {}: #[goose(align)], which needs a compiler with align n (goose-patches/0007).\n", e.name);
                continue;
            }
            comment(&mut out, &e.doc, "");
            out.push_str(&enum_decl(e));
            out.push('\n');
        }
        if self.extra_types.contains("Span") {
            if lib.structs.iter().any(|s| s.name == "Span") || lib.enums.iter().any(|e| e.name == "Span") {
                return Err("the crate declares a type named Span, which the span lists need".into());
            }
            let _ = writeln!(out, "// A part of an argument: a span list's element.");
            let _ = writeln!(out, "struct Span {{ at: i64, len: i64 }}\n");
        }
        for h in &lib.handles {
            comment(&mut out, &h.doc, "");
            let _ = writeln!(out, "// A handle: release it when done{}.", if h.pinned { "; it stays on the thread that made it" } else { "" });
            let _ = writeln!(out, "struct {} {{ slot: u32, generation: u32 }}\n", h.name);
        }
        for t in &self.extra_types {
            if t != "Span" {
                let _ = writeln!(out, "// The state of a call running a callback.");
                let _ = writeln!(out, "struct {t} {{ slot: u32, generation: u32 }}\n");
            }
        }
        match &self.o.link {
            Link::Dynamic(l) => {
                let _ = writeln!(out, "extern \"{l}\" {{");
                for e in &self.externs {
                    let _ = writeln!(out, "    {e}");
                }
                let _ = writeln!(out, "}}");
            }
            Link::Static => {
                for e in &self.externs {
                    if e.starts_with("//") {
                        let _ = writeln!(out, "{e}");
                    } else {
                        let _ = writeln!(out, "extern {e}");
                    }
                }
            }
        }
        let _ = writeln!(
            out,
            "\n// The library must be one built with this bridge ABI: another has no\n// goose_abi_1, and the program stops when it starts, naming it.\nlet ferrous_abi = goose_abi_1();\n"
        );
        out.push_str(&self.wrappers);
        let _ = writeln!(out, "// The text of the leak report: one line per handle type with live handles.");
        let _ = writeln!(out, "fn leak_report() -> u8[>..] {{\n    var out: u8[>..] = [];\n    leak_report(out);\n    return out;\n}}");
        Ok(out)
    }

    fn function(&mut self, f: &Function) -> Result<(), String> {
        if !self.o.typed_builders && f.params.iter().any(|p| matches!(p.kind, ParamKind::TypedBuf(_))) {
            let _ = writeln!(
                self.wrappers,
                "// {}: takes a typed builder, which this compiler cannot pass (goose-patches/0001).\n",
                f.name
            );
            return Ok(());
        }
        if let Some(ci) = f.params.iter().position(|p| matches!(p.kind, ParamKind::Callback { .. })) {
            return self.callback_function(f, ci);
        }
        let optionals: Vec<usize> = f
            .params
            .iter()
            .enumerate()
            .filter(|(_, p)| matches!(p.kind, ParamKind::Optional(_)))
            .map(|(i, _)| i)
            .collect();
        let typed = needs_typed(&f.ret);
        let stash = typed && !self.o.typed_builders;
        if typed {
            if matches!(&f.ret, RetKind::Spans { .. }) || matches!(&f.ret, RetKind::Option(i) | RetKind::Result(i) if matches!(**i, RetKind::Spans { .. })) {
                self.extra_types.insert("Span".to_string());
            }
        }
        if let RetKind::Borrowed { param, .. } = &f.ret {
            if matches!(f.params[*param].kind, ParamKind::Optional(_)) {
                return Err("a result borrowed from an optional parameter has no Goose shape".into());
            }
        }

        // The extern declarations: one per combination of the optionals.
        let mut leaves = Vec::new();
        for mask in 0..(1usize << optionals.len()) {
            let mut sym = f.symbol.clone();
            if !optionals.is_empty() {
                sym.push('_');
                if optionals.len() == 1 {
                    sym.push_str(if mask & 1 == 1 { "some" } else { "none" });
                } else {
                    for k in 0..optionals.len() {
                        sym.push(if mask & (1 << k) != 0 { 's' } else { 'n' });
                    }
                }
            }
            let mut c = Call::default();
            for (i, p) in f.params.iter().enumerate() {
                if let ParamKind::Optional(inner) = &p.kind {
                    let k = optionals.iter().position(|&o| o == i).unwrap();
                    if mask & (1 << k) != 0 {
                        lower_param(&mut c, p, inner);
                    }
                } else {
                    lower_param(&mut c, p, &p.kind);
                }
            }
            let mut outs = Vec::new();
            ret_outs(&f.ret, false, false, &mut outs);
            let (ret_s, ret_shape) = c_ret(&f.ret, false);
            let mut decl = c.decl.clone();
            let mut shapes = c.shapes.clone();
            for (d, s, _) in &outs {
                decl.push(d.clone());
                shapes.push(s.clone());
            }
            if optionals.is_empty() {
                self.check_shapes(f, &shapes, &ret_shape)?;
            }
            // A plain forward is bound under the Goose name, with no wrapper.
            let direct = optionals.is_empty()
                && matches!(f.ret, RetKind::Unit | RetKind::Scalar(_))
                && f.params.iter().all(|p| !matches!(p.kind, ParamKind::Value(_)));
            if direct {
                let mut d = String::new();
                comment(&mut d, &f.doc, "");
                if f.main_thread {
                    d.push_str("// Call it from the main thread only.\n");
                }
                for l in d.lines() {
                    self.externs.push(l.to_string());
                }
                self.bind(&name(&f.name), &sym, &decl, &ret_s);
                let mut w = String::new();
                self.buf_forms(&mut w, f)?;
                self.wrappers.push_str(&w);
                return Ok(());
            }
            if stash {
                let mut sdecl = c.decl.clone();
                let mut souts = Vec::new();
                ret_outs(&f.ret, false, true, &mut souts);
                for (d, _, _) in &souts {
                    sdecl.push(d.clone());
                }
                self.extern_fn(&format!("{sym}__len"), &sdecl, " -> i64");
            } else {
                self.extern_fn(&sym, &decl, &ret_s);
            }
            leaves.push((mask, sym, c));
        }

        if stash {
            let e = typed_elem(&f.ret).unwrap();
            self.extern_fn(&format!("{}__take", f.symbol), &[format!("out: {e}[:]")], "");
        }

        // The wrapper.
        let mut w = String::new();
        comment(&mut w, &f.doc, "");
        if f.main_thread {
            let _ = writeln!(w, "// Call it from the main thread only.");
        }
        let mut wparams = Vec::new();
        for p in &f.params {
            wparams.push(wrapper_param(p)?);
        }
        let fallible_result = matches!(f.ret, RetKind::Result(_));
        if fallible_result {
            wparams.push("err: u8[>..]&".to_string());
        }
        let rets = wrapper_ret(&f.ret, &f.params)?;
        let ret_s = if rets.is_empty() { String::new() } else { format!(" -> {}", rets.join(", ")) };
        let fname = name(&f.name);
        let _ = writeln!(w, "fn {fname}({}){ret_s} {{", wparams.join(", "));
        ret_locals(&f.ret, false, &mut w, "    ");
        let status = match (&f.ret, stash) {
            (_, true) => Some("r_n"),
            (RetKind::Option(_) | RetKind::Result(_), false) => Some("r_ok"),
            (RetKind::Scalar(_), false) => Some("r_out"),
            _ => None,
        };
        let call_for = |sym: &str, c: &Call| -> String {
            let mut outs = Vec::new();
            ret_outs(&f.ret, false, stash, &mut outs);
            let mut args = c.args.clone();
            for (_, _, a) in outs {
                args.push(a);
            }
            if stash {
                format!("{sym}__len({})", args.join(", "))
            } else {
                format!("{sym}({})", args.join(", "))
            }
        };
        if optionals.is_empty() {
            let (_, sym, c) = &leaves[0];
            let call = call_for(sym, c);
            match status {
                Some("r_out") => {
                    let _ = writeln!(w, "    return {call};");
                }
                Some(s) => {
                    let _ = writeln!(w, "    let {s} = {call};");
                }
                None => {
                    let _ = writeln!(w, "    {call};");
                }
            }
        } else {
            if let Some(s) = status {
                let init = match s {
                    "r_n" => "var r_n: i64 = 0;".to_string(),
                    "r_ok" => "var r_ok = false;".to_string(),
                    _ => {
                        let RetKind::Scalar(t) = &f.ret else { unreachable!() };
                        format!("var r_out = default<{}>();", ty(t))
                    }
                };
                let _ = writeln!(w, "    {init}");
            }
            self.branch(&mut w, f, &optionals, 0, 0, &leaves, &call_for, status, "    ");
        }
        // What comes back.
        match &f.ret {
            RetKind::Unit => {}
            RetKind::Scalar(_) => {
                if !optionals.is_empty() {
                    let _ = writeln!(w, "    return r_out;");
                }
            }
            RetKind::Option(_) | RetKind::Result(_) => {
                let v = ret_value(&f.ret, &f.params);
                if stash {
                    let e = typed_elem(&f.ret).unwrap();
                    let _ = writeln!(w, "    guard r_n >= 0 else {{ return r_out, false; }}");
                    let _ = writeln!(w, "    for r_i in r_n {{ r_out.push(default<{e}>()); }}");
                    let _ = writeln!(w, "    {}__take(r_out);", f.symbol);
                    let _ = writeln!(w, "    return r_out, true;");
                } else {
                    match v {
                        Some(v) => {
                            let _ = writeln!(w, "    return {v}, r_ok;");
                        }
                        None => {
                            let _ = writeln!(w, "    return r_ok;");
                        }
                    }
                }
            }
            other => {
                if stash {
                    let e = typed_elem(other).unwrap();
                    let _ = writeln!(w, "    for r_i in r_n {{ r_out.push(default<{e}>()); }}");
                    let _ = writeln!(w, "    {}__take(r_out);", f.symbol);
                }
                let v = ret_value(other, &f.params).unwrap();
                let _ = writeln!(w, "    return {v};");
            }
        }
        let _ = writeln!(w, "}}\n");

        // A Result without the error text, or one that throws it.
        if fallible_result {
            let RetKind::Result(inner) = &f.ret else { unreachable!() };
            let plain: Vec<String> = wparams[..wparams.len() - 1].to_vec();
            let args: Vec<String> = f.params.iter().map(|p| name(&p.name)).collect();
            let inner_rets = wrapper_ret(inner, &f.params)?;
            let mut call_args = args.clone();
            call_args.push("err".into());
            if f.throws {
                self.uses_exception = true;
                let r = if inner_rets.is_empty() { String::new() } else { format!(" -> {}", inner_rets.join(", ")) };
                // Untyped parameters make it generic, so it is checked only
                // where it is called: inside a try, as a throw must be.
                let untyped: Vec<String> = f.params.iter().map(|p| name(&p.name)).collect();
                let _ = writeln!(w, "// Throws the error text (u8[]) through the exception module.");
                let _ = writeln!(w, "fn {fname}({}){r} {{", untyped.join(", "));
                let _ = writeln!(w, "    var err: u8[>..] = [];");
                if inner_rets.is_empty() {
                    let _ = writeln!(w, "    if !{fname}({}) {{ exception::throw(copy(err)); }}", call_args.join(", "));
                } else {
                    let _ = writeln!(w, "    let r_v, r_ok = {fname}({});", call_args.join(", "));
                    let _ = writeln!(w, "    if !r_ok {{ exception::throw(copy(err)); }}");
                    let _ = writeln!(w, "    return r_v;");
                }
                let _ = writeln!(w, "}}\n");
            } else {
                let _ = writeln!(w, "fn {fname}({}){ret_s} {{", plain.join(", "));
                let _ = writeln!(w, "    var err: u8[>..] = [];");
                if inner_rets.is_empty() {
                    let _ = writeln!(w, "    return {fname}({});", call_args.join(", "));
                } else {
                    let _ = writeln!(w, "    let r_v, r_ok = {fname}({});", call_args.join(", "));
                    let _ = writeln!(w, "    return r_v, r_ok;");
                }
                let _ = writeln!(w, "}}\n");
            }
        }

        // Extras: a block form for span lists and iterators, a scoped form
        // for constructors of handles, a returning form for a builder.
        self.extras(&mut w, f, &wparams, fallible_result)?;
        self.buf_forms(&mut w, f)?;
        self.wrappers.push_str(&w);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn branch(
        &self,
        w: &mut String,
        f: &Function,
        optionals: &[usize],
        k: usize,
        mask: usize,
        leaves: &[(usize, String, Call)],
        call_for: &dyn Fn(&str, &Call) -> String,
        status: Option<&str>,
        ind: &str,
    ) {
        if k == optionals.len() {
            let (_, sym, c) = leaves.iter().find(|(m, _, _)| *m == mask).unwrap();
            let call = call_for(sym, c);
            match status {
                Some(s) => {
                    let _ = writeln!(w, "{ind}{s} = {call};");
                }
                None => {
                    let _ = writeln!(w, "{ind}{call};");
                }
            }
            return;
        }
        let p = name(&f.params[optionals[k]].name);
        let inner = format!("{ind}    ");
        let _ = writeln!(w, "{ind}if {p} {{");
        self.branch(w, f, optionals, k + 1, mask | (1 << k), leaves, call_for, status, &inner);
        let _ = writeln!(w, "{ind}}} else {{");
        self.branch(w, f, optionals, k + 1, mask, leaves, call_for, status, &inner);
        let _ = writeln!(w, "{ind}}}");
    }

    /// A function filling one builder parameter also gets a form that
    /// builds a fresh array and returns it.
    fn buf_forms(&mut self, w: &mut String, f: &Function) -> Result<(), String> {
        let bufs: Vec<usize> = f
            .params
            .iter()
            .enumerate()
            .filter(|(_, p)| matches!(p.kind, ParamKind::Buf | ParamKind::TypedBuf(_)))
            .map(|(i, _)| i)
            .collect();
        if bufs.len() != 1 || !matches!(f.ret, RetKind::Unit) {
            return Ok(());
        }
        let bi = bufs[0];
        let elem = match &f.params[bi].kind {
            ParamKind::TypedBuf(t) => ty(t),
            _ => "u8".to_string(),
        };
        let mut wparams = Vec::new();
        let mut args = Vec::new();
        for (i, p) in f.params.iter().enumerate() {
            if i == bi {
                args.push("r_out".to_string());
            } else {
                wparams.push(wrapper_param(p)?);
                args.push(name(&p.name));
            }
        }
        let fname = name(&f.name);
        let _ = writeln!(w, "// {fname}, built into a fresh array.");
        let _ = writeln!(w, "fn {fname}({}) -> {elem}[>..] {{", wparams.join(", "));
        let _ = writeln!(w, "    var r_out: {elem}[>..] = [];");
        let _ = writeln!(w, "    {fname}({});", args.join(", "));
        let _ = writeln!(w, "    return r_out;");
        let _ = writeln!(w, "}}\n");
        Ok(())
    }

    fn extras(&mut self, w: &mut String, f: &Function, wparams: &[String], fallible: bool) -> Result<(), String> {
        let fname = name(&f.name);
        let args: Vec<String> = f.params.iter().map(|p| name(&p.name)).collect();
        match &f.ret {
            RetKind::Spans { param, .. } => {
                let src = name(&f.params[*param].name);
                let _ = writeln!(w, "// Each part reaches the block as a slice of {src}: nothing is copied.");
                let _ = writeln!(w, "fn each_{}<F>({}) {{", f.name, wparams.join(", "));
                let _ = writeln!(w, "    let r_spans = {fname}({});", args.join(", "));
                let _ = writeln!(w, "    for r_s in r_spans {{ F({src}[r_s.at..r_s.at + r_s.len]); }}");
                let _ = writeln!(w, "}}\n");
            }
            RetKind::Iter { handle, item } => {
                // next and each.
                let sym = &f.symbol;
                let mut outs = Vec::new();
                ret_outs(item, true, false, &mut outs);
                let mut decl = vec![format!("it: {handle}")];
                for (d, _, _) in &outs {
                    decl.push(d.clone());
                }
                self.extern_fn(&format!("{sym}__next"), &decl, " -> bool");
                let item_ret = wrapper_ret(item, &f.params)?;
                let _ = writeln!(w, "// The next item, and whether there was one.");
                let _ = writeln!(w, "fn next(it: {handle}) -> {}, bool {{", item_ret.join(", "));
                ret_locals(item, true, w, "    ");
                let _ = writeln!(w, "    let r_ok = {sym}__next(it, r_out);");
                let _ = writeln!(w, "    return r_out, r_ok;");
                let _ = writeln!(w, "}}\n");
                let pass = if matches!(**item, RetKind::Bytes) { "r_x[0..r_x.len]" } else { "r_x" };
                let _ = writeln!(w, "// Each item reaches the block; the iterator is released at the end.");
                let _ = writeln!(w, "fn each_{}<F>({}) {{", f.name, wparams.join(", "));
                let _ = writeln!(w, "    let r_it = {fname}({});", args.join(", "));
                let _ = writeln!(w, "    loop {{");
                let _ = writeln!(w, "        let r_x, r_ok = next(r_it);");
                let _ = writeln!(w, "        guard r_ok else {{ break; }}");
                let _ = writeln!(w, "        F({pass});");
                let _ = writeln!(w, "    }}");
                let _ = writeln!(w, "    release(r_it);");
                let _ = writeln!(w, "}}\n");
            }
            _ => {}
        }
        // Constructors of handles get a scoped form.
        let handle_ret = match &f.ret {
            RetKind::Handle(h) => Some((h.clone(), false)),
            RetKind::Option(i) | RetKind::Result(i) => match &**i {
                RetKind::Handle(h) => Some((h.clone(), true)),
                _ => None,
            },
            _ => None,
        };
        if let Some((_h, checked)) = handle_ret {
            let _ = writeln!(w, "// Releases the handle when the block finishes normally (an early return");
            let _ = writeln!(w, "// out of the block skips it).");
            let mut call_args = args.clone();
            if fallible {
                call_args.push("err".into());
            }
            if checked {
                let _ = writeln!(w, "fn with_{}<F>({}) -> bool {{", f.name, wparams.join(", "));
                let _ = writeln!(w, "    let r_h, r_ok = {fname}({});", call_args.join(", "));
                let _ = writeln!(w, "    guard r_ok else {{ return false; }}");
                let _ = writeln!(w, "    F(r_h);");
                let _ = writeln!(w, "    release(r_h);");
                let _ = writeln!(w, "    return true;");
            } else {
                let _ = writeln!(w, "fn with_{}<F>({}) {{", f.name, wparams.join(", "));
                let _ = writeln!(w, "    let r_h = {fname}({});", call_args.join(", "));
                let _ = writeln!(w, "    F(r_h);");
                let _ = writeln!(w, "    release(r_h);");
            }
            let _ = writeln!(w, "}}\n");
        }
        Ok(())
    }

    fn callback_function(&mut self, f: &Function, ci: usize) -> Result<(), String> {
        let ParamKind::Callback { args: cb_args, ret: cb_ret } = &f.params[ci].kind else { unreachable!() };
        let sym = &f.symbol;
        let sess = format!("{}Call", camel(&f.name));
        self.extra_types.insert(sess.clone());
        // start
        let mut c = Call::default();
        for (i, p) in f.params.iter().enumerate() {
            if i != ci {
                lower_param(&mut c, p, &p.kind);
            }
        }
        let mut sdecl = c.decl.clone();
        sdecl.push(format!("out: {sess}&"));
        self.extern_fn(&format!("{sym}__start"), &sdecl, "");
        // next
        let mut ndecl = vec![format!("s: {sess}")];
        for (k, a) in cb_args.iter().enumerate() {
            ndecl.push(match a {
                CbArg::Scalar(t) | CbArg::Value(t) => format!("ev{k}: {}&", ty(t)),
                CbArg::Str => format!("ev{k}: u8[>..]&"),
                CbArg::Handle(h) => format!("ev{k}: {h}&"),
            });
        }
        self.extern_fn(&format!("{sym}__next"), &ndecl, " -> bool");
        if let Some(r) = cb_ret {
            let d = if is_prim(r) { format!("r: {}", ty(r)) } else { format!("r: const {}&", ty(r)) };
            self.extern_fn(&format!("{sym}__reply"), &[format!("s: {sess}"), d], "");
        }
        // finish
        let mut outs = Vec::new();
        ret_outs(&f.ret, false, false, &mut outs);
        let (ret_s, _) = c_ret(&f.ret, false);
        let mut fdecl = vec![format!("s: {sess}")];
        for (d, _, _) in &outs {
            fdecl.push(d.clone());
        }
        self.extern_fn(&format!("{sym}__finish"), &fdecl, &ret_s);

        // The wrapper: the caller's block is the callback.
        let mut w = String::new();
        comment(&mut w, &f.doc, "");
        let _ = writeln!(w, "// The block is called back with each call the Rust function makes.");
        let mut wparams = Vec::new();
        for (i, p) in f.params.iter().enumerate() {
            if i != ci {
                wparams.push(wrapper_param(p)?);
            }
        }
        let fallible_result = matches!(f.ret, RetKind::Result(_));
        if fallible_result {
            wparams.push("err: u8[>..]&".to_string());
        }
        let rets = wrapper_ret(&f.ret, &f.params)?;
        let ret_decl = if rets.is_empty() { String::new() } else { format!(" -> {}", rets.join(", ")) };
        let fname = name(&f.name);
        let _ = writeln!(w, "fn {fname}<F>({}){ret_decl} {{", wparams.join(", "));
        let _ = writeln!(w, "    var r_s = default<{sess}>();");
        let _ = writeln!(w, "    {sym}__start({});", {
            let mut a = c.args.clone();
            a.push("r_s".into());
            a.join(", ")
        });
        let _ = writeln!(w, "    loop {{");
        let mut evs = Vec::new();
        let mut pass = Vec::new();
        for (k, a) in cb_args.iter().enumerate() {
            match a {
                CbArg::Scalar(t) | CbArg::Value(t) => {
                    let _ = writeln!(w, "        var r_ev{k} = default<{}>();", ty(t));
                    pass.push(format!("r_ev{k}"));
                }
                CbArg::Handle(h) => {
                    let _ = writeln!(w, "        var r_ev{k} = default<{h}>();");
                    pass.push(format!("r_ev{k}"));
                }
                CbArg::Str => {
                    let _ = writeln!(w, "        var r_ev{k}: u8[>..] = [];");
                    pass.push(format!("r_ev{k}[0..r_ev{k}.len]"));
                }
            }
            evs.push(format!("r_ev{k}"));
        }
        let mut nargs = vec!["r_s".to_string()];
        nargs.extend(evs);
        let _ = writeln!(w, "        guard {sym}__next({}) else {{ break; }}", nargs.join(", "));
        if cb_ret.is_some() {
            let _ = writeln!(w, "        let r_reply = F({});", pass.join(", "));
            let _ = writeln!(w, "        {sym}__reply(r_s, r_reply);");
        } else {
            let _ = writeln!(w, "        F({});", pass.join(", "));
        }
        let _ = writeln!(w, "    }}");
        ret_locals(&f.ret, false, &mut w, "    ");
        let mut fargs = vec!["r_s".to_string()];
        for (_, _, a) in &outs {
            fargs.push(a.clone());
        }
        let call = format!("{sym}__finish({})", fargs.join(", "));
        match &f.ret {
            RetKind::Unit => {
                let _ = writeln!(w, "    {call};");
            }
            RetKind::Scalar(_) => {
                let _ = writeln!(w, "    return {call};");
            }
            RetKind::Option(_) | RetKind::Result(_) => {
                let _ = writeln!(w, "    let r_ok = {call};");
                match ret_value(&f.ret, &f.params) {
                    Some(v) => {
                        let _ = writeln!(w, "    return {v}, r_ok;");
                    }
                    None => {
                        let _ = writeln!(w, "    return r_ok;");
                    }
                }
            }
            other => {
                let _ = writeln!(w, "    {call};");
                let v = ret_value(other, &f.params).unwrap();
                let _ = writeln!(w, "    return {v};");
            }
        }
        let _ = writeln!(w, "}}\n");
        if fallible_result {
            let plain: Vec<String> = wparams[..wparams.len() - 1].to_vec();
            let RetKind::Result(inner) = &f.ret else { unreachable!() };
            let inner_rets = wrapper_ret(inner, &f.params)?;
            let mut call_args: Vec<String> =
                f.params.iter().enumerate().filter(|(i, _)| *i != ci).map(|(_, p)| name(&p.name)).collect();
            call_args.push("err".into());
            let _ = writeln!(w, "fn {fname}<F>({}){ret_decl} {{", plain.join(", "));
            let _ = writeln!(w, "    var err: u8[>..] = [];");
            if inner_rets.is_empty() {
                let _ = writeln!(w, "    return {fname}({}) {{ r_a => F(r_a) }};", call_args.join(", "));
            } else {
                let _ = writeln!(w, "    let r_v, r_ok = {fname}({});", call_args.join(", "));
                let _ = writeln!(w, "    return r_v, r_ok;");
            }
            let _ = writeln!(w, "}}\n");
        }
        self.wrappers.push_str(&w);
        Ok(())
    }
}

fn ty_uses(t: &Ty, names: &BTreeSet<String>) -> bool {
    match t {
        Ty::Prim(_) => false,
        Ty::Named(n) => names.contains(n),
        Ty::Array { elem, .. } => ty_uses(elem, names),
    }
}

fn param_uses(k: &ParamKind, names: &BTreeSet<String>) -> bool {
    match k {
        ParamKind::Scalar(t)
        | ParamKind::Value(t)
        | ParamKind::Ref(t)
        | ParamKind::RefMut(t)
        | ParamKind::Slice(t)
        | ParamKind::SliceMut(t)
        | ParamKind::TypedBuf(t) => ty_uses(t, names),
        ParamKind::Optional(i) => param_uses(i, names),
        ParamKind::Callback { args, ret } => {
            args.iter().any(|a| matches!(a, CbArg::Value(t) | CbArg::Scalar(t) if ty_uses(t, names)))
                || ret.as_ref().map(|t| ty_uses(t, names)).unwrap_or(false)
        }
        _ => false,
    }
}

fn ret_uses(r: &RetKind, names: &BTreeSet<String>) -> bool {
    match r {
        RetKind::Scalar(t) | RetKind::Value(t) | RetKind::Vec(t) => ty_uses(t, names),
        RetKind::Borrowed { elem: Some(t), .. } | RetKind::Spans { elem: Some(t), .. } => ty_uses(t, names),
        RetKind::Option(i) | RetKind::Result(i) => ret_uses(i, names),
        RetKind::Iter { item, .. } => ret_uses(item, names),
        _ => false,
    }
}

fn fn_uses(f: &Function, names: &BTreeSet<String>) -> bool {
    !names.is_empty() && (f.params.iter().any(|p| param_uses(&p.kind, names)) || ret_uses(&f.ret, names))
}

/// A struct as Goose declares it: rustc's layout, with explicit pads where
/// rustc put padding, when Goose stores it as Rust does; packed otherwise.
pub fn struct_decl(s: &Struct, align: bool) -> String {
    let mut parts = Vec::new();
    if align && s.same_layout && s.align > 1 {
        parts.push(format!("align {}", s.align));
    }
    if s.same_layout {
        let mut fields: Vec<&Field> = s.fields.iter().collect();
        fields.sort_by_key(|f| f.offset);
        let mut cursor = 0usize;
        for f in fields {
            if f.offset > cursor {
                parts.push(format!("pad {}", f.offset - cursor));
            }
            parts.push(format!("{}: {}", name(&f.name), ty(&f.ty)));
            cursor = f.offset + f.goose_size;
        }
        if s.size > cursor {
            parts.push(format!("pad {}", s.size - cursor));
        }
    } else {
        for f in &s.fields {
            parts.push(format!("{}: {}", name(&f.name), ty(&f.ty)));
        }
    }
    let layout = if s.same_layout {
        format!("{} bytes, as rustc lays it out", s.size)
    } else {
        format!("{} bytes packed; Rust's is {} and the bridge converts", s.goose_size, s.size)
    };
    format!("struct {} {{ {} }}   // {layout}\n", s.name, parts.join(", "))
}

pub fn enum_decl(e: &Enum) -> String {
    let aligned = e.same_layout && e.variants.iter().any(|v| !v.fields.is_empty());
    // An aligned enum's payload area is what is left after the tag; each
    // variant is padded to it, with pads where rustc put padding.
    let payload = if aligned { e.size - e.align.max(1) } else { 0 };
    let mut vs: Vec<String> = Vec::new();
    if aligned && e.align > 1 {
        vs.push(format!("align {}", e.align));
    }
    for v in &e.variants {
        if v.fields.is_empty() {
            vs.push(v.name.clone());
            continue;
        }
        let mut fs = Vec::new();
        let mut cursor = 0usize;
        for f in &v.fields {
            if aligned && f.offset > cursor {
                fs.push(format!("pad {}", f.offset - cursor));
            }
            fs.push(format!("{}: {}", name(&f.name), ty(&f.ty)));
            cursor = if aligned { f.offset + f.goose_size } else { cursor + f.goose_size };
        }
        if aligned && payload > cursor {
            fs.push(format!("pad {}", payload - cursor));
        }
        vs.push(format!("{} {{ {} }}", v.name, fs.join(", ")));
    }
    let layout = if aligned {
        format!("{} bytes, as Rust's #[repr(C, u8)]", e.size)
    } else if e.same_layout {
        format!("{} byte, as Rust's", e.goose_size)
    } else {
        format!("{} bytes in Goose, {} in Rust: converted at the boundary", e.goose_size, e.size)
    };
    format!("enum {} {{ {} }}   // {layout}\n", e.name, vs.join(", "))
}
