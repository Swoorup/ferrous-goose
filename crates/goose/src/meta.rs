//! The records `#[goose::export]`, `#[goose::handle]` and
//! `#[derive(goose::Flat)]` leave in a library, and the JSON
//! `goose_metadata` turns them into for `cargo goose bindings`.
//!
//! The macros cannot know a struct's field offsets (rustc computes them after
//! the macros run) or whether a name is a struct or a handle. So each macro
//! registers a function that builds its record when the library is asked,
//! with offsets from `core::mem::offset_of!` and types from the traits.

use std::collections::BTreeMap;
use std::fmt::Write;

/// A Goose type as the bindings spell it.
#[derive(Clone, Debug, PartialEq)]
pub enum Ty {
    /// `i8` ... `u64`, `f32`, `f64`, `bool`.
    Prim(&'static str),
    /// A struct, enum or handle declared in the generated module.
    Named(String),
    /// `T[n]`.
    Array(Box<Ty>, usize),
}

#[derive(Clone, Debug)]
pub struct FieldDef {
    pub name: String,
    pub ty: Ty,
    /// Where rustc put it.
    pub offset: usize,
    /// Where Goose puts it.
    pub goose_offset: usize,
    pub goose_size: usize,
}

#[derive(Clone, Debug)]
pub struct StructDef {
    pub name: String,
    pub doc: String,
    pub size: usize,
    pub align: usize,
    pub goose_size: usize,
    /// Goose stores it exactly as Rust does, explicit pads included.
    pub same_layout: bool,
    pub fields: Vec<FieldDef>,
}

#[derive(Clone, Debug)]
pub struct VariantDef {
    pub name: String,
    /// Offsets are within the payload, after the tag.
    pub fields: Vec<FieldDef>,
    /// Rust's discriminant, for a fieldless enum.
    pub discriminant: i64,
}

#[derive(Clone, Debug)]
pub struct EnumDef {
    pub name: String,
    pub doc: String,
    pub size: usize,
    pub goose_size: usize,
    pub same_layout: bool,
    /// `#[goose(align)]`: Goose lays it out as Rust's `#[repr(C, u8)]` with
    /// `align n` (goose-patches/0007), the payload at `align`; 0 otherwise.
    pub align: usize,
    pub variants: Vec<VariantDef>,
}

#[derive(Clone, Debug)]
pub struct HandleDef {
    pub name: String,
    pub doc: String,
    /// Not Send + Sync: used only on the thread that made it.
    pub pinned: bool,
    pub release: String,
    pub live_count: String,
}

/// How a parameter of an exported function crosses.
#[derive(Clone, Debug)]
pub enum ParamKind {
    /// A scalar by value.
    Scalar(Ty),
    /// A struct or enum by value: `const T&` at the boundary, `T` in the wrapper.
    Value(Ty),
    Ref(Ty),
    RefMut(Ty),
    Slice(Ty),
    SliceMut(Ty),
    /// `&str`, `String`: `const u8[:]`, checked for UTF-8.
    Str,
    /// `&[u8]`, `Vec<u8>`: `const u8[:]`.
    Bytes,
    /// `&mut GooseBuf`: `u8[>..]&`.
    Buf,
    /// `&mut GooseVec<T>`: `T[>..]&` (a typed builder, Phase 2).
    TypedBuf(Ty),
    Handle(String),
    HandleMut(String),
    /// A handle given up: the call takes the object.
    HandleOwned(String),
    /// `Option<..>`: entry points with and without it, and a `T?` wrapper.
    Optional(Box<ParamKind>),
    /// A Rust closure parameter the caller's block stands in for.
    Callback { args: Vec<CbArg>, ret: Option<Ty> },
}

/// An argument a callback receives.
#[derive(Clone, Debug)]
pub enum CbArg {
    Value(Ty),
    Scalar(Ty),
    /// Text, copied into a fresh `u8[>..]` per call.
    Str,
    Handle(String),
}

#[derive(Clone, Debug)]
pub struct ParamDef {
    pub name: String,
    pub kind: ParamKind,
}

/// How a result leaves.
#[derive(Clone, Debug)]
pub enum RetKind {
    Unit,
    Scalar(Ty),
    /// Through an out-pointer.
    Value(Ty),
    Handle(String),
    /// `String`, `Vec<u8>`: appended to a builder.
    Bytes,
    /// `Vec<T>`: a typed builder where the compiler has them, else counted
    /// and copied in two calls.
    Vec(Ty),
    /// A slice of parameter `param`: offset and length, re-sliced in Goose.
    Borrowed { param: usize, elem: Option<Ty> },
    Option(Box<RetKind>),
    Result(Box<RetKind>),
    /// An iterator over parts of parameter `param`: a span list.
    Spans { param: usize, elem: Option<Ty> },
    /// An iterator making values: a handle with a next function.
    Iter { item: Box<RetKind>, handle: String },
}

#[derive(Clone, Debug)]
pub struct FnDef {
    /// The Goose name of the wrapper.
    pub name: String,
    pub symbol: String,
    pub doc: String,
    pub params: Vec<ParamDef>,
    pub ret: RetKind,
    pub throws: bool,
    pub main_thread: bool,
    /// Takes `&goose::Reentry`: the Goose declaration is `reentrant`.
    pub reentrant: bool,
    /// No bridge payload copies on the successful data path.
    pub zero_copy: bool,
    /// The C parameters the shim takes, as the macro lowered them, for
    /// `cargo goose` to check its own lowering against.
    pub c_params: Vec<String>,
    pub c_ret: String,
}

/// Everything one library describes.
#[derive(Clone, Debug, Default)]
pub struct Library {
    pub structs: BTreeMap<String, StructDef>,
    pub enums: BTreeMap<String, EnumDef>,
    pub handles: BTreeMap<String, HandleDef>,
    pub functions: Vec<FnDef>,
    /// Two different types under one Goose name, or the like.
    pub errors: Vec<String>,
}

/// The set the type definitions a library uses are gathered into.
pub type TypeSet = Library;

impl Library {
    pub fn add_struct(&mut self, s: StructDef) -> bool {
        if self.structs.contains_key(&s.name) {
            return false;
        }
        self.structs.insert(s.name.clone(), s);
        true
    }

