//! `#[goose::export]`: an `extern "C"` shim in the shapes of spec §7.10 for
//! an ordinary Rust function, and the record `cargo goose` builds the Goose
//! wrapper from.
//!
//! A shim holds each argument (validated, borrowed in place or copied),
//! checks that no `&mut` argument aliases another, calls the function under
//! the panic catcher and sends the result out as a value, an out-pointer,
//! an offset into an argument or bytes appended to a builder.
//!
//! Symbols, for a function whose base symbol is `S` (`ns_name`):
//!
//! * `S`: the call. With optional parameters, one entry point per
//!   combination instead: `S_some`/`S_none` for one, `S_sn`... for several.
//! * `S__len`, `S__take`: a result needing a typed builder, counted and
//!   then copied, for a compiler without typed builders.
//! * `S__next`, `S__release`: a lazy iterator's handle.
//! * `S__start`, `S__next`, `S__reply`, `S__finish`: a function taking a
//!   callback, run as a coroutine the Goose wrapper drives.

use crate::doc_of;
use crate::info::{crate_info, library_info};
use crate::types::*;
use crate::camel;
use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote};
use std::collections::HashMap;
use syn::{parse::Parser, punctuated::Punctuated, spanned::Spanned, FnArg, ItemFn, Meta, Pat, Token, Type};

struct Options {
    instantiate: Option<(Ident, Vec<Type>)>,
    throws: bool,
    main_thread: bool,
    name: Option<String>,
    symbol: Option<String>,
}

fn parse_options(attr: TokenStream) -> syn::Result<Options> {
    let mut o = Options { instantiate: None, throws: false, main_thread: false, name: None, symbol: None };
    let metas = Punctuated::<Meta, Token![,]>::parse_terminated.parse2(attr)?;
    for m in metas {
        match &m {
            Meta::Path(p) if p.is_ident("throws") => o.throws = true,
            Meta::Path(p) if p.is_ident("main_thread") => o.main_thread = true,
            Meta::NameValue(nv) if nv.path.is_ident("name") || nv.path.is_ident("symbol") => {
                let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) = &nv.value else {
                    return Err(syn::Error::new(nv.value.span(), "expected a string"));
                };
                if nv.path.is_ident("name") {
                    o.name = Some(s.value());
                } else {
                    o.symbol = Some(s.value());
                }
            }
            Meta::List(l) if l.path.is_ident("instantiate") => {
                // instantiate(T = f32, f64, ...)
                let parser = |input: syn::parse::ParseStream| -> syn::Result<(Ident, Vec<Type>)> {
                    let id: Ident = input.parse()?;
                    input.parse::<Token![=]>()?;
                    let tys = Punctuated::<Type, Token![,]>::parse_terminated(input)?;
                    Ok((id, tys.into_iter().collect()))
                };
                o.instantiate = Some(parser.parse2(l.tokens.clone())?);
            }
            other => {
                return Err(syn::Error::new(
                    other.span(),
                    "goose::export takes instantiate(T = ..), throws, main_thread, name = \"..\", symbol = \"..\"",
                ))
            }
        }
    }
    Ok(o)
}

/// Replaces the type parameter `from` with `to` throughout a type.
struct Subst<'a> {
    from: &'a Ident,
    to: &'a Type,
}

impl syn::visit_mut::VisitMut for Subst<'_> {
    fn visit_type_mut(&mut self, t: &mut Type) {
        if let Type::Path(p) = t {
            if p.qself.is_none() && p.path.is_ident(self.from) {
                *t = self.to.clone();
                return;
            }
        }
        syn::visit_mut::visit_type_mut(self, t);
    }
}

