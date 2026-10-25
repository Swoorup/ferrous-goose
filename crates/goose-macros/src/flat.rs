//! `#[derive(goose::Flat)]`: a Goose declaration for a `#[repr(C)]` struct
//! or an enum, with the layout rustc gave it.
//!
//! A struct whose fields Goose stores as Rust does gets explicit pads where
//! rustc put padding, and crosses in place. One holding an enum with data
//! (whose Goose layout is a tag and a packed payload) is converted at the
//! boundary, field by field, unless the enum is `#[goose(align)]`: a
//! `#[repr(C, u8)]` enum Goose lays out as Rust does, with `align n`
//! (goose-patches/0007), which crosses in place.

use crate::doc_of;
use crate::info::library_info;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{spanned::Spanned, Data, DeriveInput, Fields, Type};

pub fn derive(input: DeriveInput) -> syn::Result<TokenStream> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new(
            input.generics.span(),
            "goose::Flat: a generic type has no single Goose declaration; give each instantiation a type of its own",
        ));
    }
    let name = &input.ident;
    let doc = doc_of(&input.attrs);
    let reprs = reprs(&input.attrs);
    let body = match &input.data {
        Data::Struct(s) => {
            if !reprs.iter().any(|r| r == "C" || r == "transparent") {
                return Err(syn::Error::new(
                    name.span(),
                    "goose::Flat: a struct needs #[repr(C)], so its fields stay in declaration order where Goose declares them",
                ));
            }
            derive_struct(name, &doc, &s.fields)?
        }
        Data::Enum(e) => {
            if goose_align(&input.attrs)? {
                derive_aligned_enum(name, &doc, e, &reprs)?
            } else {
                derive_enum(name, &doc, e, &reprs)?
            }
        }
        Data::Union(u) => {
            return Err(syn::Error::new(u.union_token.span, "goose::Flat: unions cannot cross to Goose"));
        }
    };
    let info = library_info();
    let meta_fn = format_ident!("__goose_meta_type_{}", name);
    Ok(quote! {
        #body
        ::goose::__value_arg!(#name);
        #[doc(hidden)]
        #[allow(non_snake_case)]
        fn #meta_fn(lib: &mut ::goose::meta::Library) {
            <#name as ::goose::Value>::goose_register(lib)
        }
        ::goose::__private::inventory::submit! { ::goose::meta::Entry { add: #meta_fn } }
        #info
    })
}

/// Whether `#[goose(align)]` is on the item.
fn goose_align(attrs: &[syn::Attribute]) -> syn::Result<bool> {
    let mut align = false;
    for a in attrs {
        if a.path().is_ident("goose") {
            a.parse_nested_meta(|m| {
                if m.path.is_ident("align") {
                    align = true;
                    Ok(())
                } else {
                    Err(m.error("#[goose(..)] on a type takes align"))
                }
            })?;
        }
    }
    Ok(align)
}

fn reprs(attrs: &[syn::Attribute]) -> Vec<String> {
    let mut out = Vec::new();
    for a in attrs {
        if a.path().is_ident("repr") {
            let _ = a.parse_nested_meta(|m| {
                if let Some(i) = m.path.get_ident() {
                    out.push(i.to_string());
                }
                if m.input.peek(syn::token::Paren) {
                    let content;
                    syn::parenthesized!(content in m.input);
                    let _: TokenStream = content.parse()?;
                }
                Ok(())
            });
        }
    }
    out
}

/// The fields as (Rust accessor, Goose name, type).
fn field_list(fields: &Fields) -> Vec<(TokenStream, String, Type)> {
    match fields {
        Fields::Named(n) => n
            .named
            .iter()
            .map(|f| {
                let id = f.ident.as_ref().unwrap();
                let s = id.to_string();
                let s = s.strip_prefix("r#").unwrap_or(&s).to_string();
                (quote!(#id), s, f.ty.clone())
            })
            .collect(),
        Fields::Unnamed(u) => u
            .unnamed
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let idx = syn::Index::from(i);
                (quote!(#idx), format!("f{i}"), f.ty.clone())
            })
            .collect(),
        Fields::Unit => Vec::new(),
    }
}

fn derive_struct(name: &syn::Ident, doc: &str, fields: &Fields) -> syn::Result<TokenStream> {
    let fl = field_list(fields);
    let name_s = name.to_string();
    let tys: Vec<&Type> = fl.iter().map(|f| &f.2).collect();
    let accessors: Vec<&TokenStream> = fl.iter().map(|f| &f.0).collect();
    let gnames: Vec<&String> = fl.iter().map(|f| &f.1).collect();

    // Goose offsets when converting: each field packed after the last.
    let mut goffs = Vec::new();
    let mut acc = quote!(0usize);
    for t in &tys {
        goffs.push(acc.clone());
        acc = quote!(#acc + <#t as ::goose::Value>::GOOSE_SIZE);
    }
    let packed_size = acc;

    let same = if tys.is_empty() {
        quote!(true)
    } else {
        quote!(#(<#tys as ::goose::Value>::SAME_LAYOUT)&&*)
    };
    let checked = if tys.is_empty() {
        quote!(false)
    } else {
        quote!(#(<#tys as ::goose::Value>::CHECKED)||*)
    };

    let fixup_flag = if tys.is_empty() {
        quote!(false)
    } else {
        quote!(#(<#tys as ::goose::Value>::FIXUP)||*)
    };
    let fixups = tys.iter().zip(&accessors).map(|(t, a)| {
        quote! {
            if <#t as ::goose::Value>::FIXUP {
                <#t as ::goose::Value>::goose_fixup(p.add(::core::mem::offset_of!(#name, #a)));
            }
        }
    });
    let valid = tys.iter().zip(&accessors).map(|(t, a)| {
        quote! {
            (!<#t as ::goose::Value>::CHECKED
                || <#t as ::goose::Value>::goose_valid(p.add(::core::mem::offset_of!(#name, #a))))
        }
    });
    let reads = tys.iter().zip(&accessors).zip(&goffs).map(|((t, a), o)| {
        quote! { #a: <#t as ::goose::Value>::goose_read(p.add(#o))? }
    });
    let writes = tys.iter().zip(&accessors).zip(&goffs).map(|((t, a), o)| {
        quote! { <#t as ::goose::Value>::goose_write(&self.#a, p.add(#o)); }
    });
    let field_defs = tys.iter().zip(&accessors).zip(&gnames).zip(&goffs).map(|(((t, a), g), o)| {
        quote! {
            ::goose::meta::FieldDef {
                name: #g.to_string(),
                ty: <#t as ::goose::Value>::goose_ty(),
                offset: ::core::mem::offset_of!(#name, #a),
                goose_offset: if <Self as ::goose::Value>::SAME_LAYOUT {
                    ::core::mem::offset_of!(#name, #a)
                } else {
                    #o
                },
                goose_size: <#t as ::goose::Value>::GOOSE_SIZE,
            }
        }
    });
    let read_body = if matches!(fields, Fields::Unnamed(_)) {
        let reads: Vec<TokenStream> = tys
            .iter()
            .zip(&goffs)
            .map(|(t, o)| quote! { <#t as ::goose::Value>::goose_read(p.add(#o))? })
            .collect();
        quote! { #name ( #(#reads),* ) }
    } else if matches!(fields, Fields::Unit) {
        quote! { #name }
    } else {
        quote! { #name { #(#reads),* } }
    };

    Ok(quote! {
        unsafe impl ::goose::Value for #name {
            const SAME_LAYOUT: bool = #same;
            const CHECKED: bool = #checked;
            const GOOSE_SIZE: usize = if #same { ::core::mem::size_of::<#name>() } else { #packed_size };
            const FIXUP: bool = #same && (#fixup_flag);

            #[inline]
            unsafe fn goose_valid(p: *const u8) -> bool {
                true #(&& #valid)*
            }

            unsafe fn goose_fixup(p: *mut u8) {
                #(#fixups)*
            }

            unsafe fn goose_read(p: *const u8) -> ::core::option::Option<Self> {
                if <Self as ::goose::Value>::SAME_LAYOUT {
                    return ::goose::read_same_layout::<Self>(p);
                }
                ::core::option::Option::Some(#read_body)
            }

            unsafe fn goose_write(&self, p: *mut u8) {
                if <Self as ::goose::Value>::SAME_LAYOUT {
                    return ::goose::write_same_layout(self, p);
                }
                #(#writes)*
            }

            fn goose_ty() -> ::goose::meta::Ty {
                ::goose::meta::Ty::Named(#name_s.to_string())
            }

            fn goose_register(types: &mut ::goose::meta::TypeSet) {
                let def = ::goose::meta::StructDef {
                    name: #name_s.to_string(),
                    doc: #doc.to_string(),
                    size: ::core::mem::size_of::<#name>(),
                    align: ::core::mem::align_of::<#name>(),
                    goose_size: <Self as ::goose::Value>::GOOSE_SIZE,
                    same_layout: <Self as ::goose::Value>::SAME_LAYOUT,
                    fields: ::std::vec![#(#field_defs),*],
                };
                if types.add_struct(def) {
                    #(<#tys as ::goose::Value>::goose_register(types);)*
                }
            }
        }
    })
}

fn derive_enum(name: &syn::Ident, doc: &str, e: &syn::DataEnum, reprs: &[String]) -> syn::Result<TokenStream> {
    let name_s = name.to_string();
    if e.variants.is_empty() {
        return Err(syn::Error::new(name.span(), "goose::Flat: an enum without variants has no values to pass"));
    }
    if e.variants.len() > 256 {
        return Err(syn::Error::new(name.span(), "goose::Flat: at most 256 variants (a one-byte Goose tag)"));
    }
    let fieldless = e.variants.iter().all(|v| matches!(v.fields, Fields::Unit));
    // Goose numbers variants from 0 in declaration order. A fieldless
    // #[repr(u8)] enum numbered the same way is one byte on both sides.
    let mut sequential = true;
    for (i, v) in e.variants.iter().enumerate() {
        if let Some((_, expr)) = &v.discriminant {
            let ok = match expr {
                syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Int(n), .. }) => n.base10_parse::<u64>().ok() == Some(i as u64),
                _ => false,
            };
            if !ok {
                sequential = false;
            }
        }
    }
    let repr_u8 = reprs.iter().any(|r| r == "u8");
    let same = fieldless && sequential && repr_u8;

    let mut read_arms = Vec::new();
    let mut write_arms = Vec::new();
    let mut variant_defs = Vec::new();
    let mut payload_sizes = Vec::new();
    let mut registers = Vec::new();
    for (i, v) in e.variants.iter().enumerate() {
        let tag = i as u8;
        let vname = &v.ident;
        let vname_s = vname.to_string();
        let fl = field_list(&v.fields);
        let mut off = quote!(1usize);
        let mut poff = quote!(0usize);
        let mut reads = Vec::new();
        let mut writes = Vec::new();
        let mut fdefs = Vec::new();
        let mut binds = Vec::new();
        for (j, (acc, gname, ty)) in fl.iter().enumerate() {
            let b = format_ident!("__f{}", j);
            match &v.fields {
                Fields::Named(_) => {
                    reads.push(quote! { #acc: <#ty as ::goose::Value>::goose_read(p.add(#off))? });
                    binds.push(quote! { #acc: #b });
                }
                _ => {
                    reads.push(quote! { <#ty as ::goose::Value>::goose_read(p.add(#off))? });
                    binds.push(quote! { #b });
                }
            }
            writes.push(quote! { <#ty as ::goose::Value>::goose_write(#b, p.add(#off)); });
            fdefs.push(quote! {
                ::goose::meta::FieldDef {
                    name: #gname.to_string(),
                    ty: <#ty as ::goose::Value>::goose_ty(),
                    offset: 0,
                    goose_offset: #poff,
                    goose_size: <#ty as ::goose::Value>::GOOSE_SIZE,
                }
            });
            registers.push(quote! { <#ty as ::goose::Value>::goose_register(types); });
            off = quote!(#off + <#ty as ::goose::Value>::GOOSE_SIZE);
            poff = quote!(#poff + <#ty as ::goose::Value>::GOOSE_SIZE);
        }
        payload_sizes.push(poff);
        let (ctor, pat) = match &v.fields {
            Fields::Named(_) => (quote! { #name::#vname { #(#reads),* } }, quote! { #name::#vname { #(#binds),* } }),
            Fields::Unnamed(_) => (quote! { #name::#vname ( #(#reads),* ) }, quote! { #name::#vname ( #(#binds),* ) }),
            Fields::Unit => (quote! { #name::#vname }, quote! { #name::#vname }),
        };
        read_arms.push(quote! { #tag => #ctor, });
        write_arms.push(quote! { #pat => { *p = #tag; #(#writes)* } });
        let disc = if fieldless { quote!(#name::#vname as i64) } else { quote!(#i as i64) };
        variant_defs.push(quote! {
            ::goose::meta::VariantDef {
                name: #vname_s.to_string(),
                fields: ::std::vec![#(#fdefs),*],
                discriminant: #disc,
            }
        });
    }
    let count = e.variants.len() as u16;
    let goose_size = quote!(1usize + ::goose::max_size(&[#(#payload_sizes),*]));
    Ok(quote! {
        unsafe impl ::goose::Value for #name {
            const SAME_LAYOUT: bool = #same;
            const CHECKED: bool = true;
            const GOOSE_SIZE: usize = #goose_size;

            #[inline]
            unsafe fn goose_valid(p: *const u8) -> bool {
                (*p as u16) < #count
            }

            #[allow(unreachable_patterns)]
            unsafe fn goose_read(p: *const u8) -> ::core::option::Option<Self> {
                ::core::option::Option::Some(match *p {
                    #(#read_arms)*
                    _ => return ::core::option::Option::None,
                })
            }

            unsafe fn goose_write(&self, p: *mut u8) {
                match self { #(#write_arms)* }
            }

            fn goose_ty() -> ::goose::meta::Ty {
                ::goose::meta::Ty::Named(#name_s.to_string())
            }

            fn goose_register(types: &mut ::goose::meta::TypeSet) {
                let def = ::goose::meta::EnumDef {
                    name: #name_s.to_string(),
                    doc: #doc.to_string(),
                    size: ::core::mem::size_of::<#name>(),
                    goose_size: <Self as ::goose::Value>::GOOSE_SIZE,
                    same_layout: <Self as ::goose::Value>::SAME_LAYOUT,
                    align: 0,
                    variants: ::std::vec![#(#variant_defs),*],
                };
                if types.add_enum(def) {
                    #(#registers)*
                }
            }
        }
    })
}

/// `#[goose(align)]` on a `#[repr(C, u8)]` enum: Goose declares it with
/// `align n` and the pads rustc's layout has, so it crosses in place. The
/// layout is RFC 2195's: a u8 tag, then a union of one `#[repr(C)]` struct
/// per variant, at the union's alignment.
fn derive_aligned_enum(name: &syn::Ident, doc: &str, e: &syn::DataEnum, reprs: &[String]) -> syn::Result<TokenStream> {
    let name_s = name.to_string();
    if !(reprs.iter().any(|r| r == "C") && reprs.iter().any(|r| r == "u8")) {
        return Err(syn::Error::new(
            name.span(),
            "goose::Flat: #[goose(align)] lays the enum out as #[repr(C, u8)]: add that repr",
        ));
    }
    if e.variants.iter().any(|v| v.discriminant.is_some()) {
        return Err(syn::Error::new(name.span(), "goose::Flat: #[goose(align)] numbers variants as Goose does: no explicit discriminants"));
    }
    if e.variants.is_empty() || e.variants.len() > 256 {
        return Err(syn::Error::new(name.span(), "goose::Flat: an enum has from 1 to 256 variants"));
    }
    let count = e.variants.len() as u16;
    // Per variant: its fields' offsets in its repr(C) struct, its alignment
    // and its size, all const.
    let mut consts = Vec::new();
    let mut variant_aligns = Vec::new();
    let mut variant_sizes = Vec::new();
    let mut same_parts = Vec::new();
    let mut valid_arms = Vec::new();
    let mut fixup_arms = Vec::new();
    let mut variant_defs = Vec::new();
    let mut registers = Vec::new();
    for (i, v) in e.variants.iter().enumerate() {
        let tag = i as u8;
        let vname = &v.ident;
        let vname_s = vname.to_string();
        let fl = field_list(&v.fields);
        let mut end = quote!(0usize);
        let mut align = quote!(1usize);
        let mut valids = Vec::new();
        let mut fixups = Vec::new();
        let mut fdefs = Vec::new();
        for (j, (_, gname, ty)) in fl.iter().enumerate() {
            let off_id = format_ident!("__V{}_F{}", i, j);
            consts.push(quote! {
                const #off_id: usize = ::goose::align_up(#end, ::core::mem::align_of::<#ty>());
            });
            let off = quote!(Self::#off_id);
            end = quote!((#off + ::core::mem::size_of::<#ty>()));
            align = quote!(::goose::max_size(&[#align, ::core::mem::align_of::<#ty>()]));
            same_parts.push(quote!(<#ty as ::goose::Value>::SAME_LAYOUT));
            valids.push(quote! {
                (!<#ty as ::goose::Value>::CHECKED || <#ty as ::goose::Value>::goose_valid(p.add(Self::__PAYLOAD + #off)))
            });
            fixups.push(quote! {
                if <#ty as ::goose::Value>::FIXUP {
                    <#ty as ::goose::Value>::goose_fixup(p.add(Self::__PAYLOAD + #off));
                }
            });
            fdefs.push(quote! {
                ::goose::meta::FieldDef {
                    name: #gname.to_string(),
                    ty: <#ty as ::goose::Value>::goose_ty(),
                    offset: #off,
                    goose_offset: #off,
                    goose_size: ::core::mem::size_of::<#ty>(),
                }
            });
            registers.push(quote! { <#ty as ::goose::Value>::goose_register(types); });
        }
        let va = format_ident!("__V{}_ALIGN", i);
        let vs = format_ident!("__V{}_SIZE", i);
        consts.push(quote! {
            const #va: usize = #align;
            const #vs: usize = ::goose::align_up(#end, Self::#va);
        });
        variant_aligns.push(quote!(Self::#va));
        variant_sizes.push(quote!(Self::#vs));
        valid_arms.push(quote! { #tag => true #(&& #valids)*, });
        fixup_arms.push(quote! { #tag => { #(#fixups)* } });
        variant_defs.push(quote! {
            ::goose::meta::VariantDef {
                name: #vname_s.to_string(),
                fields: ::std::vec![#(#fdefs),*],
                discriminant: #i as i64,
            }
        });
    }
    let same = if same_parts.is_empty() { quote!(true) } else { quote!(#(#same_parts)&&*) };
    let mod_name = format_ident!("__goose_layout_{}", name);
    Ok(quote! {
        #[doc(hidden)]
        #[allow(non_upper_case_globals, non_snake_case)]
        impl #name {
            #(#consts)*
            const __UNION_ALIGN: usize = ::goose::max_size(&[1usize, #(#variant_aligns),*]);
            const __UNION_SIZE: usize = ::goose::align_up(::goose::max_size(&[0usize, #(#variant_sizes),*]), Self::__UNION_ALIGN);
            const __PAYLOAD: usize = Self::__UNION_ALIGN;
        }

        #[allow(non_snake_case)]
        mod #mod_name {
            // The layout computed here is rustc's, or the enum cannot cross in place.
            const _: () = assert!(
                ::core::mem::size_of::<super::#name>() == super::#name::__PAYLOAD + super::#name::__UNION_SIZE,
                "goose::Flat: rustc lays this #[repr(C, u8)] enum out other than RFC 2195 says"
            );
            const _: () = assert!(
                super::#name::__UNION_ALIGN <= 8,
                "goose::Flat: #[goose(align)] enums are aligned to 8 at most (Goose's widest tag)"
            );
        }

        unsafe impl ::goose::Value for #name {
            const SAME_LAYOUT: bool = {
                assert!(#same, "goose::Flat: #[goose(align)] needs every payload field laid out as Goose lays it out");
                true
            };
            const CHECKED: bool = true;
            const GOOSE_SIZE: usize = ::core::mem::size_of::<#name>();
            const FIXUP: bool = true;

            #[allow(unreachable_patterns)]
            unsafe fn goose_valid(p: *const u8) -> bool {
                match *p {
                    #(#valid_arms)*
                    _ => false,
                }
            }

            unsafe fn goose_read(p: *const u8) -> ::core::option::Option<Self> {
                ::goose::read_same_layout::<Self>(p)
            }

            unsafe fn goose_write(&self, p: *mut u8) {
                ::goose::write_same_layout(self, p)
            }

            /// Goose's tag fills the alignment: the bytes after Rust's u8 are
            /// zero for it.
            #[allow(unreachable_patterns)]
            unsafe fn goose_fixup(p: *mut u8) {
                for i in 1..Self::__PAYLOAD {
                    *p.add(i) = 0;
                }
                match *p {
                    #(#fixup_arms)*
                    _ => {}
                }
            }

            fn goose_ty() -> ::goose::meta::Ty {
                ::goose::meta::Ty::Named(#name_s.to_string())
            }

            fn goose_register(types: &mut ::goose::meta::TypeSet) {
                let def = ::goose::meta::EnumDef {
                    name: #name_s.to_string(),
                    doc: #doc.to_string(),
                    size: ::core::mem::size_of::<#name>(),
                    goose_size: ::core::mem::size_of::<#name>(),
                    same_layout: true,
                    align: Self::__PAYLOAD,
                    variants: ::std::vec![#(#variant_defs),*],
                };
                if types.add_enum(def) {
                    #(#registers)*
                }
            }
        }
        const _: u16 = #count;
    })
}
