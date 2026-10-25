//! `#[goose::handle]`: a slot table for a type, a release function and a
//! live count, and the traits that make it a parameter or result of
//! exported functions. Goose holds `struct Name { slot: u32, generation: u32 }`.

use crate::doc_of;
use crate::info::{crate_info, library_info};
use crate::snake;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse::Parser, punctuated::Punctuated, spanned::Spanned, Item, Meta, Token};

pub fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let item: Item = syn::parse2(item)?;
    let (ident, generics, attrs) = match &item {
        Item::Struct(s) => (&s.ident, &s.generics, &s.attrs),
        Item::Enum(e) => (&e.ident, &e.generics, &e.attrs),
        other => return Err(syn::Error::new(other.span(), "#[goose::handle] goes on a struct or an enum")),
    };
    if !generics.params.is_empty() {
        return Err(syn::Error::new(
            generics.span(),
            "#[goose::handle]: a handle type cannot be generic (one slot table per type)",
        ));
    }
    let mut pinned = false;
    let mut goose_name = ident.to_string();
    let opts = Punctuated::<Meta, Token![,]>::parse_terminated.parse2(attr)?;
    for m in opts {
        match &m {
            Meta::Path(p) if p.is_ident("pinned") => pinned = true,
            Meta::NameValue(nv) if nv.path.is_ident("name") => {
                if let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) = &nv.value {
                    goose_name = s.value();
                } else {
                    return Err(syn::Error::new(nv.value.span(), "name = \"...\" takes a string"));
                }
            }
            other => {
                return Err(syn::Error::new(
                    other.span(),
                    "#[goose::handle] takes `pinned` (for a type that is not Send + Sync) and `name = \"...\"`",
                ))
            }
        }
    }
    let info = crate_info();
    let ns = &info.namespace;
    let qualified = format!("{ns}::{goose_name}");
    let doc = doc_of(attrs);
    let base = format!("{ns}_{}", snake(&goose_name));
    let release_sym = format!("{base}_release");
    let live_sym = format!("{base}_live_count");
    let release_fn = format_ident!("{}", release_sym);
    let live_fn = format_ident!("{}", live_sym);
    let meta_fn = format_ident!("__goose_meta_handle_{}", ident);
    let release_name = format!("{ns}::release");

    let table = if pinned {
        quote! { static TABLE: ::goose::HandleTable<#ident> = unsafe { ::goose::HandleTable::new_pinned(#qualified) }; }
    } else {
        quote! {
            const _: fn() = || {
                fn send_and_sync_or_mark_it_pinned<T: ::goose::SendSyncHandle>() {}
                send_and_sync_or_mark_it_pinned::<#ident>();
            };
            static TABLE: ::goose::HandleTable<#ident> = ::goose::HandleTable::new(#qualified);
        }
    };
    let lib = library_info();

    Ok(quote! {
        #item

        impl ::goose::HandleType for #ident {
            fn table() -> &'static ::goose::HandleTable<Self> {
                #table
                &TABLE
            }
        }

        unsafe impl ::goose::Arg for #ident {
            type Raw = ::goose::Handle;
            type ValRaw = ::goose::Handle;
            type Shared = ::goose::HandleRef<#ident>;
            type Exclusive = ::goose::HandleMut<#ident>;
            type Kept = ::goose::HandleRef<#ident>;
            const C_REF: &'static str = "handle";
            const C_VAL: &'static str = "handle";
            fn keep(s: ::goose::HandleRef<#ident>) -> ::goose::HandleRef<#ident> {
                s
            }
            unsafe fn shared(raw: ::goose::Handle, _param: &'static str) -> ::core::result::Result<Self::Shared, ::goose::BridgeError> {
                <#ident as ::goose::HandleType>::table().shared(raw)
            }
            unsafe fn exclusive(raw: ::goose::Handle, _param: &'static str) -> ::core::result::Result<Self::Exclusive, ::goose::BridgeError> {
                <#ident as ::goose::HandleType>::table().exclusive(raw)
            }
            unsafe fn take(raw: ::goose::Handle, _param: &'static str) -> ::core::result::Result<Self, ::goose::BridgeError> {
                <#ident as ::goose::HandleType>::table().take(raw)
            }
            fn span(raw: &::goose::Handle, mutable: bool, param: &'static str) -> ::goose::Span {
                ::goose::handle_span(<#ident as ::goose::HandleType>::table().space(), raw, mutable, param)
            }
            fn val_span(raw: &::goose::Handle, param: &'static str) -> ::goose::Span {
                ::goose::handle_span(<#ident as ::goose::HandleType>::table().space(), raw, true, param)
            }
            fn ref_kind(mutable: bool) -> ::goose::meta::ParamKind {
                if mutable {
                    ::goose::meta::ParamKind::HandleMut(#goose_name.to_string())
                } else {
                    ::goose::meta::ParamKind::Handle(#goose_name.to_string())
                }
            }
            fn val_kind() -> ::goose::meta::ParamKind {
                ::goose::meta::ParamKind::HandleOwned(#goose_name.to_string())
            }
            fn register(_types: &mut ::goose::meta::TypeSet) {}
        }

        unsafe impl ::goose::Out for #ident {
            type OutRaw = *mut ::goose::Handle;
            const C_OUT: &'static str = "out:handle";
            unsafe fn put(self, out: *mut ::goose::Handle) -> ::core::result::Result<(), ::goose::BridgeError> {
                let h = <#ident as ::goose::HandleType>::table().insert(self);
                ::core::ptr::write_unaligned(out, h);
                ::core::result::Result::Ok(())
            }
            fn ret_kind() -> ::goose::meta::RetKind {
                ::goose::meta::RetKind::Handle(#goose_name.to_string())
            }
            fn register(_types: &mut ::goose::meta::TypeSet) {}
        }

        /// Releases a handle: the object is dropped once no call is using it.
        #[no_mangle]
        pub extern "C" fn #release_fn(h: ::goose::Handle) {
            static SITE: ::goose::Site = ::goose::Site { name: #release_name, fallible: false, main_thread: false };
            ::goose::call(&SITE, || <#ident as ::goose::HandleType>::table().release(h), |_| ())
        }

        /// How many handles of this type are live.
        #[no_mangle]
        pub extern "C" fn #live_fn() -> i64 {
            <#ident as ::goose::HandleType>::table().live() as i64
        }

        #[doc(hidden)]
        #[allow(non_snake_case)]
        fn #meta_fn(lib: &mut ::goose::meta::Library) {
            lib.handles.insert(#goose_name.to_string(), ::goose::meta::HandleDef {
                name: #goose_name.to_string(),
                doc: #doc.to_string(),
                pinned: #pinned,
                release: #release_sym.to_string(),
                live_count: #live_sym.to_string(),
            });
        }
        ::goose::__private::inventory::submit! { ::goose::meta::Entry { add: #meta_fn } }
        #lib
    })
}
