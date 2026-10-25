//! `goose::callbacks!`: Goose export fns Rust calls back while Goose waits
//! on a call (goose-patches/0008). Each declared signature becomes a
//! function taking the call's `&goose::Reentry` token, which finds the
//! export fn by name and Goose signature through the runtime (once) and
//! calls its C wrapper.

use crate::info::crate_info;
use crate::types::{prim_of, Prim};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{spanned::Spanned, Attribute, FnArg, Pat, ReturnType, Signature, Token, Type};

pub struct Decls(Vec<(Vec<Attribute>, Signature)>);

impl Parse for Decls {
    fn parse(input: ParseStream) -> syn::Result<Decls> {
        let mut v = Vec::new();
        while !input.is_empty() {
            let attrs = input.call(Attribute::parse_outer)?;
            let _vis: syn::Visibility = input.parse()?;
            let sig: Signature = input.parse()?;
            input.parse::<Token![;]>()?;
            v.push((attrs, sig));
        }
        Ok(Decls(v))
    }
}

fn reject(t: &impl Spanned, msg: &str) -> syn::Error {
    syn::Error::new(t.span(), format!("goose::callbacks: {msg}"))
}

pub fn expand(input: TokenStream) -> syn::Result<TokenStream> {
    let decls: Decls = syn::parse2(input)?;
    let ns = &crate_info().namespace;
    let mut out = TokenStream::new();
    for (attrs, sig) in decls.0 {
        if !sig.generics.params.is_empty() || sig.asyncness.is_some() {
            return Err(reject(&sig, "a call back is a plain function: no generics, no async"));
        }
        let name = &sig.ident;
        let name_s = name.to_string();
        let mut params = Vec::new();
        let mut raws = Vec::new();
        let mut convs = Vec::new();
        let mut texts = Vec::new();
        let mut checks = Vec::new();
        for (i, a) in sig.inputs.iter().enumerate() {
            let FnArg::Typed(pt) = a else { return Err(reject(a, "no self")) };
            let n = match &*pt.pat {
                Pat::Ident(p) => p.ident.clone(),
                _ => format_ident!("a{}", i),
            };
            let ty = &*pt.ty;
            params.push(quote!(#n: #ty));
            if let Some(p) = prim_of(ty) {
                let g = p.goose();
                raws.push(p.raw());
                convs.push(p.to_raw(quote!(#n)));
                texts.push(quote!(::std::string::String::from(#g)));
                continue;
            }
            let Type::Reference(r) = ty else {
                return Err(reject(ty, "a call back takes scalars, &str, slices and references to goose::Flat values"));
            };
            let mutable = r.mutability.is_some();
            match &*r.elem {
                Type::Path(p) if p.path.is_ident("str") => {
                    raws.push(quote!(::goose::GooseSlice<u8>));
                    convs.push(quote!(::goose::GooseSlice::from_slice(#n.as_bytes())));
                    texts.push(quote!(::std::string::String::from("const u8[:]")));
                }
                Type::Slice(s) => {
                    let e = &*s.elem;
                    raws.push(quote!(::goose::GooseSlice<#e>));
                    checks.push(quote! {
                        const { assert!(<#e as ::goose::Value>::SAME_LAYOUT, "a slice passed to Goose is laid out as Goose lays it out") };
                    });
                    if mutable {
                        convs.push(quote!(::goose::GooseSlice::from_mut_slice(#n)));
                    } else {
                        convs.push(quote!(::goose::GooseSlice::from_slice(#n)));
                    }
                    texts.push(quote!(::goose::reentry::ref_text::<#e>(#ns, #mutable, true)));
                }
                e => {
                    checks.push(quote! {
                        const { assert!(<#e as ::goose::Value>::SAME_LAYOUT, "a value passed to Goose is laid out as Goose lays it out") };
                    });
                    if mutable {
                        raws.push(quote!(*mut #e));
                        convs.push(quote!(#n as *mut #e));
                    } else {
                        raws.push(quote!(*const #e));
                        convs.push(quote!(#n as *const #e));
                    }
                    texts.push(quote!(::goose::reentry::ref_text::<#e>(#ns, #mutable, false)));
                }
            }
        }
        let (ret_ty, ret_raw, ret_conv, ret_text) = match &sig.output {
            ReturnType::Default => (quote!(()), quote!(()), quote!(()), quote!(::std::string::String::new())),
            ReturnType::Type(_, t) => match prim_of(t) {
                Some(Prim::Bool) => (quote!(bool), quote!(u8), quote!(__r != 0), quote!(::std::string::String::from("bool"))),
                Some(p) => {
                    let g = p.goose();
                    let rust = p.rust();
                    let raw = p.raw();
                    (quote!(#rust), quote!(#raw), quote!(__r as #rust), quote!(::std::string::String::from(#g)))
                }
                None => return Err(reject(t, "a call back returns a scalar or nothing")),
            },
        };
        out.extend(quote! {
            #(#attrs)*
            #[allow(clippy::all)]
            pub fn #name(goose: &::goose::Reentry<'_>, #(#params),*) -> ::core::result::Result<#ret_ty, ::goose::ReentryError> {
                static SLOT: ::goose::ReentrySlot = ::goose::ReentrySlot::new(#name_s);
                #(#checks)*
                let __f = SLOT.get(goose, || {
                    let __params: ::std::vec::Vec<::std::string::String> = ::std::vec![#(#texts),*];
                    ::std::format!("{}->{}", __params.join(","), #ret_text)
                })?;
                let __f: unsafe extern "C" fn(#(#raws),*) -> #ret_raw = unsafe { ::core::mem::transmute(__f) };
                let __r = unsafe { __f(#(#convs),*) };
                ::core::result::Result::Ok(#ret_conv)
            }
        });
    }
    Ok(out)
}
