//! Ferrous Goose: Goose programs calling Rust crates, and Rust programs
//! calling Goose, with no C, no headers and no unsafe in user code.
//!
//! A crate marks what Goose may call:
//!
//! ```ignore
//! #[goose::handle]
//! pub struct Regex(regex::Regex);
//!
//! #[goose::export]
//! pub fn compile(pattern: &str) -> Result<Regex, regex::Error> {
//!     regex::Regex::new(pattern).map(Regex)
//! }
//! ```
//!
//! and `cargo goose bindings` writes the Goose module a program imports.
//! This crate is what the generated shims use: the C shapes of Goose values
//! ([`abi`]), the argument holders that check alignment, UTF-8 and aliasing
//! ([`args`]), builders ([`GooseBuf`]), handle tables ([`HandleTable`]), the
//! panic catcher ([`call`]) and the records `cargo goose` reads ([`meta`]).
//! For the other direction, [`Runtime`] and [`include!`] wrap what
//! `goose_build` generates.

pub mod abi;
mod args;
mod buf;
mod embed;
mod handle;
pub mod meta;
pub mod reentry;
mod shim;
mod value;

pub use abi::{GooseSlice, GsRref, Handle};
pub use args::*;
pub use buf::{GooseBuf, GooseVec};
pub use embed::{GooseInit, InitError, Runtime};
pub use reentry::{Reentry, ReentryError, ReentrySlot};
pub use handle::{
    leak_report, live_count, print_leak_report, HandleMut, HandleRef, HandleTable, HandleType, LiveTable, SendSyncHandle,
};
pub use shim::{abort_at, call, on_main_thread, BridgeError, RuntimeHooks, Site};
pub use value::{align_up, max_size, read_same_layout, write_same_layout, Value};

/// Marks a function Goose may call: `#[goose::export]`,
/// `#[goose::export(instantiate(T = f32, f64))]`, `#[goose::export(throws)]`,
/// `#[goose::export(main_thread)]`, `#[goose::export(zero_copy)]`,
/// `#[goose::export(name = "...")]`. Strict exports retain safety checks and
/// reject a misaligned argument instead of copying it.
pub use ferrous_goose_macros::export;
/// Makes a type a handle Goose holds: `#[goose::handle]`, or
/// `#[goose::handle(pinned)]` for a type that is not Send + Sync.
pub use ferrous_goose_macros::handle;
/// Gives a `#[repr(C)]` struct or an enum a Goose declaration with rustc's
/// layout: `#[derive(goose::Flat)]`.
pub use ferrous_goose_macros::Flat;
/// Declares Goose export fns that Rust calls back while Goose waits on a
/// call (goose-patches/0008): `goose::callbacks! { fn on_event(x: i64) -> bool; }`
/// makes `on_event(&Reentry, i64) -> Result<bool, ReentryError>`.
pub use ferrous_goose_macros::callbacks;

#[doc(hidden)]
pub mod __private {
    pub use corosensei::{Coroutine, CoroutineResult, Yielder};
    pub use inventory;

    /// Which crate a library is, for `goose_metadata`.
    pub struct LibraryInfo {
        pub crate_name: &'static str,
        pub version: &'static str,
        pub namespace: &'static str,
        pub library: &'static str,
    }

    inventory::collect!(LibraryInfo);

    pub fn info() -> Option<&'static LibraryInfo> {
        inventory::iter::<LibraryInfo>.into_iter().next()
    }
}

/// A Goose type as written in Goose.
pub fn ty_name(t: &meta::Ty) -> String {
    match t {
        meta::Ty::Prim(p) => p.to_string(),
        meta::Ty::Named(n) => n.clone(),
        meta::Ty::Array(e, n) => format!("{}[{n}]", ty_name(e)),
    }
}

/// The library's records as JSON (`cargo goose bindings` reads them).
pub fn metadata_json() -> String {
    let (c, v, n) = match __private::info() {
        Some(i) => (i.crate_name, i.version, i.namespace),
        None => ("", "", ""),
    };
    meta::to_json(&meta::library(), c, v, n)
}

/// The records, as `cargo goose bindings` asks for them: a pointer to the
/// JSON, its length through `len`. The bytes live as long as the library.
///
/// # Safety
/// `len` must be writable.
#[no_mangle]
pub unsafe extern "C" fn goose_metadata(len: *mut usize) -> *const u8 {
    static JSON: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let s = JSON.get_or_init(|| {
        std::panic::catch_unwind(metadata_json).unwrap_or_else(|_| "{\"format\":0}".to_string())
    });
    *len = s.len();
    s.as_ptr()
}

/// The bridge's calling-convention version, in the name: a generated module
/// calls `goose_abi_1` when the program starts, so a library built with
/// another version fails there, by name, rather than misbehave.
#[no_mangle]
pub extern "C" fn goose_abi_1() -> u32 {
    abi::ABI_VERSION
}

/// Arguments copied rather than lent in place so far (`ns::copy_count()`).
#[no_mangle]
pub extern "C" fn goose_copy_count() -> i64 {
    args::copy_count() as i64
}

/// Live handles of every type the library made (`ns::live_count()`).
#[no_mangle]
pub extern "C" fn goose_live_count() -> i64 {
    live_count() as i64
}

/// Appends one line per handle type with live handles to `out`; the number
/// of live handles (`ns::leak_report()`).
///
/// # Safety
/// `out` must be a builder Goose passed to this call.
#[no_mangle]
pub unsafe extern "C" fn goose_leak_report(out: GsRref) -> i64 {
    let mut b = GooseBuf::from_raw(out);
    let name = __private::info().map(|i| i.namespace).unwrap_or("rust");
    for l in leak_report() {
        b.push_str(name);
        b.push_str(": ");
        b.push_str(&l);
        b.push(b'\n');
    }
    live_count() as i64
}

/// The leak report a debug build prints when the program ends, where the
/// runtime calls back at its end (goose-patches/0004).
#[no_mangle]
pub extern "C" fn goose_at_end() {
    if cfg!(debug_assertions) || std::env::var_os("GOOSE_RUST_LEAKS").is_some() {
        let name = __private::info().map(|i| i.namespace).unwrap_or("rust");
        print_leak_report(name);
    }
}
