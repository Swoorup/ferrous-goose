//! A Goose program's C interface: its export fns and the layouts of the
//! types they reach. Read from the compiler's `--emit-interface` JSON
//! (goose-patches/0003), or, with a compiler without it, parsed from the
//! sources for the shapes a stock export takes.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GTy {
    Prim(String),
    Struct(String),
    Enum(String),
    Array {
        elem: Box<GTy>,
        len: usize,
    },
    Slice {
        elem: Box<GTy>,
        #[serde(rename = "const")]
        constant: bool,
    },
    Ref {
        to: Box<GTy>,
        #[serde(rename = "const")]
        constant: bool,
    },
    Builder {
        elem: Box<GTy>,
    },
    Other(String),
}

#[derive(Deserialize, Clone, Debug)]
pub struct Param {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: GTy,
    #[serde(default)]
    pub text: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Res {
    #[serde(rename = "type")]
    pub ty: GTy,
    #[serde(default)]
    pub text: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Export {
    pub name: String,
    #[serde(default)]
    pub qname: String,
    pub symbol: String,
    #[serde(default)]
    pub doc: String,
    pub params: Vec<Param>,
    pub results: Vec<Res>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum Field {
    Pad { pad: usize, offset: usize },
    Named { name: String, #[serde(rename = "type")] ty: GTy, offset: usize, size: usize },
}

#[derive(Deserialize, Clone, Debug)]
pub struct Struct {
    pub name: String,
    pub qname: String,
    #[serde(default)]
    pub doc: String,
    pub size: usize,
    #[serde(default = "one")]
    pub align: usize,
    pub fields: Vec<Field>,
}

fn one() -> usize {
    1
}

#[derive(Deserialize, Clone, Debug)]
pub struct Variant {
    pub name: String,
    pub tag: usize,
    pub fields: Vec<Field>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Enum {
    pub name: String,
    pub qname: String,
    #[serde(default)]
    pub doc: String,
    pub tag_size: usize,
    pub size: usize,
    #[serde(default = "one")]
    pub align: usize,
    pub variants: Vec<Variant>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct Interface {
    #[serde(default)]
    pub sources: Vec<String>,
    pub exports: Vec<Export>,
    pub structs: Vec<Struct>,
    pub enums: Vec<Enum>,
}

pub fn parse_json(text: &str) -> Result<Interface, String> {
    serde_json::from_str(text).map_err(|e| format!("the compiler's interface JSON: {e}"))
}

// ---- the fallback: reading the sources ----

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Int(i64),
    Str(String),
    Punct(String),
}

fn lex(src: &str) -> Vec<Tok> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() {
            i += 1;
        } else if src[i..].starts_with("//") {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if src[i..].starts_with("/*") {
            i += 2;
            while i + 1 < b.len() && !src[i..].starts_with("*/") {
                i += 1;
            }
            i += 2;
        } else if c.is_ascii_alphabetic() || c == '_' {
            let s = i;
            while i < b.len() && ((b[i] as char).is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(Tok::Ident(src[s..i].to_string()));
        } else if c.is_ascii_digit() {
            let s = i;
            while i < b.len() && ((b[i] as char).is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'.') {
                if b[i] == b'.' && i + 1 < b.len() && b[i + 1] == b'.' {
                    break;
                }
                i += 1;
            }
            let t = src[s..i].replace('_', "");
            let v = if let Some(h) = t.strip_prefix("0x") { i64::from_str_radix(h, 16).unwrap_or(0) } else { t.parse().unwrap_or(0) };
            out.push(Tok::Int(v));
        } else if c == '"' || c == '\'' {
            let q = b[i];
            let s = i + 1;
            i += 1;
            while i < b.len() && b[i] != q {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            out.push(Tok::Str(src[s..i.min(b.len())].to_string()));
            i += 1;
        } else {
            for p in ["->", "::", "..", ">.."] {
                let _ = p;
            }
            let two = if i + 1 < b.len() { &src[i..i + 2] } else { "" };
            if two == "->" || two == "::" || two == ".." {
                out.push(Tok::Punct(two.to_string()));
                i += 2;
            } else {
                out.push(Tok::Punct(c.to_string()));
                i += 1;
            }
        }
    }
    out
}

struct Parser {
    toks: Vec<Tok>,
    i: usize,
    ns: String,
}

/// A type as written, before resolution.
#[derive(Clone, Debug)]
enum TyText {
    Name(String),
    Array(Box<TyText>, usize),
    Slice(Box<TyText>, bool),
    Ref(Box<TyText>, bool),
    Grow(Box<TyText>),
}

#[derive(Clone, Debug)]
enum FieldText {
    Pad(Option<usize>),
    Named(String, TyText),
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i)
    }
    fn at(&self, p: &str) -> bool {
        matches!(self.peek(), Some(Tok::Punct(x)) if x == p)
    }
    fn at_ident(&self, s: &str) -> bool {
        matches!(self.peek(), Some(Tok::Ident(x)) if x == s)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.i).cloned();
        self.i += 1;
        t
    }
    fn ident(&mut self) -> Option<String> {
        match self.next() {
            Some(Tok::Ident(s)) => Some(s),
            _ => None,
        }
    }
    fn eat(&mut self, p: &str) -> bool {
        if self.at(p) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    /// Skips a balanced `{ ... }` starting at the current `{`.
    fn skip_block(&mut self) {
        let mut depth = 0;
        while let Some(t) = self.next() {
            match t {
                Tok::Punct(p) if p == "{" => depth += 1,
                Tok::Punct(p) if p == "}" => {
                    depth -= 1;
                    if depth <= 0 {
                        return;
                    }
                }
                _ => {}
            }
        }
    }
    fn qualified(&mut self, first: String) -> String {
        if self.eat("::") {
            let leaf = self.ident().unwrap_or_default();
            if first.is_empty() {
                return leaf;
            }
            return format!("{first}::{leaf}");
        }
        first
    }
    fn ty(&mut self) -> Option<TyText> {
        let constq = if self.at_ident("const") {
            self.i += 1;
            true
        } else {
            false
        };
        let base = match self.next()? {
            Tok::Ident(n) => {
                let q = self.qualified(n);
                TyText::Name(q)
            }
            Tok::Punct(p) if p == "::" => {
                let n = self.ident()?;
                TyText::Name(n)
            }
            _ => return None,
        };
        let mut t = base;
        let mut first = true;
        loop {
            if self.at("[") {
                self.i += 1;
                if self.eat(":") {
                    self.eat("]");
                    t = TyText::Slice(Box::new(t), constq && first);
                    first = false;
                    continue;
                }
                if self.at(">") {
                    // [>..]
                    while !self.eat("]") && self.peek().is_some() {
                        self.i += 1;
                    }
                    t = TyText::Grow(Box::new(t));
                    continue;
                }
                if let Some(Tok::Int(n)) = self.peek().cloned() {
                    self.i += 1;
                    self.eat("]");
                    t = TyText::Array(Box::new(t), n as usize);
                    continue;
                }
                // Other arrays: skip to ].
                while !self.eat("]") && self.peek().is_some() {
                    self.i += 1;
                }
                t = TyText::Name("<array>".into());
                continue;
            }
            if self.eat("&") {
                t = TyText::Ref(Box::new(t), constq && first);
                first = false;
                continue;
            }
            break;
        }
        Some(t)
    }
}

struct Source {
    path: PathBuf,
    ns: String,
}

/// Parses the program's sources for its exports and struct declarations,
/// following its imports.
pub fn parse_sources(root: &Path) -> Result<Interface, String> {
    let root_dir = root.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut queue = vec![root.to_path_buf()];
    let mut seen = std::collections::BTreeSet::new();
    let mut structs_text: BTreeMap<String, (String, Vec<FieldText>, usize)> = BTreeMap::new();
    let mut enums_text: BTreeMap<String, (String, Vec<(String, Vec<FieldText>)>)> = BTreeMap::new();
    let mut exports_text: Vec<(String, String, String, Vec<(String, TyText)>, Vec<TyText>)> = Vec::new();
    let mut sources = Vec::new();
    while let Some(path) = queue.pop() {
        let key = std::fs::canonicalize(&path).unwrap_or(path.clone());
        if !seen.insert(key) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        sources.push(path.to_string_lossy().into_owned());
        let mut p = Parser { toks: lex(&text), i: 0, ns: String::new() };
        let src = Source { path: path.clone(), ns: String::new() };
        let _ = &src.ns;
        while let Some(t) = p.peek().cloned() {
            match t {
                Tok::Ident(k) if k == "namespace" => {
                    p.i += 1;
                    p.ns = p.ident().unwrap_or_default();
                    p.eat(";");
                }
                Tok::Ident(k) if k == "import" => {
                    p.i += 1;
                    let rel = p.eat(".");
                    let mut parts = Vec::new();
                    while let Some(Tok::Ident(n)) = p.peek().cloned() {
                        parts.push(n);
                        p.i += 1;
                        if !p.eat(".") {
                            break;
                        }
                    }
                    let from = p.at_ident("from");
                    while !p.eat(";") && p.peek().is_some() {
                        p.i += 1;
                    }
                    if from || parts.is_empty() {
                        continue;
                    }
                    let base = if rel { src.path.parent().map(Path::to_path_buf).unwrap_or_default() } else { root_dir.clone() };
                    let mut f = base;
                    for (j, part) in parts.iter().enumerate() {
                        if j + 1 == parts.len() {
                            f.push(format!("{part}.goose"));
                        } else {
                            f.push(part);
                        }
                    }
                    if f.is_file() {
                        queue.push(f);
                    }
                }
                Tok::Ident(k) if k == "struct" => {
                    p.i += 1;
                    let name = p.ident().unwrap_or_default();
                    let name = p.qualified(name);
                    if p.at("<") {
                        // Generic: not something an export names concretely here.
                        while !p.at("{") && p.peek().is_some() {
                            p.i += 1;
                        }
                        p.skip_block();
                        continue;
                    }
                    if !p.eat("{") {
                        continue;
                    }
                    let mut fields = Vec::new();
                    let mut align = 1;
                    while !p.at("}") && p.peek().is_some() {
                        if p.at_ident("pad") {
                            p.i += 1;
                            let n = if let Some(Tok::Int(n)) = p.peek().cloned() {
                                p.i += 1;
                                Some(n as usize)
                            } else {
                                None
                            };
                            fields.push(FieldText::Pad(n));
                        } else if p.at_ident("align") && matches!(p.toks.get(p.i + 1), Some(Tok::Int(_))) {
                            p.i += 1;
                            if let Some(Tok::Int(n)) = p.next() {
                                align = n as usize;
                            }
                        } else {
                            if p.at_ident("let") || p.at_ident("const") {
                                p.i += 1;
                            }
                            let fname = p.ident().unwrap_or_default();
                            p.eat(":");
                            let ft = p.ty().unwrap_or(TyText::Name("<unknown>".into()));
                            // A default value: skip to the next comma at this level.
                            if p.eat("=") {
                                let mut depth = 0;
                                while let Some(t) = p.peek() {
                                    match t {
                                        Tok::Punct(x) if (x == "," || x == "}") && depth == 0 => break,
                                        Tok::Punct(x) if x == "(" || x == "[" || x == "{" => depth += 1,
                                        Tok::Punct(x) if x == ")" || x == "]" || x == "}" => depth -= 1,
                                        _ => {}
                                    }
                                    p.i += 1;
                                }
                            }
                            fields.push(FieldText::Named(fname, ft));
                        }
                        if !p.eat(",") {
                            break;
                        }
                    }
                    p.eat("}");
                    let q = if name.contains("::") || p.ns.is_empty() { name.clone() } else { format!("{}::{name}", p.ns) };
                    structs_text.insert(q, (name.rsplit("::").next().unwrap_or(&name).to_string(), fields, align));
                }
                Tok::Ident(k) if k == "enum" => {
                    p.i += 1;
                    let name = p.ident().unwrap_or_default();
                    let name = p.qualified(name);
                    if !p.eat("{") {
                        continue;
                    }
                    let mut variants = Vec::new();
                    while !p.at("}") && p.peek().is_some() {
                        if p.at_ident("align") {
                            p.i += 2;
                            p.eat(",");
                            continue;
                        }
                        let vname = p.ident().unwrap_or_default();
                        let mut vf = Vec::new();
                        if p.eat("{") {
                            while !p.at("}") && p.peek().is_some() {
                                if p.at_ident("pad") {
                                    p.i += 1;
                                    let n = if let Some(Tok::Int(n)) = p.peek().cloned() {
                                        p.i += 1;
                                        Some(n as usize)
                                    } else {
                                        None
                                    };
                                    vf.push(FieldText::Pad(n));
                                } else {
                                    let fname = p.ident().unwrap_or_default();
                                    p.eat(":");
                                    let ft = p.ty().unwrap_or(TyText::Name("<unknown>".into()));
                                    vf.push(FieldText::Named(fname, ft));
                                }
                                if !p.eat(",") {
                                    break;
                                }
                            }
                            p.eat("}");
                        }
                        variants.push((vname, vf));
                        if !p.eat(",") {
                            break;
                        }
                    }
                    p.eat("}");
                    let q = if name.contains("::") || p.ns.is_empty() { name.clone() } else { format!("{}::{name}", p.ns) };
                    enums_text.insert(q, (name.rsplit("::").next().unwrap_or(&name).to_string(), variants));
                }
                Tok::Ident(k) if k == "export" => {
                    p.i += 1;
                    let mut sym = None;
                    if let Some(Tok::Str(s)) = p.peek().cloned() {
                        p.i += 1;
                        sym = Some(s);
                    }
                    while !p.at_ident("fn") && p.peek().is_some() {
                        p.i += 1;
                    }
                    p.i += 1;
                    let name = p.ident().unwrap_or_default();
                    let name = p.qualified(name);
                    p.eat("(");
                    let mut params = Vec::new();
                    while !p.at(")") && p.peek().is_some() {
                        if p.at_ident("var") {
                            p.i += 1;
                        }
                        let pname = p.ident().unwrap_or_default();
                        p.eat(":");
                        let pt = p.ty().unwrap_or(TyText::Name("<unknown>".into()));
                        params.push((pname, pt));
                        if !p.eat(",") {
                            break;
                        }
                    }
                    p.eat(")");
                    let mut rets = Vec::new();
                    if p.eat("->") {
                        loop {
                            if let Some(t) = p.ty() {
                                rets.push(t);
                            }
                            if !p.eat(",") {
                                break;
                            }
                        }
                    }
                    while !p.at("{") && p.peek().is_some() {
                        p.i += 1;
                    }
                    p.skip_block();
                    let leaf = name.rsplit("::").next().unwrap_or(&name).to_string();
                    let symbol = sym.unwrap_or_else(|| leaf.clone());
                    exports_text.push((leaf, p.ns.clone(), symbol, params, rets));
                }
                Tok::Ident(k) if k == "fn" || k == "extern" || k == "thread_fn" => {
                    // Skip to the body or the `;`.
                    p.i += 1;
                    while !p.at("{") && !p.at(";") && p.peek().is_some() {
                        p.i += 1;
                    }
                    if p.at("{") {
                        p.skip_block();
                    } else {
                        p.i += 1;
                    }
                }
                Tok::Punct(x) if x == "{" => p.skip_block(),
                _ => p.i += 1,
            }
        }
    }

    // Resolution and layout.
    let mut iface = Interface { sources, ..Default::default() };
    let mut layouts: BTreeMap<String, (usize, usize)> = BTreeMap::new(); // qname -> (size, align)
    let resolve_name = |n: &str, ns: &str| -> String {
        if structs_text.contains_key(n) || enums_text.contains_key(n) {
            return n.to_string();
        }
        let q = format!("{ns}::{n}");
        if structs_text.contains_key(&q) || enums_text.contains_key(&q) {
            return q;
        }
        n.to_string()
    };
    fn prim_size(n: &str) -> Option<usize> {
        Some(match n {
            "i8" | "u8" | "bool" => 1,
            "i16" | "u16" => 2,
            "i32" | "u32" | "f32" => 4,
            "i64" | "u64" | "f64" => 8,
            _ => return None,
        })
    }
    fn to_gty(t: &TyText, ns: &str, resolve: &dyn Fn(&str, &str) -> String, enums: &BTreeMap<String, (String, Vec<(String, Vec<FieldText>)>)>) -> GTy {
        match t {
            TyText::Name(n) => {
                if prim_size(n).is_some() {
                    GTy::Prim(n.clone())
                } else {
                    let q = resolve(n, ns);
                    if enums.contains_key(&q) {
                        GTy::Enum(q)
                    } else {
                        GTy::Struct(q)
                    }
                }
            }
            TyText::Array(e, n) => GTy::Array { elem: Box::new(to_gty(e, ns, resolve, enums)), len: *n },
            TyText::Slice(e, c) => GTy::Slice { elem: Box::new(to_gty(e, ns, resolve, enums)), constant: *c },
            TyText::Ref(inner, c) => {
                if let TyText::Grow(e) = &**inner {
                    GTy::Builder { elem: Box::new(to_gty(e, ns, resolve, enums)) }
                } else {
                    GTy::Ref { to: Box::new(to_gty(inner, ns, resolve, enums)), constant: *c }
                }
            }
            TyText::Grow(e) => GTy::Other(format!("{:?}[>..]", e)),
        }
    }
    // Sizes, computed on demand with recursion through names.
    fn size_of(
        t: &GTy,
        structs: &BTreeMap<String, (String, Vec<FieldText>, usize)>,
        enums: &BTreeMap<String, (String, Vec<(String, Vec<FieldText>)>)>,
        layouts: &mut BTreeMap<String, (usize, usize)>,
        resolve: &dyn Fn(&str, &str) -> String,
        out: &mut Interface,
    ) -> usize {
        match t {
            GTy::Prim(p) => prim_size(p).unwrap_or(0),
            GTy::Array { elem, len } => len * size_of(elem, structs, enums, layouts, resolve, out),
            GTy::Slice { .. } => 16,
            GTy::Ref { .. } => 8,
            GTy::Builder { .. } => 16,
            GTy::Struct(q) => {
                if let Some(l) = layouts.get(q) {
                    return l.0;
                }
                let Some((leaf, fields, align)) = structs.get(q).cloned() else { return 0 };
                let ns = q.rsplit_once("::").map(|x| x.0.to_string()).unwrap_or_default();
                let mut off = 0usize;
                let mut fs = Vec::new();
                let mut i = 0;
                while i < fields.len() {
                    match &fields[i] {
                        FieldText::Pad(Some(n)) => {
                            fs.push(Field::Pad { pad: *n, offset: off });
                            off += n;
                        }
                        FieldText::Pad(None) => {
                            // Aligns the next field to its scalar width.
                            let next = fields[i + 1..].iter().find_map(|f| if let FieldText::Named(_, t) = f { Some(t.clone()) } else { None });
                            let a = match next {
                                Some(TyText::Name(n)) => prim_size(&n).unwrap_or(1),
                                Some(TyText::Slice(..)) | Some(TyText::Ref(..)) => 8,
                                _ => 1,
                            };
                            let to = off.div_ceil(a) * a;
                            fs.push(Field::Pad { pad: to - off, offset: off });
                            off = to;
                        }
                        FieldText::Named(n, t) => {
                            let g = to_gty(t, &ns, resolve, enums);
                            let sz = size_of(&g, structs, enums, layouts, resolve, out);
                            fs.push(Field::Named { name: n.clone(), ty: g, offset: off, size: sz });
                            off += sz;
                        }
                    }
                    i += 1;
                }
                let size = off.max(1);
                layouts.insert(q.clone(), (size, align));
                out.structs.push(Struct { name: leaf, qname: q.clone(), doc: String::new(), size, align, fields: fs });
                size
            }
            GTy::Enum(q) => {
                if let Some(l) = layouts.get(q) {
                    return l.0;
                }
                let Some((leaf, variants)) = enums.get(q).cloned() else { return 0 };
                let ns = q.rsplit_once("::").map(|x| x.0.to_string()).unwrap_or_default();
                let mut vs = Vec::new();
                let mut maxp = 0;
                for (k, (vn, vf)) in variants.iter().enumerate() {
                    let mut off = 0;
                    let mut fs = Vec::new();
                    for f in vf {
                        match f {
                            FieldText::Pad(n) => {
                                let n = n.unwrap_or(0);
                                fs.push(Field::Pad { pad: n, offset: off });
                                off += n;
                            }
                            FieldText::Named(n, t) => {
                                let g = to_gty(t, &ns, resolve, enums);
                                let sz = size_of(&g, structs, enums, layouts, resolve, out);
                                fs.push(Field::Named { name: n.clone(), ty: g, offset: off, size: sz });
                                off += sz;
                            }
                        }
                    }
                    maxp = maxp.max(off);
                    vs.push(Variant { name: vn.clone(), tag: k, fields: fs });
                }
                let tag = if variants.len() <= 256 { 1 } else { 2 };
                let size = tag + maxp;
                layouts.insert(q.clone(), (size, 1));
                out.enums.push(Enum { name: leaf, qname: q.clone(), doc: String::new(), tag_size: tag, size, align: 1, variants: vs });
                size
            }
            GTy::Other(_) => 0,
        }
    }
    // The types an export names, behind references and slices too.
    fn reached(t: &GTy) -> Vec<GTy> {
        match t {
            GTy::Ref { to, .. } => reached(to),
            GTy::Slice { elem, .. } | GTy::Builder { elem } | GTy::Array { elem, .. } => reached(elem),
            other => vec![other.clone()],
        }
    }
    for (leaf, ns, symbol, params, rets) in &exports_text {
        let mut ps = Vec::new();
        for (n, t) in params {
            let g = to_gty(t, ns, &resolve_name, &enums_text);
            for r in reached(&g) {
                size_of(&r, &structs_text, &enums_text, &mut layouts, &resolve_name, &mut iface);
            }
            ps.push(Param { name: n.clone(), ty: g, text: String::new() });
        }
        let mut rs = Vec::new();
        for t in rets {
            let g = to_gty(t, ns, &resolve_name, &enums_text);
            for r in reached(&g) {
                size_of(&r, &structs_text, &enums_text, &mut layouts, &resolve_name, &mut iface);
            }
            rs.push(Res { ty: g, text: String::new() });
        }
        let qname = if ns.is_empty() { leaf.clone() } else { format!("{ns}::{leaf}") };
        iface.exports.push(Export { name: leaf.clone(), qname, symbol: symbol.clone(), doc: String::new(), params: ps, results: rs });
    }
    // Nested types reached only through fields were added by size_of.
    Ok(iface)
}