    pub fn add_enum(&mut self, e: EnumDef) -> bool {
        if self.enums.contains_key(&e.name) {
            return false;
        }
        self.enums.insert(e.name.clone(), e);
        true
    }

    pub fn has_type(&self, name: &str) -> bool {
        self.structs.contains_key(name) || self.enums.contains_key(name) || self.handles.contains_key(name)
    }
}

/// One record, registered by a macro with `inventory`.
pub struct Entry {
    pub add: fn(&mut Library),
}

inventory::collect!(Entry);

/// The library's records, every macro's added.
pub fn library() -> Library {
    let mut lib = Library::default();
    for e in inventory::iter::<Entry> {
        (e.add)(&mut lib);
    }
    lib.functions.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    lib
}

// ---- JSON ----

pub(crate) fn json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn ty_json(out: &mut String, t: &Ty) {
    match t {
        Ty::Prim(p) => {
            out.push_str("{\"prim\":");
            json_str(out, p);
            out.push('}');
        }
        Ty::Named(n) => {
            out.push_str("{\"named\":");
            json_str(out, n);
            out.push('}');
        }
        Ty::Array(e, n) => {
            out.push_str("{\"array\":{\"elem\":");
            ty_json(out, e);
            let _ = write!(out, ",\"len\":{n}}}}}");
        }
    }
}

fn opt_ty_json(out: &mut String, t: &Option<Ty>) {
    match t {
        Some(t) => ty_json(out, t),
        None => out.push_str("null"),
    }
}

fn fields_json(out: &mut String, fields: &[FieldDef]) {
    out.push('[');
    for (i, f) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"name\":");
        json_str(out, &f.name);
        out.push_str(",\"ty\":");
        ty_json(out, &f.ty);
        let _ = write!(
            out,
            ",\"offset\":{},\"goose_offset\":{},\"goose_size\":{}}}",
            f.offset, f.goose_offset, f.goose_size
        );
    }
    out.push(']');
}

