//! The attributes of Ferrous Goose. Use them through the `goose` crate:
//! `#[goose::export]`, `#[goose::handle]`, `#[derive(goose::Flat)]`.
//!
//! The macros never write files. Each one emits the shim Goose calls and a
//! record of the item, which `cargo goose bindings` reads back out of the
//! built library (layouts included, which only rustc knows).

use proc_macro::TokenStream;

mod callbacks;
mod export;
mod flat;
mod handle;
mod info;
mod types;

/// Marks a function Goose may call.
///
/// Options: `instantiate(T = f32, f64)` for a generic function (one Goose
/// overload per type), `throws` (the Goose wrapper of a `Result` function
/// throws the error text through Goose's `exception` module),
/// `main_thread` (calls from any other thread abort), `name = "..."` (the
/// Goose name), `symbol = "..."` (the C symbol).
#[proc_macro_attribute]
pub fn export(attr: TokenStream, item: TokenStream) -> TokenStream {
    export::expand(attr.into(), item.into()).unwrap_or_else(|e| e.to_compile_error()).into()
}

/// Makes a type a handle: an object in a slot table, which Goose names by
/// slot and generation. `#[goose::handle(pinned)]` for a type that is not
/// Send + Sync: each object then stays on the thread that made it.
#[proc_macro_attribute]
pub fn handle(attr: TokenStream, item: TokenStream) -> TokenStream {
    handle::expand(attr.into(), item.into()).unwrap_or_else(|e| e.to_compile_error()).into()
}

/// A Goose declaration for a `#[repr(C)]` struct or an enum, with the
/// layout rustc gave it.
#[proc_macro_derive(Flat, attributes(goose))]
pub fn flat(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    flat::derive(input).unwrap_or_else(|e| e.to_compile_error()).into()
}

/// Goose export fns Rust calls back while Goose waits on a call
/// (goose-patches/0008): `goose::callbacks! { fn on_event(x: i64) -> bool; }`.
#[proc_macro]
pub fn callbacks(input: TokenStream) -> TokenStream {
    callbacks::expand(input.into()).unwrap_or_else(|e| e.to_compile_error()).into()
}

/// The doc comment on an item, joined into lines.
pub(crate) fn doc_of(attrs: &[syn::Attribute]) -> String {
    let mut lines = Vec::new();
    for a in attrs {
        if !a.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &a.meta {
            if let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) = &nv.value {
                let v = s.value();
                lines.push(v.strip_prefix(' ').unwrap_or(&v).to_string());
            }
        }
    }
    lines.join("\n")
}

/// `MatchSet` -> `match_set`.
pub(crate) fn snake(s: &str) -> String {
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

/// `find_all` -> `FindAll`.
pub(crate) fn camel(s: &str) -> String {
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