fn type_suffix(t: &Type) -> String {
    let s = quote!(#t).to_string();
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if (c == ';' || c == ',' || c == ' ') && !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_string()
}

pub fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let opts = parse_options(attr)?;
    let f: ItemFn = syn::parse2(item)?;
    if let Some(a) = &f.sig.asyncness {
        return Err(syn::Error::new(a.span(), "goose::export: async functions cannot cross; Goose has no futures"));
    }
    if let Some(r) = f.sig.receiver() {
        return Err(syn::Error::new(r.span(), "goose::export goes on a free function; wrap the method in one"));
    }
    if f.sig.variadic.is_some() {
        return Err(syn::Error::new(f.sig.span(), "goose::export: variadic functions cannot cross"));
    }
    let type_params: Vec<&syn::TypeParam> = f.sig.generics.type_params().collect();
    let const_params = f.sig.generics.const_params().count();
    if const_params > 0 {
        return Err(syn::Error::new(f.sig.generics.span(), "goose::export: const generics cannot be instantiated from Goose"));
    }
    let insts: Vec<Option<(Ident, Type)>> = match (&opts.instantiate, type_params.as_slice()) {
        (None, []) => vec![None],
        (None, _) => {
            return Err(syn::Error::new(
                f.sig.generics.span(),
                "goose::export: a generic function needs instantiate(T = ...) to name the versions Goose may call",
            ))
        }
        (Some((id, tys)), [tp]) => {
            if &tp.ident != id {
                return Err(syn::Error::new(id.span(), format!("the function's type parameter is {}", tp.ident)));
            }
            tys.iter().map(|t| Some((id.clone(), t.clone()))).collect()
        }
        (Some((id, _)), []) => return Err(syn::Error::new(id.span(), "instantiate(..) on a function without type parameters")),
        (Some(_), _) => {
            return Err(syn::Error::new(f.sig.generics.span(), "goose::export: instantiate supports one type parameter"))
        }
    };
    if opts.throws {
        let ok = matches!(&f.sig.output, syn::ReturnType::Type(_, t) if matches!(&**t, Type::Path(p) if p.path.segments.last().map(|s| s.ident == "Result").unwrap_or(false)));
        if !ok {
            return Err(syn::Error::new(f.sig.span(), "goose::export(throws) goes on a function returning Result"));
        }
    }
    let mut out = quote!(#f);
    for inst in &insts {
        out.extend(function(&f, inst.as_ref(), &opts)?);
    }
    out.extend(library_info());
    Ok(out)
}

struct Param {
    name: String,
    ty: Type,
    kind: PKind,
}

/// One shim's inputs, as lowered.
#[derive(Default)]
struct Lowered {
    c_params: Vec<TokenStream>,
    shapes: Vec<TokenStream>,
    spans: Vec<TokenStream>,
    may_alias: bool,
    holds: Vec<TokenStream>,
    takes: Vec<TokenStream>,
    args: Vec<TokenStream>,
    /// Parameter index -> (base pointer, length) of a held slice, for results
    /// borrowed from it.
    bases: HashMap<usize, (Ident, Ident)>,
}

fn id(s: &str) -> Ident {
    Ident::new(s, Span::call_site())
}

/// Lowers parameter `i` (not optional, not a callback). `present` wraps the
/// argument in `Some` for an optional parameter's entry point.
fn lower_param(l: &mut Lowered, i: usize, p: &Param, kind: &PKind, some: bool) {
    let raw = format_ident!("p{}", i);
    let h = format_ident!("__h{}", i);
    let name = &p.name;
    let arg: TokenStream = match kind {
        PKind::Scalar(pr) => {
            let rt = pr.raw();
            let shape = pr.goose();
            l.c_params.push(quote!(#raw: #rt));
            l.shapes.push(quote!(#shape));
            let a = format_ident!("__a{}", i);
            l.holds.push(pr.from_raw(&raw, &a, name));
            quote!(#a)
        }
        PKind::Str { owned } => {
            l.c_params.push(quote!(#raw: ::goose::GooseSlice<u8>));
            l.shapes.push(quote!("slice"));
            if !owned {
                l.spans.push(quote!(::goose::slice_span::<u8>(&#raw, false, #name)));
            }
            l.holds.push(quote! { let #h = unsafe { ::goose::StrRef::hold(#raw, #name) }?; });
            if *owned {
                quote!(::std::string::String::from(#h.get()))
            } else {
                record_base(l, i, quote!(#h.bytes()));
                quote!(#h.get())
            }
        }
        PKind::Bytes { owned } => {
            l.c_params.push(quote!(#raw: ::goose::GooseSlice<u8>));
            l.shapes.push(quote!("slice"));
            if !owned {
                l.spans.push(quote!(::goose::slice_span::<u8>(&#raw, false, #name)));
            }
            l.holds.push(quote! { let #h = unsafe { ::goose::SliceRef::<u8>::hold(#raw, #name) }?; });
            if *owned {
                quote!(#h.into_vec())
            } else {
                record_base(l, i, quote!(#h.get()));
                quote!(#h.get())
            }
        }
        PKind::Slice { elem, mutable, owned } => {
            l.c_params.push(quote!(#raw: ::goose::GooseSlice<#elem>));
            l.shapes.push(quote!("slice"));
            if !owned {
                l.spans.push(quote!(::goose::slice_span::<#elem>(&#raw, #mutable, #name)));
                if *mutable {
                    l.may_alias = true;
                }
            }
            if *mutable {
                l.holds.push(quote! { let mut #h = unsafe { ::goose::SliceMut::<#elem>::hold(#raw, #name) }?; });
                record_base(l, i, quote!(#h.get()));
                quote!(#h.get_mut())
            } else {
                l.holds.push(quote! { let #h = unsafe { ::goose::SliceRef::<#elem>::hold(#raw, #name) }?; });
                if *owned {
                    quote!(#h.into_vec())
                } else {
                    record_base(l, i, quote!(#h.get()));
                    quote!(#h.get())
                }
            }
        }
        PKind::Buf => {
            l.c_params.push(quote!(#raw: ::goose::GsRref));
            l.shapes.push(quote!("builder"));
            l.holds.push(quote! { let mut #h = unsafe { ::goose::GooseBuf::from_raw(#raw) }; });
            quote!(&mut #h)
        }
        PKind::TypedBuf(t) => {
            l.c_params.push(quote!(#raw: ::goose::GsRref));
            l.shapes.push(quote!("builder"));
            l.holds.push(quote! { let mut #h = unsafe { ::goose::GooseVec::<#t>::from_raw(#raw) }; });
            quote!(&mut #h)
        }
        PKind::Ref { ty, mutable } => {
            l.c_params.push(quote!(#raw: <#ty as ::goose::Arg>::Raw));
            l.shapes.push(quote!(<#ty as ::goose::Arg>::C_REF));
            l.spans.push(quote!(<#ty as ::goose::Arg>::span(&#raw, #mutable, #name)));
            if *mutable {
                l.may_alias = true;
                l.holds.push(quote! { let mut #h = unsafe { <#ty as ::goose::Arg>::exclusive(#raw, #name) }?; });
                quote!(&mut *#h)
            } else {
                l.holds.push(quote! { let #h = unsafe { <#ty as ::goose::Arg>::shared(#raw, #name) }?; });
                quote!(&*#h)
            }
        }
        PKind::Val(ty) => {
            l.c_params.push(quote!(#raw: <#ty as ::goose::Arg>::ValRaw));
            l.shapes.push(quote!(<#ty as ::goose::Arg>::C_VAL));
            l.spans.push(quote!(<#ty as ::goose::Arg>::val_span(&#raw, #name)));
            l.may_alias = true;
            l.takes.push(quote! { let #h = unsafe { <#ty as ::goose::Arg>::take(#raw, #name) }?; });
            quote!(#h)
        }
        PKind::Reentry => {
            l.holds.push(quote! { let __reentry = ::goose::Reentry::enter()?; });
            quote!(&__reentry)
        }
        PKind::Optional(_) | PKind::Callback(_) => unreachable!("lowered elsewhere"),
    };
    l.args.push(if some { quote!(::core::option::Option::Some(#arg)) } else { arg });
}

fn record_base(l: &mut Lowered, i: usize, slice: TokenStream) {
    let b = format_ident!("__base{}", i);
    let n = format_ident!("__blen{}", i);
    l.holds.push(quote! {
        let #b = { let __s = #slice; __s.as_ptr() };
        let #n = { let __s = #slice; __s.len() };
    });
    l.bases.insert(i, (b, n));
}

/// The out-parameters a result adds, and its C return type.
struct RetLower {
    c_params: Vec<TokenStream>,
    shapes: Vec<TokenStream>,
    c_ret: TokenStream,
    c_ret_shape: String,
}

fn ret_out_params(r: &RKind, wrapped: bool, stash: bool, c: &mut Vec<TokenStream>, s: &mut Vec<TokenStream>) {
    match r {
        RKind::Unit => {}
        RKind::Scalar(p) => {
            if wrapped {
                let rt = p.raw();
                let shape = format!("out:{}", p.goose());
                c.push(quote!(__out: *mut #rt));
                s.push(quote!(#shape));
            }
        }
        RKind::Out(t) => {
            c.push(quote!(__out: <#t as ::goose::Out>::OutRaw));
            s.push(quote!(<#t as ::goose::Out>::C_OUT));
        }
        RKind::Bytes | RKind::Copied(BElem::Str) | RKind::Copied(BElem::Bytes) => {
            c.push(quote!(__out: ::goose::GsRref));
            s.push(quote!("builder"));
        }
        RKind::Vec(_) | RKind::Spans { .. } | RKind::Copied(BElem::Slice(_)) => {
            if !stash {
                c.push(quote!(__out: ::goose::GsRref));
                s.push(quote!("builder"));
            }
        }
        RKind::Borrowed { .. } => {
            c.push(quote!(__at: *mut i64));
            c.push(quote!(__len: *mut i64));
            s.push(quote!("out:i64"));
            s.push(quote!("out:i64"));
        }
        RKind::Iter { .. } => {
            c.push(quote!(__out: *mut ::goose::Handle));
            s.push(quote!("out:handle"));
        }
        RKind::Option(i) => ret_out_params(i, true, stash, c, s),
        RKind::Result(i) => {
            ret_out_params(i, true, stash, c, s);
            c.push(quote!(__err: ::goose::GsRref));
            s.push(quote!("builder"));
        }
    }
}

fn ret_lower(r: &RKind, stash: bool) -> RetLower {
    let mut c = Vec::new();
    let mut s = Vec::new();
    ret_out_params(r, false, stash, &mut c, &mut s);
    let (c_ret, c_ret_shape) = if stash {
        (quote!(i64), "i64".to_string())
    } else {
        match r {
            RKind::Scalar(Prim::Bool) => (quote!(bool), "bool".to_string()),
            RKind::Scalar(p) => (p.raw(), p.goose().to_string()),
            RKind::Option(_) | RKind::Result(_) => (quote!(bool), "bool".to_string()),
            _ => (quote!(()), "void".to_string()),
        }
    };
    RetLower { c_params: c, shapes: s, c_ret, c_ret_shape }
}

/// The element a typed-builder result holds, as a Rust type.
fn typed_elem(r: &RKind) -> Option<TokenStream> {
    match r {
        RKind::Vec(t) => Some(quote!(#t)),
        RKind::Copied(BElem::Slice(t)) => Some(quote!(#t)),
        RKind::Spans { .. } => Some(quote!(::goose::GooseSpan)),
        RKind::Option(i) | RKind::Result(i) => typed_elem(i),
        _ => None,
    }
}

struct RetCtx<'a> {
    bases: &'a HashMap<usize, (Ident, Ident)>,
    params: &'a [Param],
    /// The stash key, when the result is counted and copied.
    stash: Option<String>,
    /// The iterator handle type of a lazy iterator result.
    iter_ty: Option<Ident>,
}

/// Statements that send `v` (wrapped: inside an Option or Result, where a
/// scalar goes through an out-pointer).
fn write_inner(r: &RKind, v: TokenStream, cx: &RetCtx) -> TokenStream {
    match r {
        RKind::Unit => quote! { let _ = #v; },
        RKind::Scalar(p) => {
            let rv = p.to_raw(quote!(#v));
            quote! { unsafe { ::core::ptr::write_unaligned(__out, #rv) }; }
        }
        RKind::Out(t) => quote! { unsafe { ::goose::put::<#t>(#v, __out) }?; },
        RKind::Bytes | RKind::Copied(BElem::Str) | RKind::Copied(BElem::Bytes) => quote! {
            {
                let mut __b = unsafe { ::goose::GooseBuf::from_raw(__out) };
                __b.extend_from_slice(::goose::AsBytes::goose_bytes(&#v));
            }
        },
        RKind::Vec(_) | RKind::Copied(BElem::Slice(_)) | RKind::Spans { .. } => {
            let elem = typed_elem(r).unwrap();
            let collect = match r {
                RKind::Spans { param, elem: be } => {
                    let (b, n) = &cx.bases[param];
                    let pname = &cx.params[*param].name;
                    let part = be.part(quote!(__p));
                    quote! {
                        let mut __v: ::std::vec::Vec<::goose::GooseSpan> = ::std::vec::Vec::new();
                        for __p in #v {
                            let (__o, __n) = ::goose::offset_in_raw(#b, #n, #part, #pname)?;
                            __v.push(::goose::GooseSpan { at: __o, len: __n });
                        }
                    }
                }
                RKind::Copied(_) => quote! { let __v: ::std::vec::Vec<#elem> = (#v).to_vec(); },
                _ => quote! { let __v: ::std::vec::Vec<#elem> = ::core::iter::IntoIterator::into_iter(#v).collect(); },
            };
            match &cx.stash {
                Some(key) => quote! {
                    #collect
                    __count = ::goose::stash_put::<#elem>(#key, __v);
                },
                None => quote! {
                    #collect
                    let mut __b = unsafe { ::goose::GooseVec::<#elem>::from_raw(__out) };
                    __b.extend_from_slice(&__v);
                },
            }
        }
        RKind::Borrowed { param, elem } => {
            let (b, n) = &cx.bases[param];
            let pname = &cx.params[*param].name;
            let part = elem.part(quote!(__p));
            quote! {
                {
                    let __p = #v;
                    let (__o, __n) = ::goose::offset_in_raw(#b, #n, #part, #pname)?;
                    unsafe {
                        ::goose::put_i64(__at, __o);
                        ::goose::put_i64(__len, __n);
                    }
                }
            }
        }
        RKind::Iter { .. } => {
            let it = cx.iter_ty.as_ref().unwrap();
            quote! {
                {
                    let __h = <#it as ::goose::HandleType>::table().insert(#it(::std::boxed::Box::new(#v)));
                    unsafe { ::core::ptr::write_unaligned(__out, __h) };
                }
            }
        }
        RKind::Option(_) | RKind::Result(_) => unreachable!("nested"),
    }
}

/// An expression of type `Result<c_ret, BridgeError>` sending the result `v`.
fn write_top(r: &RKind, v: TokenStream, cx: &RetCtx) -> TokenStream {
    let stash = cx.stash.is_some();
    match r {
        RKind::Scalar(Prim::Bool) if !stash => quote! { ::core::result::Result::Ok(#v) },
        RKind::Scalar(p) if !stash => {
            let rv = p.to_raw(v);
            quote! { ::core::result::Result::Ok(#rv) }
        }
        RKind::Option(i) => {
            let w = write_inner(i, quote!(__x), cx);
            if stash {
                quote! {{
                    let mut __count: i64 = -1;
                    match #v {
                        ::core::option::Option::Some(__x) => { #w }
                        ::core::option::Option::None => {}
                    }
                    ::core::result::Result::Ok(__count)
                }}
            } else {
                quote! {
                    match #v {
                        ::core::option::Option::Some(__x) => { #w ::core::result::Result::Ok(true) }
                        ::core::option::Option::None => ::core::result::Result::Ok(false),
                    }
                }
            }
        }
        RKind::Result(i) => {
            let w = write_inner(i, quote!(__x), cx);
            let err = quote! {
                {
                    use ::core::fmt::Write as _;
                    let mut __b = unsafe { ::goose::GooseBuf::from_raw(__err) };
                    let _ = ::core::write!(__b, "{}", __e);
                }
            };
            if stash {
                quote! {{
                    let mut __count: i64 = -1;
                    match #v {
                        ::core::result::Result::Ok(__x) => { #w }
                        ::core::result::Result::Err(__e) => #err,
                    }
                    ::core::result::Result::Ok(__count)
                }}
            } else {
                quote! {
                    match #v {
                        ::core::result::Result::Ok(__x) => { #w ::core::result::Result::Ok(true) }
                        ::core::result::Result::Err(__e) => { #err ::core::result::Result::Ok(false) }
                    }
                }
            }
        }
        _ => {
            let w = write_inner(r, v, cx);
            if stash {
                quote! {{ let mut __count: i64 = 0; #w ::core::result::Result::Ok(__count) }}
            } else {
                quote! {{ #w ::core::result::Result::Ok(()) }}
            }
        }
    }
}

/// What a fallible shim gives back when its function failed: the message
/// into the error builder of a Result function.
fn fail_closure(r: &RKind, stash: bool) -> TokenStream {
    let fail_val = if stash { quote!(-1i64) } else { quote!(false) };
    match r {
        RKind::Result(_) => quote! {
            |__m: &str| {
                let mut __b = unsafe { ::goose::GooseBuf::from_raw(__err) };
                __b.push_str(__m);
                #fail_val
            }
        },
        RKind::Option(_) => quote! { |_m: &str| #fail_val },
        _ => quote! { |_m: &str| ::core::default::Default::default() },
    }
}

fn overlap_check(l: &Lowered) -> TokenStream {
    if !l.may_alias || l.spans.len() < 2 {
        return quote!();
    }
    let spans = &l.spans;
    quote! { ::goose::check_overlap(&[#(#spans),*])?; }
}

#[allow(clippy::too_many_arguments)]
fn shim(
    sym: &str,
    goose_qualified: &str,
    main_thread: bool,
    l: &Lowered,
    call: &TokenStream,
    r: &RKind,
    cx: &RetCtx,
) -> TokenStream {
    let stash = cx.stash.is_some();
    let rl = ret_lower(r, stash);
    let sym_id = id(sym);
    let c_params = &l.c_params;
    let rp = &rl.c_params;
    let c_ret = &rl.c_ret;
    let overlap = overlap_check(l);
    let holds = &l.holds;
    let takes = &l.takes;
    let fallible = r.fallible();
    let write = write_top(r, quote!(__r), cx);
    let fail = fail_closure(r, stash);
    quote! {
        #[unsafe(no_mangle)]
        #[doc(hidden)]
        #[allow(improper_ctypes_definitions, unused_mut, unused_variables, unused_unsafe, non_snake_case, clippy::all)]
        pub unsafe extern "C" fn #sym_id(#(#c_params,)* #(#rp),*) -> #c_ret {
            static SITE: ::goose::Site = ::goose::Site { name: #goose_qualified, fallible: #fallible, main_thread: #main_thread };
            ::goose::call(&SITE, || {
                #overlap
                #(#holds)*
                #(#takes)*
                let __r = #call;
                #write
            }, #fail)
        }
    }
}

fn param_meta(kind: &PKind) -> TokenStream {
    match kind {
        PKind::Scalar(p) => {
            let g = p.goose();
            quote!(::goose::meta::ParamKind::Scalar(::goose::meta::Ty::Prim(#g)))
        }
        PKind::Str { .. } => quote!(::goose::meta::ParamKind::Str),
        PKind::Bytes { .. } => quote!(::goose::meta::ParamKind::Bytes),
        PKind::Slice { elem, mutable, owned } => {
            if *mutable && !owned {
                quote!(::goose::meta::ParamKind::SliceMut(<#elem as ::goose::Value>::goose_ty()))
            } else {
                quote!(::goose::meta::ParamKind::Slice(<#elem as ::goose::Value>::goose_ty()))
            }
        }
        PKind::Buf => quote!(::goose::meta::ParamKind::Buf),
        PKind::TypedBuf(t) => quote!(::goose::meta::ParamKind::TypedBuf(<#t as ::goose::Value>::goose_ty())),
        PKind::Ref { ty, mutable } => quote!(<#ty as ::goose::Arg>::ref_kind(#mutable)),
        PKind::Val(ty) => quote!(<#ty as ::goose::Arg>::val_kind()),
        PKind::Optional(inner) => {
            let i = param_meta(inner);
            quote!(::goose::meta::ParamKind::Optional(::std::boxed::Box::new(#i)))
        }
        PKind::Reentry => quote!(unreachable!()),
        PKind::Callback(cb) => {
            let args = cb.args.iter().map(|a| match a {
                CbArgKind::Scalar(p, _) => {
                    let g = p.goose();
                    quote!(::goose::meta::CbArg::Scalar(::goose::meta::Ty::Prim(#g)))
                }
                CbArgKind::Value(t, _) => quote!(::goose::meta::CbArg::Value(<#t as ::goose::Value>::goose_ty())),
                CbArgKind::Str => quote!(::goose::meta::CbArg::Str),
            });
            let ret = match &cb.ret {
                None => quote!(::core::option::Option::None),
                Some(CbRet::Scalar(p)) => {
                    let g = p.goose();
                    quote!(::core::option::Option::Some(::goose::meta::Ty::Prim(#g)))
                }
                Some(CbRet::Value(t)) => quote!(::core::option::Option::Some(<#t as ::goose::Value>::goose_ty())),
            };
            quote!(::goose::meta::ParamKind::Callback { args: ::std::vec![#(#args),*], ret: #ret })
        }
    }
}

fn param_register(kind: &PKind) -> TokenStream {
    match kind {
        PKind::Slice { elem, .. } => quote!(<#elem as ::goose::Value>::goose_register(lib);),
        PKind::TypedBuf(t) => quote!(<#t as ::goose::Value>::goose_register(lib);),
        PKind::Ref { ty, .. } | PKind::Val(ty) => quote!(<#ty as ::goose::Arg>::register(lib);),
        PKind::Optional(i) => param_register(i),
        PKind::Callback(cb) => {
            let mut out = TokenStream::new();
            for a in &cb.args {
                if let CbArgKind::Value(t, _) = a {
                    out.extend(quote!(<#t as ::goose::Value>::goose_register(lib);));
                }
            }
            if let Some(CbRet::Value(t)) = &cb.ret {
                out.extend(quote!(<#t as ::goose::Value>::goose_register(lib);));
            }
            out
        }
        _ => quote!(),
    }
}

fn belem_meta(e: &BElem) -> TokenStream {
    match e {
        BElem::Str | BElem::Bytes => quote!(::core::option::Option::None),
        BElem::Slice(t) => quote!(::core::option::Option::Some(<#t as ::goose::Value>::goose_ty())),
    }
}

fn ret_meta(r: &RKind, iter_name: &str) -> TokenStream {
    match r {
        RKind::Unit => quote!(::goose::meta::RetKind::Unit),
        RKind::Scalar(p) => {
            let g = p.goose();
            quote!(::goose::meta::RetKind::Scalar(::goose::meta::Ty::Prim(#g)))
        }
        RKind::Out(t) => quote!(<#t as ::goose::Out>::ret_kind()),
        RKind::Bytes | RKind::Copied(BElem::Str) | RKind::Copied(BElem::Bytes) => quote!(::goose::meta::RetKind::Bytes),
        RKind::Vec(t) | RKind::Copied(BElem::Slice(t)) => {
            quote!(::goose::meta::RetKind::Vec(<#t as ::goose::Value>::goose_ty()))
        }
        RKind::Borrowed { param, elem } => {
            let e = belem_meta(elem);
            quote!(::goose::meta::RetKind::Borrowed { param: #param, elem: #e })
        }
        RKind::Spans { param, elem } => {
            let e = belem_meta(elem);
            quote!(::goose::meta::RetKind::Spans { param: #param, elem: #e })
        }
        RKind::Option(i) => {
            let i = ret_meta(i, iter_name);
            quote!(::goose::meta::RetKind::Option(::std::boxed::Box::new(#i)))
        }
        RKind::Result(i) => {
            let i = ret_meta(i, iter_name);
            quote!(::goose::meta::RetKind::Result(::std::boxed::Box::new(#i)))
        }
        RKind::Iter { item, .. } => {
            let i = ret_meta(item, iter_name);
            quote!(::goose::meta::RetKind::Iter { item: ::std::boxed::Box::new(#i), handle: #iter_name.to_string() })
        }
    }
}

fn ret_register(r: &RKind) -> TokenStream {
    match r {
        RKind::Out(t) => quote!(<#t as ::goose::Out>::register(lib);),
        RKind::Vec(t) | RKind::Copied(BElem::Slice(t)) => quote!(<#t as ::goose::Value>::goose_register(lib);),
        RKind::Borrowed { elem: BElem::Slice(t), .. } | RKind::Spans { elem: BElem::Slice(t), .. } => {
            quote!(<#t as ::goose::Value>::goose_register(lib);)
        }
        RKind::Option(i) | RKind::Result(i) => ret_register(i),
        RKind::Iter { item, .. } => ret_register(item),
        _ => quote!(),
    }
}

fn function(f: &ItemFn, inst: Option<&(Ident, Type)>, opts: &Options) -> syn::Result<TokenStream> {
    let info = crate_info();
    let ns = &info.namespace;
    let fname = &f.sig.ident;
    let rust_name = fname.to_string();
    let rust_name = rust_name.strip_prefix("r#").unwrap_or(&rust_name).to_string();
    let goose_name = opts.name.clone().unwrap_or_else(|| rust_name.clone());
    let mut sig = f.sig.clone();
    let mut suffix = String::new();
    if let Some((from, to)) = inst {
        let mut s = Subst { from, to };
        for input in sig.inputs.iter_mut() {
            syn::visit_mut::visit_fn_arg_mut(&mut s, input);
        }
        syn::visit_mut::visit_return_type_mut(&mut s, &mut sig.output);
        suffix = format!("_{}", type_suffix(to));
    }
    let base = match (&opts.symbol, inst) {
        (Some(s), None) => s.clone(),
        (Some(s), Some(_)) => format!("{s}{suffix}"),
        (None, _) => format!("{ns}_{goose_name}{suffix}"),
    };
    let qualified = format!("{ns}::{goose_name}");

    let mut params = Vec::new();
    for (i, input) in sig.inputs.iter().enumerate() {
        let FnArg::Typed(pt) = input else { unreachable!() };
        let name = match &*pt.pat {
            Pat::Ident(pi) => {
                let s = pi.ident.to_string();
                s.strip_prefix("r#").unwrap_or(&s).trim_start_matches('_').to_string()
            }
            _ => format!("arg{i}"),
        };
        let name = if name.is_empty() { format!("arg{i}") } else { name };
        let kind = classify_param(&pt.ty)?;
        params.push(Param { name, ty: (*pt.ty).clone(), kind });
    }
    let triples: Vec<(String, Type, PKind)> =
        params.iter().map(|p| (p.name.clone(), p.ty.clone(), p.kind.clone())).collect();
    let ret = classify_ret(&sig.output, &triples)?;

    let callbacks: Vec<usize> =
        params.iter().enumerate().filter(|(_, p)| matches!(p.kind, PKind::Callback(_))).map(|(i, _)| i).collect();
    let optionals: Vec<usize> =
        params.iter().enumerate().filter(|(_, p)| matches!(p.kind, PKind::Optional(_))).map(|(i, _)| i).collect();
    if callbacks.len() > 1 {
        return Err(syn::Error::new(f.sig.span(), "goose::export: at most one callback parameter"));
    }
    if optionals.len() > 3 {
        return Err(syn::Error::new(
            f.sig.span(),
            "goose::export: at most three Option parameters (one entry point per combination)",
        ));
    }
    let reentrant = params.iter().any(|p| matches!(p.kind, PKind::Reentry));
    if reentrant && params.iter().any(|p| matches!(p.kind, PKind::Buf | PKind::TypedBuf(_) | PKind::Callback(_))) {
        return Err(syn::Error::new(
            f.sig.span(),
            "goose::export: a function calling Goose back takes no builder (it would be live during the call back) and no callback",
        ));
    }
    if !callbacks.is_empty() && !optionals.is_empty() {
        return Err(syn::Error::new(f.sig.span(), "goose::export: a function taking a callback cannot take Option parameters too"));
    }

    let turbofish = match inst {
        Some((_, t)) => quote!(::<#t>),
        None => quote!(),
    };
    let doc = doc_of(&f.attrs);
    let mut out = TokenStream::new();
    let iter_name = format!("{}Iter", camel(&goose_name));
    let iter_ty = format_ident!("__GooseIter_{}", base);

    if let Some(&ci) = callbacks.first() {
        out.extend(callback_shims(f, &params, ci, &ret, &base, &qualified, opts.main_thread, &turbofish)?);
    } else {
        // One entry point per combination of the optional parameters.
        let combos = 1usize << optionals.len();
        for mask in 0..combos {
            let mut l = Lowered::default();
            let mut sym = base.clone();
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
            for (i, p) in params.iter().enumerate() {
                if let PKind::Optional(inner) = &p.kind {
                    let k = optionals.iter().position(|&o| o == i).unwrap();
                    if mask & (1 << k) != 0 {
                        lower_param(&mut l, i, p, inner, true);
                    } else {
                        l.args.push(quote!(::core::option::Option::None));
                    }
                } else {
                    lower_param(&mut l, i, p, &p.kind, false);
                }
            }
            let args = &l.args;
            let call = quote!(#fname #turbofish (#(#args),*));
            let cx = RetCtx { bases: &l.bases, params: &params, stash: None, iter_ty: Some(iter_ty.clone()) };
            out.extend(shim(&sym, &qualified, opts.main_thread, &l, &call, &ret, &cx));
            if ret.needs_typed() {
                // Counted, then copied: for a compiler without typed builders.
                let len_sym = format!("{sym}__len");
                let key = format!("{base}__stash");
                let cx = RetCtx { bases: &l.bases, params: &params, stash: Some(key), iter_ty: None };
                out.extend(shim(&len_sym, &qualified, opts.main_thread, &l, &call, &ret, &cx));
            }
        }
        if ret.needs_typed() {
            let take = format_ident!("{}__take", base);
            let key = format!("{base}__stash");
            let elem = typed_elem(&ret).unwrap();
            out.extend(quote! {
                #[unsafe(no_mangle)]
                #[doc(hidden)]
                #[allow(improper_ctypes_definitions, non_snake_case, clippy::all)]
                pub unsafe extern "C" fn #take(out: ::goose::GooseSlice<#elem>) {
                    static SITE: ::goose::Site = ::goose::Site { name: #qualified, fallible: false, main_thread: false };
                    ::goose::call(&SITE, || unsafe { ::goose::stash_take::<#elem>(#key, out) }, |_m: &str| ())
                }
            });
        }
        if let RKind::Iter { item, item_ty } = &ret {
            out.extend(iter_shims(&iter_ty, item, item_ty, &base, &qualified, &iter_name, &doc));
        }
    }

    // The record cargo goose reads back.
    let meta_fn = format_ident!("__goose_meta_fn_{}", base);
    let pdefs = params.iter().filter(|p| !matches!(p.kind, PKind::Reentry)).map(|p| {
        let n = &p.name;
        let k = param_meta(&p.kind);
        quote!(::goose::meta::ParamDef { name: #n.to_string(), kind: #k })
    });
    let regs: Vec<TokenStream> = params.iter().map(|p| param_register(&p.kind)).collect();
    let rmeta = ret_meta(&ret, &iter_name);
    let rreg = ret_register(&ret);
    // The base shim's C signature, as lowered here, for cargo goose to check.
    let mut l0 = Lowered::default();
    if optionals.is_empty() && callbacks.is_empty() {
        for (i, p) in params.iter().enumerate() {
            lower_param(&mut l0, i, p, &p.kind, false);
        }
        let rl = ret_lower(&ret, false);
        let shapes_extra = rl.shapes;
        let c_ret_shape = rl.c_ret_shape;
        let shapes = &l0.shapes;
        let throws = opts.throws;
        let mt = opts.main_thread;
        out.extend(quote! {
            #[doc(hidden)]
            #[allow(non_snake_case)]
            fn #meta_fn(lib: &mut ::goose::meta::Library) {
                lib.functions.push(::goose::meta::FnDef {
                    name: #goose_name.to_string(),
                    symbol: #base.to_string(),
                    doc: #doc.to_string(),
                    params: ::std::vec![#(#pdefs),*],
                    ret: #rmeta,
                    throws: #throws,
                    main_thread: #mt,
                    reentrant: #reentrant,
                    c_params: ::std::vec![#(::std::string::String::from(#shapes),)* #(::std::string::String::from(#shapes_extra)),*],
                    c_ret: #c_ret_shape.to_string(),
                });
                #(#regs)*
                #rreg
            }
            ::goose::__private::inventory::submit! { ::goose::meta::Entry { add: #meta_fn } }
        });
    } else {
        let throws = opts.throws;
        let mt = opts.main_thread;
        out.extend(quote! {
            #[doc(hidden)]
            #[allow(non_snake_case)]
            fn #meta_fn(lib: &mut ::goose::meta::Library) {
                lib.functions.push(::goose::meta::FnDef {
                    name: #goose_name.to_string(),
                    symbol: #base.to_string(),
                    doc: #doc.to_string(),
                    params: ::std::vec![#(#pdefs),*],
                    ret: #rmeta,
                    throws: #throws,
                    main_thread: #mt,
                    reentrant: #reentrant,
                    c_params: ::std::vec::Vec::new(),
                    c_ret: ::std::string::String::new(),
                });
                #(#regs)*
                #rreg
            }
            ::goose::__private::inventory::submit! { ::goose::meta::Entry { add: #meta_fn } }
        });
    }
    Ok(out)
}

/// A lazy iterator's handle type and its `next` and `release`.
fn iter_shims(
    iter_ty: &Ident,
    item: &RKind,
    item_ty: &Type,
    base: &str,
    qualified: &str,
    iter_name: &str,
    doc: &str,
) -> TokenStream {
    let next = format_ident!("{}__next", base);
    let release = format_ident!("{}__release", base);
    let release_s = format!("{base}__release");
    let table_name = format!("{}::{}", crate_info().namespace, iter_name);
    let mut c = Vec::new();
    let mut s = Vec::new();
    ret_out_params(item, true, false, &mut c, &mut s);
    let empty = HashMap::new();
    let cx = RetCtx { bases: &empty, params: &[], stash: None, iter_ty: None };
    let w = write_inner(item, quote!(__x), &cx);
    let meta_fn = format_ident!("__goose_meta_iter_{}", base);
    let doc = format!("The items of {qualified}, one per next.\n{doc}");
    quote! {
        #[doc(hidden)]
        #[allow(non_camel_case_types)]
        pub struct #iter_ty(::std::boxed::Box<dyn ::core::iter::Iterator<Item = #item_ty>>);

        impl ::goose::HandleType for #iter_ty {
            fn table() -> &'static ::goose::HandleTable<Self> {
                static TABLE: ::goose::HandleTable<#iter_ty> = unsafe { ::goose::HandleTable::new_pinned(#table_name) };
                &TABLE
            }
        }

        #[unsafe(no_mangle)]
        #[doc(hidden)]
        #[allow(improper_ctypes_definitions, unused_unsafe, non_snake_case, clippy::all)]
        pub unsafe extern "C" fn #next(it: ::goose::Handle, #(#c),*) -> bool {
            static SITE: ::goose::Site = ::goose::Site { name: #qualified, fallible: false, main_thread: false };
            ::goose::call(&SITE, || {
                let mut __it = <#iter_ty as ::goose::HandleType>::table().exclusive(it)?;
                match __it.0.next() {
                    ::core::option::Option::Some(__x) => { #w ::core::result::Result::Ok(true) }
                    ::core::option::Option::None => ::core::result::Result::Ok(false),
                }
            }, |_m: &str| false)
        }

        #[unsafe(no_mangle)]
        #[doc(hidden)]
        pub extern "C" fn #release(it: ::goose::Handle) {
            static SITE: ::goose::Site = ::goose::Site { name: #qualified, fallible: false, main_thread: false };
            ::goose::call(&SITE, || <#iter_ty as ::goose::HandleType>::table().release(it), |_m: &str| ())
        }

        #[doc(hidden)]
        #[allow(non_snake_case)]
        fn #meta_fn(lib: &mut ::goose::meta::Library) {
            lib.handles.insert(#iter_name.to_string(), ::goose::meta::HandleDef {
                name: #iter_name.to_string(),
                doc: #doc.to_string(),
                pinned: true,
                release: #release_s.to_string(),
                live_count: ::std::string::String::new(),
            });
        }
        ::goose::__private::inventory::submit! { ::goose::meta::Entry { add: #meta_fn } }
    }
}

/// A function taking a callback runs as a coroutine: `start` makes it,
/// each `next` runs it to its next call of the callback (handing Goose the
/// arguments) or to its end, `reply` gives the callback's result back, and
/// `finish` sends the function's own result.
#[allow(clippy::too_many_arguments)]
fn callback_shims(
    f: &ItemFn,
    params: &[Param],
    ci: usize,
    ret: &RKind,
    base: &str,
    qualified: &str,
    main_thread: bool,
    turbofish: &TokenStream,
) -> syn::Result<TokenStream> {
    let PKind::Callback(cb) = &params[ci].kind else { unreachable!() };
    if matches!(ret, RKind::Borrowed { .. } | RKind::Spans { .. } | RKind::Iter { .. }) || ret.needs_typed() {
        return Err(syn::Error::new(
            f.sig.span(),
            "goose::export: a function taking a callback returns a scalar, a value, a handle, String, Option or Result of those",
        ));
    }
    let fname = &f.sig.ident;
    let sess = format_ident!("__GooseCb_{}", base);
    let table_name = format!("{qualified} call");
    let ret_ty: TokenStream = match &f.sig.output {
        syn::ReturnType::Default => quote!(()),
        syn::ReturnType::Type(_, t) => quote!(#t),
    };
    let reply_ty: TokenStream = match &cb.ret {
        None => quote!(()),
        Some(CbRet::Scalar(p)) => p.rust(),
        Some(CbRet::Value(t)) => quote!(#t),
    };
    let ev_owned: Vec<TokenStream> = cb
        .args
        .iter()
        .map(|a| match a {
            CbArgKind::Scalar(p, _) => p.rust(),
            CbArgKind::Value(t, _) => quote!(#t),
            CbArgKind::Str => quote!(::std::string::String),
        })
        .collect();

    // start: the arguments held, then made owned for the coroutine.
    let mut l = Lowered::default();
    let mut owned = Vec::new();
    let mut inner_args = Vec::new();
    for (i, p) in params.iter().enumerate() {
        let o = format_ident!("__o{}", i);
        if i == ci {
            inner_args.push(if cb.by_ref { quote!(&mut __cb) } else { quote!(__cb) });
            continue;
        }
        match &p.kind {
            PKind::Buf | PKind::TypedBuf(_) | PKind::Slice { mutable: true, .. } | PKind::Ref { mutable: true, .. } => {
                return Err(syn::Error::new(
                    p.ty.span(),
                    "goose::export: a function taking a callback runs between Goose calls, so it takes no &mut arguments or builders",
                ));
            }
            _ => {}
        }
        lower_param(&mut l, i, p, &p.kind, false);
        let a = l.args.pop().unwrap();
        match &p.kind {
            PKind::Str { owned: false } => {
                owned.push(quote! { let #o: ::std::string::String = ::std::string::String::from(#a); });
                inner_args.push(quote!(#o.as_str()));
            }
            PKind::Bytes { owned: false } => {
                owned.push(quote! { let #o: ::std::vec::Vec<u8> = (#a).to_vec(); });
                inner_args.push(quote!(&#o[..]));
            }
            PKind::Slice { elem, owned: false, .. } => {
                owned.push(quote! { let #o: ::std::vec::Vec<#elem> = (#a).to_vec(); });
                inner_args.push(quote!(&#o[..]));
            }
            PKind::Ref { ty, .. } => {
                // What outlives the call: a copy of a value, a handle's guard.
                let h = format_ident!("__h{}", i);
                owned.push(quote! { let #o = <#ty as ::goose::Arg>::keep(#h); });
                inner_args.push(quote!(&*#o));
            }
            _ => {
                owned.push(quote! { let #o = #a; });
                inner_args.push(quote!(#o));
            }
        }
    }
    let cb_params: Vec<TokenStream> = cb
        .arg_types
        .iter()
        .enumerate()
        .map(|(k, t)| {
            let a = format_ident!("__c{}", k);
            quote!(#a: #t)
        })
        .collect();
    let cb_owned: Vec<TokenStream> = cb
        .args
        .iter()
        .enumerate()
        .map(|(k, a)| {
            let v = format_ident!("__c{}", k);
            match a {
                CbArgKind::Str => quote!(::std::string::String::from(::core::convert::AsRef::<str>::as_ref(&#v))),
                CbArgKind::Scalar(_, true) | CbArgKind::Value(_, true) => quote!(*#v),
                CbArgKind::Scalar(_, false) | CbArgKind::Value(_, false) => quote!(#v),
            }
        })
        .collect();
    let ev_tuple = quote!((#(#ev_owned,)*));
    let start = format_ident!("{}__start", base);
    let next = format_ident!("{}__next", base);
    let reply = format_ident!("{}__reply", base);
    let finish = format_ident!("{}__finish", base);
    let c_params = &l.c_params;
    let holds = &l.holds;
    let takes = &l.takes;
    let overlap = overlap_check(&l);

    // next: the callback's arguments go out, one out-parameter each.
    let mut ev_params = Vec::new();
    let mut ev_writes = Vec::new();
    for (k, a) in cb.args.iter().enumerate() {
        let e = format_ident!("__ev{}", k);
        let idx = syn::Index::from(k);
        match a {
            CbArgKind::Scalar(p, _) => {
                let rt = p.raw();
                let rv = p.to_raw(quote!(__ev.#idx));
                ev_params.push(quote!(#e: *mut #rt));
                ev_writes.push(quote! { unsafe { ::core::ptr::write_unaligned(#e, #rv) }; });
            }
            CbArgKind::Value(t, _) => {
                ev_params.push(quote!(#e: *mut u8));
                ev_writes.push(quote! { unsafe { <#t as ::goose::Value>::goose_write(&__ev.#idx, #e) }; });
            }
            CbArgKind::Str => {
                ev_params.push(quote!(#e: ::goose::GsRref));
                ev_writes.push(quote! { unsafe { ::goose::GooseBuf::from_raw(#e) }.push_str(&__ev.#idx); });
            }
        }
    }
    let unit_reply = cb.ret.is_none();
    let auto_reply = if unit_reply { quote! { __s.reply = ::core::option::Option::Some(()); } } else { quote!() };

    let reply_shim = match &cb.ret {
        None => quote!(),
        Some(r) => {
            let (raw_ty, conv) = match r {
                CbRet::Scalar(p) => {
                    let rt = p.raw();
                    let a = id("__v");
                    let raw = id("r");
                    let c = p.from_raw(&raw, &a, "reply");
                    (quote!(#rt), quote! { #c })
                }
                CbRet::Value(t) => (
                    quote!(*const u8),
                    quote! {
                        let __v: #t = unsafe { <#t as ::goose::Value>::goose_read(r) }.ok_or_else(||
                            ::goose::BridgeError::Abort(::std::string::String::from("the callback's result is not a valid value")))?;
                    },
                ),
            };
            quote! {
                #[unsafe(no_mangle)]
                #[doc(hidden)]
                #[allow(improper_ctypes_definitions, unused_unsafe, non_snake_case, clippy::all)]
                pub unsafe extern "C" fn #reply(s: ::goose::Handle, r: #raw_ty) {
                    static SITE: ::goose::Site = ::goose::Site { name: #qualified, fallible: false, main_thread: #main_thread };
                    ::goose::call(&SITE, || {
                        #conv
                        let mut __s = <#sess as ::goose::HandleType>::table().exclusive(s)?;
                        __s.reply = ::core::option::Option::Some(__v);
                        ::core::result::Result::Ok(())
                    }, |_m: &str| ())
                }
            }
        }
    };

    let rl = ret_lower(ret, false);
    let rp = &rl.c_params;
    let c_ret = &rl.c_ret;
    let empty = HashMap::new();
    let cx = RetCtx { bases: &empty, params, stash: None, iter_ty: None };
    let write = write_top(ret, quote!(__r), &cx);
    let fail = fail_closure(ret, false);
    let fallible = ret.fallible();

    Ok(quote! {
        #[doc(hidden)]
        #[allow(non_camel_case_types)]
        pub struct #sess {
            co: ::goose::__private::Coroutine<::core::option::Option<#reply_ty>, #ev_tuple, #ret_ty>,
            reply: ::core::option::Option<#reply_ty>,
            result: ::core::option::Option<#ret_ty>,
        }

        impl ::goose::HandleType for #sess {
            fn table() -> &'static ::goose::HandleTable<Self> {
                static TABLE: ::goose::HandleTable<#sess> = unsafe { ::goose::HandleTable::new_pinned(#table_name) };
                &TABLE
            }
        }

        #[unsafe(no_mangle)]
        #[doc(hidden)]
        #[allow(improper_ctypes_definitions, unused_mut, unused_variables, unused_unsafe, non_snake_case, clippy::all)]
        pub unsafe extern "C" fn #start(#(#c_params,)* __sess: *mut ::goose::Handle) {
            static SITE: ::goose::Site = ::goose::Site { name: #qualified, fallible: false, main_thread: #main_thread };
            ::goose::call(&SITE, || {
                #overlap
                #(#holds)*
                #(#takes)*
                #(#owned)*
                let __co = ::goose::__private::Coroutine::new(
                    move |__y: &::goose::__private::Yielder<::core::option::Option<#reply_ty>, #ev_tuple>,
                          _first: ::core::option::Option<#reply_ty>| -> #ret_ty {
                        let mut __cb = |#(#cb_params),*| -> #reply_ty {
                            let __reply = __y.suspend((#(#cb_owned,)*));
                            __reply.expect("the Goose wrapper replies before it resumes")
                        };
                        #fname #turbofish (#(#inner_args),*)
                    },
                );
                let __h = <#sess as ::goose::HandleType>::table().insert(#sess {
                    co: __co,
                    reply: ::core::option::Option::None,
                    result: ::core::option::Option::None,
                });
                unsafe { ::core::ptr::write_unaligned(__sess, __h) };
                ::core::result::Result::Ok(())
            }, |_m: &str| ())
        }

        #[unsafe(no_mangle)]
        #[doc(hidden)]
        #[allow(improper_ctypes_definitions, unused_unsafe, non_snake_case, clippy::all)]
        pub unsafe extern "C" fn #next(s: ::goose::Handle, #(#ev_params),*) -> bool {
            static SITE: ::goose::Site = ::goose::Site { name: #qualified, fallible: false, main_thread: #main_thread };
            ::goose::call(&SITE, || {
                let mut __s = <#sess as ::goose::HandleType>::table().exclusive(s)?;
                if __s.result.is_some() {
                    return ::core::result::Result::Ok(false);
                }
                let __input = __s.reply.take();
                match __s.co.resume(__input) {
                    ::goose::__private::CoroutineResult::Yield(__ev) => {
                        #(#ev_writes)*
                        #auto_reply
                        ::core::result::Result::Ok(true)
                    }
                    ::goose::__private::CoroutineResult::Return(__r) => {
                        __s.result = ::core::option::Option::Some(__r);
                        ::core::result::Result::Ok(false)
                    }
                }
            }, |_m: &str| false)
        }

        #reply_shim

        #[unsafe(no_mangle)]
        #[doc(hidden)]
        #[allow(improper_ctypes_definitions, unused_mut, unused_variables, unused_unsafe, non_snake_case, clippy::all)]
        pub unsafe extern "C" fn #finish(s: ::goose::Handle, #(#rp),*) -> #c_ret {
            static SITE: ::goose::Site = ::goose::Site { name: #qualified, fallible: #fallible, main_thread: #main_thread };
            ::goose::call(&SITE, || {
                let __s = <#sess as ::goose::HandleType>::table().take(s)?;
                let __r = __s.result.ok_or_else(|| ::goose::BridgeError::Abort(
                    ::std::string::String::from("finished before the function returned")))?;
                #write
            }, #fail)
        }
    })
}