fn param_json(out: &mut String, k: &ParamKind) {
    let simple = |out: &mut String, tag: &str, t: &Ty| {
        let _ = write!(out, "{{\"{tag}\":");
        ty_json(out, t);
        out.push('}');
    };
    let named = |out: &mut String, tag: &str, n: &str| {
        let _ = write!(out, "{{\"{tag}\":");
        json_str(out, n);
        out.push('}');
    };
    match k {
        ParamKind::Scalar(t) => simple(out, "scalar", t),
        ParamKind::Value(t) => simple(out, "value", t),
        ParamKind::Ref(t) => simple(out, "ref", t),
        ParamKind::RefMut(t) => simple(out, "ref_mut", t),
        ParamKind::Slice(t) => simple(out, "slice", t),
        ParamKind::SliceMut(t) => simple(out, "slice_mut", t),
        ParamKind::Str => out.push_str("\"str\""),
        ParamKind::Bytes => out.push_str("\"bytes\""),
        ParamKind::Buf => out.push_str("\"buf\""),
        ParamKind::TypedBuf(t) => simple(out, "typed_buf", t),
        ParamKind::Handle(n) => named(out, "handle", n),
        ParamKind::HandleMut(n) => named(out, "handle_mut", n),
        ParamKind::HandleOwned(n) => named(out, "handle_owned", n),
        ParamKind::Optional(inner) => {
            out.push_str("{\"optional\":");
            param_json(out, inner);
            out.push('}');
        }
        ParamKind::Callback { args, ret } => {
            out.push_str("{\"callback\":{\"args\":[");
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                match a {
                    CbArg::Value(t) => simple(out, "value", t),
                    CbArg::Scalar(t) => simple(out, "scalar", t),
                    CbArg::Str => out.push_str("\"str\""),
                    CbArg::Handle(n) => named(out, "handle", n),
                }
            }
            out.push_str("],\"ret\":");
            opt_ty_json(out, ret);
            out.push_str("}}");
        }
    }
}

fn ret_json(out: &mut String, r: &RetKind) {
    match r {
        RetKind::Unit => out.push_str("\"unit\""),
        RetKind::Scalar(t) => {
            out.push_str("{\"scalar\":");
            ty_json(out, t);
            out.push('}');
        }
        RetKind::Value(t) => {
            out.push_str("{\"value\":");
            ty_json(out, t);
            out.push('}');
        }
        RetKind::Handle(n) => {
            out.push_str("{\"handle\":");
            json_str(out, n);
            out.push('}');
        }
        RetKind::Bytes => out.push_str("\"bytes\""),
        RetKind::Vec(t) => {
            out.push_str("{\"vec\":");
            ty_json(out, t);
            out.push('}');
        }
        RetKind::Borrowed { param, elem } => {
            let _ = write!(out, "{{\"borrowed\":{{\"param\":{param},\"elem\":");
            opt_ty_json(out, elem);
            out.push_str("}}");
        }
        RetKind::Option(inner) => {
            out.push_str("{\"option\":");
            ret_json(out, inner);
            out.push('}');
        }
        RetKind::Result(inner) => {
            out.push_str("{\"result\":");
            ret_json(out, inner);
            out.push('}');
        }
        RetKind::Spans { param, elem } => {
            let _ = write!(out, "{{\"spans\":{{\"param\":{param},\"elem\":");
            opt_ty_json(out, elem);
            out.push_str("}}");
        }
        RetKind::Iter { item, handle } => {
            out.push_str("{\"iter\":{\"handle\":");
            json_str(out, handle);
            out.push_str(",\"item\":");
            ret_json(out, item);
            out.push_str("}}");
        }
    }
}

