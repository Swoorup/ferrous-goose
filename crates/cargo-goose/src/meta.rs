//! The records a Ferrous Goose library describes itself with, as
//! `goose_metadata` writes them (the `goose` crate's `meta` module), and
//! how they are read out of a built library.

use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Ty {
    Prim(String),
    Named(String),
    Array { elem: Box<Ty>, len: usize },
}

#[derive(Deserialize, Clone, Debug)]
pub struct Field {
    pub name: String,
    pub ty: Ty,
    pub offset: usize,
    pub goose_offset: usize,
    pub goose_size: usize,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Struct {
    pub name: String,
    pub doc: String,
    pub size: usize,
    pub align: usize,
    pub goose_size: usize,
    pub same_layout: bool,
    pub fields: Vec<Field>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Variant {
    pub name: String,
    pub discriminant: i64,
    pub fields: Vec<Field>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Enum {
    pub name: String,
    pub doc: String,
    pub size: usize,
    pub goose_size: usize,
    pub same_layout: bool,
    /// `#[goose(align)]`: where the payload starts; 0 for a converted enum.
    #[serde(default)]
    pub align: usize,
    pub variants: Vec<Variant>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct HandleDef {
    pub name: String,
    pub doc: String,
    pub pinned: bool,
    pub release: String,
    pub live_count: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum CbArg {
    Value(Ty),
    Scalar(Ty),
    Str,
    Handle(String),
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ParamKind {
    Scalar(Ty),
    Value(Ty),
    Ref(Ty),
    RefMut(Ty),
    Slice(Ty),
    SliceMut(Ty),
    Str,
    Bytes,
    Buf,
    TypedBuf(Ty),
    Handle(String),
    HandleMut(String),
    HandleOwned(String),
    Optional(Box<ParamKind>),
    Callback { args: Vec<CbArg>, ret: Option<Ty> },
}

#[derive(Deserialize, Clone, Debug)]
pub struct Param {
    pub name: String,
    pub kind: ParamKind,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum RetKind {
    Unit,
    Scalar(Ty),
    Value(Ty),
    Handle(String),
    Bytes,
    Vec(Ty),
    Borrowed { param: usize, elem: Option<Ty> },
    Option(Box<RetKind>),
    Result(Box<RetKind>),
    Spans { param: usize, elem: Option<Ty> },
    Iter { handle: String, item: Box<RetKind> },
}

#[derive(Deserialize, Clone, Debug)]
pub struct Function {
    pub name: String,
    pub symbol: String,
    pub doc: String,
    pub params: Vec<Param>,
    pub ret: RetKind,
    pub throws: bool,
    pub main_thread: bool,
    #[serde(default)]
    pub reentrant: bool,
    pub c_params: Vec<String>,
    pub c_ret: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Library {
    pub format: u32,
    pub abi: u32,
    pub bridge: String,
    pub goose_runtime: String,
    #[serde(rename = "crate")]
    pub crate_name: String,
    pub version: String,
    pub namespace: String,
    pub structs: Vec<Struct>,
    pub enums: Vec<Enum>,
    pub handles: Vec<HandleDef>,
    pub functions: Vec<Function>,
    pub errors: Vec<String>,
}

/// The metadata format and bridge ABI this cargo-goose reads and writes for.
pub const FORMAT: u32 = 1;
pub const ABI: u32 = 1;

/// Parses metadata JSON, checking its versions first.
pub fn parse(json: &str, source: &str) -> Result<Library, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("{source}: metadata is not JSON: {e}"))?;
    let format = v.get("format").and_then(|f| f.as_u64()).unwrap_or(0) as u32;
    let abi = v.get("abi").and_then(|f| f.as_u64()).unwrap_or(0) as u32;
    let bridge = v.get("bridge").and_then(|f| f.as_str()).unwrap_or("?");
    if format != FORMAT || abi != ABI {
        return Err(format!(
            "{source} was built with Ferrous Goose {bridge} (metadata format {format}, bridge ABI {abi}); \
             this cargo-goose {} reads format {FORMAT}, ABI {ABI}. Rebuild the crate against a matching `goose` crate.",
            env!("CARGO_PKG_VERSION")
        ));
    }
    let lib: Library = serde_json::from_value(v).map_err(|e| format!("{source}: unexpected metadata: {e}"))?;
    if !lib.errors.is_empty() {
        return Err(format!("{source}: {}", lib.errors.join("; ")));
    }
    Ok(lib)
}

/// Opens a built library and asks it for its records.
pub fn read_library(path: &Path) -> Result<Library, String> {
    let shown = path.display().to_string();
    // The library's own constructors (inventory's) run on load: that is
    // the code of the crate being bound, which the user asked to build.
    let lib = unsafe { libloading::Library::new(path) }.map_err(|e| format!("cannot open {shown}: {e}"))?;
    let f: libloading::Symbol<unsafe extern "C" fn(*mut usize) -> *const u8> = unsafe { lib.get(b"goose_metadata\0") }
        .map_err(|_| format!("{shown} has no goose_metadata: is it a crate using #[goose::export]?"))?;
    let mut len = 0usize;
    let p = unsafe { f(&mut len) };
    let bytes = unsafe { std::slice::from_raw_parts(p, len) };
    let json = String::from_utf8_lossy(bytes).into_owned();
    // The library stays loaded: its static JSON is what we read, and
    // unloading Rust libraries with thread-locals is not reliable.
    std::mem::forget(lib);
    parse(&json, &shown)
}
