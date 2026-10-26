//! The C shapes Goose values cross in (spec §7.10, C.2, and `goose.h`).
//!
//! Everything here is `#[repr(C, packed)]` where Goose's C is packed, so a
//! Rust `extern "C"` function taking these types has the signature the
//! generated Goose C calls through.

use core::marker::PhantomData;

/// A Goose `T[:]` as it crosses to C: `{ data, len }`, packed.
///
/// `T` is the Rust element type the slice is read as. The data pointer is
/// untyped because Goose stores some types (enums with data) in a layout of
/// its own, which the bridge converts.
#[repr(C, packed)]
pub struct GooseSlice<T> {
    pub data: *mut u8,
    pub len: i64,
    _elem: PhantomData<*const T>,
}

impl<T> Clone for GooseSlice<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for GooseSlice<T> {}

impl<T> GooseSlice<T> {
    pub fn new(data: *mut u8, len: i64) -> Self {
        GooseSlice { data, len, _elem: PhantomData }
    }

    /// A slice over Rust memory, for calling a Goose export: Goose never
    /// keeps an argument past the call.
    pub fn from_slice(s: &[T]) -> Self {
        GooseSlice::new(s.as_ptr() as *mut u8, s.len() as i64)
    }

    pub fn from_mut_slice(s: &mut [T]) -> Self {
        GooseSlice::new(s.as_mut_ptr() as *mut u8, s.len() as i64)
    }
}

impl<'a, T> From<&'a [T]> for GooseSlice<T> {
    fn from(s: &'a [T]) -> Self {
        GooseSlice::from_slice(s)
    }
}

impl<'a, T> From<&'a mut [T]> for GooseSlice<T> {
    fn from(s: &'a mut [T]) -> Self {
        GooseSlice::from_mut_slice(s)
    }
}

impl<'a> From<&'a str> for GooseSlice<u8> {
    fn from(s: &'a str) -> Self {
        GooseSlice::from_slice(s.as_bytes())
    }
}

/// A data stack's control block (spec C.1): `{ top }`.
#[repr(C)]
pub struct GsStack {
    pub top: *mut u8,
}

/// A resizable's header in its owning frame: `{ base, len }`, packed.
#[repr(C, packed)]
pub struct GsRhdr {
    pub base: *mut u8,
    pub len: i64,
}

/// A builder, `T[>..]&`: the resizable's header and its data stack.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct GsRref {
    pub hdr: *mut GsRhdr,
    pub stk: *mut GsStack,
}

/// An 8-byte handle to a Rust object in a slot table: Goose's
/// `struct Name { slot: u32, generation: u32 }`.
#[repr(C, packed)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Handle {
    pub slot: u32,
    pub generation: u32,
}

impl core::fmt::Debug for Handle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let (slot, generation) = (self.slot, self.generation);
        write!(f, "slot {slot}, generation {generation}")
    }
}

/// The bridge's calling-convention version: what the shims expect of the
/// shapes above. A library and a generated module must agree on it.
pub const ABI_VERSION: u32 = 1;

/// The metadata format `goose_metadata` writes.
pub const METADATA_FORMAT: u32 = 2;

/// The Goose runtime whose layouts this bridge was written against (what
/// `goose version` prints). `cargo goose` compares it with the compiler's.
pub const GOOSE_RUNTIME: &str = "fba599d313fe";