/// The library's records as JSON, with the versions a reader checks first.
pub fn to_json(lib: &Library, crate_name: &str, crate_version: &str, namespace: &str) -> String {
    let mut o = String::new();
    let _ = write!(
        o,
        "{{\"format\":{},\"abi\":{},\"bridge\":",
        crate::abi::METADATA_FORMAT,
        crate::abi::ABI_VERSION
    );
    json_str(&mut o, env!("CARGO_PKG_VERSION"));
    o.push_str(",\"goose_runtime\":");
    json_str(&mut o, crate::abi::GOOSE_RUNTIME);
    o.push_str(",\"crate\":");
    json_str(&mut o, crate_name);
    o.push_str(",\"version\":");
    json_str(&mut o, crate_version);
    o.push_str(",\"namespace\":");
    json_str(&mut o, namespace);
    o.push_str(",\"structs\":[");
    for (i, s) in lib.structs.values().enumerate() {
        if i > 0 {
            o.push(',');
        }
        o.push_str("{\"name\":");
        json_str(&mut o, &s.name);
        o.push_str(",\"doc\":");
        json_str(&mut o, &s.doc);
        let _ = write!(
            o,
            ",\"size\":{},\"align\":{},\"goose_size\":{},\"same_layout\":{},\"fields\":",
            s.size, s.align, s.goose_size, s.same_layout
        );
        fields_json(&mut o, &s.fields);
        o.push('}');
    }
    o.push_str("],\"enums\":[");
    for (i, e) in lib.enums.values().enumerate() {
        if i > 0 {
            o.push(',');
        }
        o.push_str("{\"name\":");
        json_str(&mut o, &e.name);
        o.push_str(",\"doc\":");
        json_str(&mut o, &e.doc);
        let _ = write!(
            o,
            ",\"size\":{},\"goose_size\":{},\"same_layout\":{},\"align\":{},\"variants\":[",
            e.size, e.goose_size, e.same_layout, e.align
        );
        for (j, v) in e.variants.iter().enumerate() {
            if j > 0 {
                o.push(',');
            }
            o.push_str("{\"name\":");
            json_str(&mut o, &v.name);
            let _ = write!(o, ",\"discriminant\":{},\"fields\":", v.discriminant);
            fields_json(&mut o, &v.fields);
            o.push('}');
        }
        o.push_str("]}");
    }
    o.push_str("],\"handles\":[");
    for (i, h) in lib.handles.values().enumerate() {
        if i > 0 {
            o.push(',');
        }
        o.push_str("{\"name\":");
        json_str(&mut o, &h.name);
        o.push_str(",\"doc\":");
        json_str(&mut o, &h.doc);
        let _ = write!(o, ",\"pinned\":{},\"release\":", h.pinned);
        json_str(&mut o, &h.release);
        o.push_str(",\"live_count\":");
        json_str(&mut o, &h.live_count);
        o.push('}');
    }
    o.push_str("],\"functions\":[");
    for (i, f) in lib.functions.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        o.push_str("{\"name\":");
        json_str(&mut o, &f.name);
        o.push_str(",\"symbol\":");
        json_str(&mut o, &f.symbol);
        o.push_str(",\"doc\":");
        json_str(&mut o, &f.doc);
        o.push_str(",\"params\":[");
        for (j, p) in f.params.iter().enumerate() {
            if j > 0 {
                o.push(',');
            }
            o.push_str("{\"name\":");
            json_str(&mut o, &p.name);
            o.push_str(",\"kind\":");
            param_json(&mut o, &p.kind);
            o.push('}');
        }
        o.push_str("],\"ret\":");
        ret_json(&mut o, &f.ret);
        let _ = write!(
            o,
            ",\"throws\":{},\"main_thread\":{},\"reentrant\":{},\"zero_copy\":{},\"c_params\":[",
            f.throws, f.main_thread, f.reentrant, f.zero_copy
        );
        for (j, c) in f.c_params.iter().enumerate() {
            if j > 0 {
                o.push(',');
            }
            json_str(&mut o, c);
        }
        o.push_str("],\"c_ret\":");
        json_str(&mut o, &f.c_ret);
        o.push('}');
    }
    o.push_str("],\"errors\":[");
    for (i, e) in lib.errors.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        json_str(&mut o, e);
    }
    o.push_str("]}");
    o
}
