//! The type map: how each Rust parameter and result type crosses, decided
//! from its syntax. Which named types are structs and which are handles is
//! left to the traits (`goose::Arg`, `goose::Out`), which the derive and
//! the handle attribute implement.

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{GenericArgument, PathArguments, ReturnType, Type, TypeParamBound};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Prim {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Bool,
    Char,
    Usize,
    Isize,
}

impl Prim {
    pub fn from_ident(s: &str) -> Option<Prim> {
        Some(match s {
            "i8" => Prim::I8,
            "i16" => Prim::I16,
            "i32" => Prim::I32,
            "i64" => Prim::I64,
            "u8" => Prim::U8,
            "u16" => Prim::U16,
            "u32" => Prim::U32,
            "u64" => Prim::U64,
            "f32" => Prim::F32,
            "f64" => Prim::F64,
            "bool" => Prim::Bool,
            "char" => Prim::Char,
            "usize" => Prim::Usize,
            "isize" => Prim::Isize,
            _ => return None,
        })
    }

    /// The Goose type.
    pub fn goose(self) -> &'static str {
        match self {
            Prim::I8 => "i8",
            Prim::I16 => "i16",
            Prim::I32 => "i32",
            Prim::I64 | Prim::Isize => "i64",
            Prim::U8 => "u8",
            Prim::U16 => "u16",
            Prim::U32 | Prim::Char => "u32",
            Prim::U64 | Prim::Usize => "u64",
            Prim::F32 => "f32",
            Prim::F64 => "f64",
            Prim::Bool => "bool",
        }
    }

    /// The Rust type a shim takes or returns it as.
    pub fn raw(self) -> TokenStream {
        match self {
            Prim::Bool => quote!(u8),
            Prim::Char => quote!(u32),
            Prim::Usize => quote!(u64),
            Prim::Isize => quote!(i64),
            other => {
                let id = syn::Ident::new(other.goose(), proc_macro2::Span::call_site());
                quote!(#id)
            }
        }
    }

    /// The Rust type itself.
    pub fn rust(self) -> TokenStream {
        let s = match self {
            Prim::Usize => "usize",
            Prim::Isize => "isize",
            Prim::Char => "char",
            other => other.goose(),
        };
        let id = syn::Ident::new(s, proc_macro2::Span::call_site());
        quote!(#id)
    }

    /// Converts a value of the Rust type to the raw one.
    pub fn to_raw(self, v: TokenStream) -> TokenStream {
        match self {
            Prim::Bool => quote!((#v) as u8),
            Prim::Char => quote!((#v) as u32),
            Prim::Usize => quote!((#v) as u64),
            Prim::Isize => quote!((#v) as i64),
            _ => v,
        }
    }

    /// Converts a raw argument to the Rust type: statements that bind
    /// `out`, failing through `?` when the raw value is not a value.
    pub fn from_raw(self, raw: &syn::Ident, out: &syn::Ident, param: &str) -> TokenStream {
        match self {
            Prim::Bool => quote! {
                let #out: bool = match #raw {
                    0 => false,
                    1 => true,
                    _ => return ::core::result::Result::Err(::goose::BridgeError::Abort(
                        ::std::format!("argument {} is a bool other than 0 or 1", #param))),
                };
            },
            Prim::Char => quote! {
                let #out: char = ::core::char::from_u32(#raw).ok_or_else(|| ::goose::BridgeError::Fail(
                    ::std::format!("argument {} is not a Unicode scalar value", #param)))?;
            },
            Prim::Usize => quote! { let #out = #raw as usize; },
            Prim::Isize => quote! { let #out = #raw as isize; },
            _ => quote! { let #out = #raw; },
        }
    }
}

/// A callback argument: what the Rust closure is called with.
#[derive(Clone)]
pub enum CbArgKind {
    /// A scalar; the bool says it is given by reference.
    Scalar(Prim, bool),
    /// A value type; the bool says it is given by reference.
    Value(Type, bool),
    /// `&str` or `String`: copied into a fresh builder per call.
    Str,
}

#[derive(Clone)]
pub struct Callback {
    pub args: Vec<CbArgKind>,
    /// The closure's result: a scalar or a value type.
    pub ret: Option<CbRet>,
    /// `&mut dyn FnMut`/`&dyn Fn` rather than `impl Fn...`.
    pub by_ref: bool,
    /// The argument types as written, for the closure's signature.
    pub arg_types: Vec<Type>,
}

#[derive(Clone)]
pub enum CbRet {
    Scalar(Prim),
    Value(Type),
}

#[derive(Clone)]
pub enum PKind {
    Scalar(Prim),
    Str { owned: bool },
    Bytes { owned: bool },
    Slice { elem: Type, mutable: bool, owned: bool },
    Buf,
    TypedBuf(Type),
    Ref { ty: Type, mutable: bool },
    Val(Type),
    Optional(Box<PKind>),
    Callback(Callback),
    /// `&goose::Reentry`: not from Goose; the token for calling it back.
    Reentry,
}

impl PKind {
    /// Whether results may borrow from it as from a Goose slice.
    pub fn is_slice(&self) -> bool {
        matches!(self, PKind::Str { owned: false } | PKind::Bytes { owned: false } | PKind::Slice { owned: false, .. })
    }
}

fn last_segment(ty: &Type) -> Option<&syn::PathSegment> {
    match ty {
        Type::Path(p) if p.qself.is_none() => p.path.segments.last(),
        Type::Group(g) => last_segment(&g.elem),
        Type::Paren(p) => last_segment(&p.elem),
        _ => None,
    }
}

fn ident_is(ty: &Type, name: &str) -> bool {
    last_segment(ty).map(|s| s.ident == name && s.arguments.is_empty()).unwrap_or(false)
}

pub fn prim_of(ty: &Type) -> Option<Prim> {
    match ty {
        Type::Path(p) if p.qself.is_none() && p.path.segments.len() == 1 => {
            let s = &p.path.segments[0];
            if s.arguments.is_empty() {
                Prim::from_ident(&s.ident.to_string())
            } else {
                None
            }
        }
        Type::Group(g) => prim_of(&g.elem),
        Type::Paren(p) => prim_of(&p.elem),
        _ => None,
    }
}

/// The single type argument of `Name<T>`, where the last segment is `name`.
pub fn generic_arg<'a>(ty: &'a Type, name: &str) -> Option<&'a Type> {
    let seg = last_segment(ty)?;
    if seg.ident != name {
        return None;
    }
    if let PathArguments::AngleBracketed(a) = &seg.arguments {
        let tys: Vec<&Type> = a
            .args
            .iter()
            .filter_map(|g| if let GenericArgument::Type(t) = g { Some(t) } else { None })
            .collect();
        return tys.first().copied();
    }
    None
}

/// Both type arguments of `Result<T, E>`.
fn result_args(ty: &Type) -> Option<(&Type, &Type)> {
    let seg = last_segment(ty)?;
    if seg.ident != "Result" {
        return None;
    }
    if let PathArguments::AngleBracketed(a) = &seg.arguments {
        let tys: Vec<&Type> = a
            .args
            .iter()
            .filter_map(|g| if let GenericArgument::Type(t) = g { Some(t) } else { None })
            .collect();
        if tys.len() == 2 {
            return Some((tys[0], tys[1]));
        }
    }
    None
}

fn reject(span: &impl ToTokens, msg: &str) -> syn::Error {
    syn::Error::new_spanned(span, format!("goose::export: {msg}"))
}

pub fn classify_param(ty: &Type) -> syn::Result<PKind> {
    match ty {
        Type::Reference(r) => {
            if let Some(lt) = &r.lifetime {
                if lt.ident == "static" {
                    return Err(reject(
                        ty,
                        "a Goose argument is lent for the call only and cannot be 'static (spec §7.10)",
                    ));
                }
            }
            let mutable = r.mutability.is_some();
            let elem = &*r.elem;
            if let Type::Slice(s) = elem {
                if !mutable && ident_is(&s.elem, "u8") {
                    return Ok(PKind::Bytes { owned: false });
                }
                return Ok(PKind::Slice { elem: (*s.elem).clone(), mutable, owned: false });
            }
            if ident_is(elem, "str") {
                if mutable {
                    return Err(reject(ty, "&mut str cannot cross; take &mut [u8]"));
                }
                return Ok(PKind::Str { owned: false });
            }
            if let Some(seg) = last_segment(elem) {
                if seg.ident == "Reentry" {
                    if mutable {
                        return Err(reject(ty, "the call back token is taken as &goose::Reentry"));
                    }
                    return Ok(PKind::Reentry);
                }
                if seg.ident == "GooseBuf" {
                    if !mutable {
                        return Err(reject(ty, "a builder is taken as &mut GooseBuf"));
                    }
                    return Ok(PKind::Buf);
                }
                if seg.ident == "GooseVec" {
                    let t = generic_arg(elem, "GooseVec")
                        .ok_or_else(|| reject(ty, "GooseVec<T> needs its element type"))?;
                    if !mutable {
                        return Err(reject(ty, "a typed builder is taken as &mut GooseVec<T>"));
                    }
                    return Ok(PKind::TypedBuf(t.clone()));
                }
                if seg.ident == "String" || seg.ident == "Vec" {
                    return Err(reject(ty, "take &str or &[T] rather than a reference to an owned String or Vec"));
                }
            }
            if let Type::TraitObject(t) = elem {
                if let Some(cb) = callback_of(t.bounds.iter(), true)? {
                    return Ok(PKind::Callback(cb));
                }
                return Err(reject(ty, "trait objects cannot cross, except &mut dyn FnMut(..) callbacks"));
            }
            Ok(PKind::Ref { ty: elem.clone(), mutable })
        }
        Type::ImplTrait(t) => {
            if let Some(cb) = callback_of(t.bounds.iter(), false)? {
                return Ok(PKind::Callback(cb));
            }
            Err(reject(ty, "only impl Fn/FnMut/FnOnce(..) parameters (callbacks) can cross"))
        }
        Type::Path(_) | Type::Group(_) | Type::Paren(_) => {
            if let Some(p) = prim_of(ty) {
                return Ok(PKind::Scalar(p));
            }
            if ident_is(ty, "String") {
                return Ok(PKind::Str { owned: true });
            }
            if let Some(t) = generic_arg(ty, "Vec") {
                if ident_is(t, "u8") {
                    return Ok(PKind::Bytes { owned: true });
                }
                return Ok(PKind::Slice { elem: t.clone(), mutable: false, owned: true });
            }
            if let Some(t) = generic_arg(ty, "Option") {
                let inner = classify_param(t)?;
                return match inner {
                    PKind::Optional(_) => Err(reject(ty, "Option<Option<..>> cannot cross")),
                    PKind::Buf | PKind::TypedBuf(_) | PKind::Callback(_) => {
                        Err(reject(ty, "an optional builder or callback cannot cross"))
                    }
                    PKind::Slice { mutable: true, .. } => Err(reject(ty, "an optional &mut slice cannot cross")),
                    other => Ok(PKind::Optional(Box::new(other))),
                };
            }
            if let Some(seg) = last_segment(ty) {
                if seg.ident == "GooseBuf" || seg.ident == "GooseVec" {
                    return Err(reject(ty, "a builder is taken as &mut GooseBuf / &mut GooseVec<T>"));
                }
                if !seg.arguments.is_empty() {
                    if let PathArguments::AngleBracketed(a) = &seg.arguments {
                        if a.args.iter().any(|g| matches!(g, GenericArgument::Lifetime(_))) {
                            return Err(reject(ty, "a type borrowing from elsewhere cannot cross by value"));
                        }
                    }
                }
            }
            Ok(PKind::Val(ty.clone()))
        }
        Type::Array(_) => Ok(PKind::Val(ty.clone())),
        Type::Tuple(t) if t.elems.is_empty() => Err(reject(ty, "a () parameter carries nothing")),
        _ => Err(reject(ty, "this type cannot cross to Goose (see the type map)")),
    }
}

fn callback_of<'a>(
    bounds: impl Iterator<Item = &'a TypeParamBound>,
    by_ref: bool,
) -> syn::Result<Option<Callback>> {
    for b in bounds {
        let TypeParamBound::Trait(t) = b else { continue };
        let Some(seg) = t.path.segments.last() else { continue };
        if seg.ident != "Fn" && seg.ident != "FnMut" && seg.ident != "FnOnce" {
            continue;
        }
        let PathArguments::Parenthesized(p) = &seg.arguments else { continue };
        let mut args = Vec::new();
        let mut arg_types = Vec::new();
        for a in &p.inputs {
            arg_types.push(a.clone());
            args.push(cb_arg(a)?);
        }
        let ret = match &p.output {
            ReturnType::Default => None,
            ReturnType::Type(_, t) => {
                if let Type::Tuple(tt) = &**t {
                    if tt.elems.is_empty() {
                        None
                    } else {
                        return Err(reject(t, "a callback returns a scalar or a goose::Flat value"));
                    }
                } else if let Some(p) = prim_of(t) {
                    Some(CbRet::Scalar(p))
                } else {
                    Some(CbRet::Value((**t).clone()))
                }
            }
        };
        return Ok(Some(Callback { args, ret, by_ref, arg_types }));
    }
    Ok(None)
}

fn cb_arg(t: &Type) -> syn::Result<CbArgKind> {
    if let Some(p) = prim_of(t) {
        return Ok(CbArgKind::Scalar(p, false));
    }
    if ident_is(t, "String") {
        return Ok(CbArgKind::Str);
    }
    if let Type::Reference(r) = t {
        if r.mutability.is_some() {
            return Err(reject(t, "a callback argument is given to Goose by value; &mut cannot cross"));
        }
        if ident_is(&r.elem, "str") {
            return Ok(CbArgKind::Str);
        }
        if let Some(p) = prim_of(&r.elem) {
            return Ok(CbArgKind::Scalar(p, true));
        }
        return Ok(CbArgKind::Value((*r.elem).clone(), true));
    }
    Ok(CbArgKind::Value(t.clone(), false))
}

/// What a borrowed result is a slice of.
#[derive(Clone)]
pub enum BElem {
    Str,
    Bytes,
    Slice(Type),
}

impl BElem {
    /// The part as a Rust slice of the element type.
    pub fn part(&self, v: TokenStream) -> TokenStream {
        match self {
            BElem::Str => quote!(::core::primitive::str::as_bytes(#v)),
            _ => quote!(#v),
        }
    }
}

#[derive(Clone)]
pub enum RKind {
    Unit,
    Scalar(Prim),
    Out(Type),
    Bytes,
    Vec(Type),
    Borrowed { param: usize, elem: BElem },
    /// Borrowed from something that is not a Goose slice (a handle's
    /// object): copied out. Text and bytes become a builder, a slice of
    /// values a Vec.
    Copied(BElem),
    Option(Box<RKind>),
    Result(Box<RKind>),
    Spans { param: usize, elem: BElem },
    Iter { item: Box<RKind>, item_ty: Type },
}

impl RKind {
    /// Whether it (or what it wraps) needs a typed builder, which a compiler
    /// without them replaces with a count and a copy.
    pub fn needs_typed(&self) -> bool {
        match self {
            RKind::Vec(_) | RKind::Spans { .. } | RKind::Copied(BElem::Slice(_)) => true,
            RKind::Option(i) | RKind::Result(i) => i.needs_typed(),
            _ => false,
        }
    }

    pub fn fallible(&self) -> bool {
        matches!(self, RKind::Option(_) | RKind::Result(_))
    }
}

/// The parameter a borrowed result's lifetime names: by name, or by Rust's
/// elision rule (the only reference parameter).
fn borrowed_from(lt: Option<&syn::Lifetime>, params: &[(String, Type, PKind)], span: &Type) -> syn::Result<usize> {
    let has_lt = |t: &Type, name: &syn::Lifetime| -> bool {
        let mut found = false;
        fn walk(t: &Type, name: &syn::Lifetime, found: &mut bool) {
            match t {
                Type::Reference(r) => {
                    if r.lifetime.as_ref() == Some(name) {
                        *found = true;
                    }
                    walk(&r.elem, name, found);
                }
                Type::Path(p) => {
                    for s in &p.path.segments {
                        if let PathArguments::AngleBracketed(a) = &s.arguments {
                            for g in &a.args {
                                match g {
                                    GenericArgument::Lifetime(l) if l == name => *found = true,
                                    GenericArgument::Type(t) => walk(t, name, found),
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                Type::Slice(s) => walk(&s.elem, name, found),
                _ => {}
            }
        }
        walk(t, name, &mut found);
        found
    };
    if let Some(lt) = lt {
        if lt.ident != "_" {
            let hits: Vec<usize> =
                params.iter().enumerate().filter(|(_, p)| has_lt(&p.1, lt)).map(|(i, _)| i).collect();
            return match hits.as_slice() {
                [i] => Ok(*i),
                [] => Err(reject(span, &format!("no parameter carries the lifetime {lt}"))),
                _ => Err(reject(span, &format!("the result borrows from several parameters ({lt}); Goose roots a result at one"))),
            };
        }
    }
    let refs: Vec<usize> = params
        .iter()
        .enumerate()
        .filter(|(_, p)| matches!(p.1, Type::Reference(_)) && !matches!(p.2, PKind::Callback(_)))
        .map(|(i, _)| i)
        .collect();
    match refs.as_slice() {
        [i] => Ok(*i),
        _ => Err(reject(span, "name the lifetime of the parameter the result borrows from")),
    }
}

fn belem_of(t: &Type) -> Option<BElem> {
    match t {
        Type::Path(_) if ident_is(t, "str") => Some(BElem::Str),
        Type::Slice(s) if ident_is(&s.elem, "u8") => Some(BElem::Bytes),
        Type::Slice(s) => Some(BElem::Slice((*s.elem).clone())),
        _ => None,
    }
}

pub fn classify_ret(ret: &ReturnType, params: &[(String, Type, PKind)]) -> syn::Result<RKind> {
    match ret {
        ReturnType::Default => Ok(RKind::Unit),
        ReturnType::Type(_, t) => classify_ret_ty(t, params, true),
    }
}

fn classify_ret_ty(t: &Type, params: &[(String, Type, PKind)], top: bool) -> syn::Result<RKind> {
    if let Type::Tuple(tt) = t {
        if tt.elems.is_empty() {
            return Ok(RKind::Unit);
        }
        return Err(reject(t, "tuples cannot cross yet; return a goose::Flat struct"));
    }
    if let Some(p) = prim_of(t) {
        return Ok(RKind::Scalar(p));
    }
    if let Type::Reference(r) = t {
        if r.mutability.is_some() {
            return Err(reject(t, "a &mut result cannot cross"));
        }
        let Some(elem) = belem_of(&r.elem) else {
            return Err(reject(t, "a borrowed result is a &str, &[u8] or &[T] of a parameter"));
        };
        let from = borrowed_from(r.lifetime.as_ref(), params, t)?;
        if params[from].2.is_slice() || matches!(params[from].2, PKind::Slice { mutable: true, owned: false, .. }) {
            return Ok(RKind::Borrowed { param: from, elem });
        }
        return Ok(RKind::Copied(elem));
    }
    if let Type::ImplTrait(it) = t {
        return classify_iter(t, it, params, top);
    }
    if ident_is(t, "String") {
        return Ok(RKind::Bytes);
    }
    if let Some(inner) = generic_arg(t, "Box") {
        if ident_is(inner, "str") || matches!(inner, Type::Slice(s) if ident_is(&s.elem, "u8")) {
            return Ok(RKind::Bytes);
        }
    }
    if let Some(seg) = last_segment(t) {
        if seg.ident == "Cow" {
            return Ok(RKind::Bytes);
        }
    }
    if let Some(inner) = generic_arg(t, "Vec") {
        if ident_is(inner, "u8") {
            return Ok(RKind::Bytes);
        }
        return Ok(RKind::Vec(inner.clone()));
    }
    if let Some(inner) = generic_arg(t, "Option") {
        if !top {
            return Err(reject(t, "an Option inside an Option or Result cannot cross"));
        }
        let i = classify_ret_ty(inner, params, false)?;
        if matches!(i, RKind::Unit) {
            return Err(reject(t, "Option<()> carries only a bool: return bool"));
        }
        return Ok(RKind::Option(Box::new(i)));
    }
    if let Some((ok, _err)) = result_args(t) {
        if !top {
            return Err(reject(t, "a Result inside an Option or Result cannot cross"));
        }
        let i = classify_ret_ty(ok, params, false)?;
        return Ok(RKind::Result(Box::new(i)));
    }
    if matches!(t, Type::Path(_) | Type::Array(_)) {
        return Ok(RKind::Out(t.clone()));
    }
    Err(reject(t, "this result cannot cross to Goose (see the type map)"))
}

fn classify_iter(
    t: &Type,
    it: &syn::TypeImplTrait,
    params: &[(String, Type, PKind)],
    top: bool,
) -> syn::Result<RKind> {
    if !top {
        return Err(reject(t, "an iterator inside an Option or Result cannot cross"));
    }
    let mut item: Option<Type> = None;
    let mut bounded = false;
    for b in &it.bounds {
        match b {
            TypeParamBound::Lifetime(_) => bounded = true,
            TypeParamBound::Trait(tb) => {
                let Some(seg) = tb.path.segments.last() else { continue };
                if seg.ident != "Iterator" && seg.ident != "ExactSizeIterator" && seg.ident != "DoubleEndedIterator" {
                    continue;
                }
                if let PathArguments::AngleBracketed(a) = &seg.arguments {
                    for g in &a.args {
                        if let GenericArgument::AssocType(at) = g {
                            if at.ident == "Item" {
                                item = Some(at.ty.clone());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let Some(item) = item else {
        return Err(reject(t, "only impl Iterator<Item = ..> results can cross"));
    };
    if let Type::Reference(r) = &item {
        let Some(elem) = belem_of(&r.elem) else {
            return Err(reject(&item, "an iterator of references yields &str, &[u8] or &[T] of a parameter"));
        };
        let from = borrowed_from(r.lifetime.as_ref(), params, &item)?;
        if !params[from].2.is_slice() {
            return Err(reject(&item, "the iterator's items must borrow from a slice or &str parameter"));
        }
        return Ok(RKind::Spans { param: from, elem });
    }
    let it_kind = classify_ret_ty(&item, params, false)?;
    match &it_kind {
        RKind::Scalar(_) | RKind::Out(_) | RKind::Bytes => {}
        _ => return Err(reject(&item, "an iterator yields scalars, goose::Flat values, handles or String")),
    }
    if bounded {
        // The iterator borrows the call's arguments, so it cannot outlive
        // the call: collect it then.
        return match it_kind {
            RKind::Scalar(_) | RKind::Out(_) => Ok(RKind::Vec(item)),
            _ => Err(reject(&item, "an iterator of String that borrows its arguments cannot cross; collect a String")),
        };
    }
    Ok(RKind::Iter { item: Box::new(it_kind), item_ty: item })
}

