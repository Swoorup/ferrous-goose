//! Values that cross by copy or in place: scalars, `#[derive(goose::Flat)]`
//! structs and enums, and fixed arrays of them.

use crate::meta::{TypeSet, Ty};

/// A Rust type with a Goose counterpart of fixed size.
///
/// # Safety
///
/// `SAME_LAYOUT` promises that Goose stores the value byte for byte as Rust
/// does (the generated declaration has explicit pads where rustc put
/// padding), so a valid, aligned Goose value can be borrowed in place.
/// `goose_read` must read `GOOSE_SIZE` bytes at most and return `None` for
/// any bytes that are not a valid value. `SCALAR` may certify a single
/// primitive value, never an aggregate payload.
pub unsafe trait Value: Copy + 'static {
    /// Goose stores it exactly as Rust does.
    const SAME_LAYOUT: bool;
    /// A scalar value, whose ordinary by-value passing is allowed in strict exports.
    const SCALAR: bool = false;
    /// Some byte patterns are not values (`bool`, `char`, enum tags), so
    /// Goose bytes are checked before Rust sees them.
    const CHECKED: bool;
    /// Its size in Goose, packed.
    const GOOSE_SIZE: usize;
    /// After Rust wrote the value into Goose memory in place, some bytes
    /// need setting for Goose (an aligned enum's tag is wider than Rust's
    /// u8, and its upper bytes are Rust's padding).
    const FIXUP: bool = false;

    /// Whether the Goose bytes at `p` (any alignment) are a valid value.
    /// Only asked of `SAME_LAYOUT` types.
    ///
    /// # Safety
    /// `p` must point at `GOOSE_SIZE` readable bytes.
    unsafe fn goose_valid(p: *const u8) -> bool;

    /// The value the Goose bytes at `p` hold.
    ///
    /// # Safety
    /// `p` must point at `GOOSE_SIZE` readable bytes.
    unsafe fn goose_read(p: *const u8) -> Option<Self>;

    /// Writes the value as Goose stores it.
    ///
    /// # Safety
    /// `p` must point at `GOOSE_SIZE` writable bytes.
    unsafe fn goose_write(&self, p: *mut u8);

    /// Sets what `FIXUP` says needs setting, at `p`.
    ///
    /// # Safety
    /// `p` must point at a value of this type in Goose memory.
    unsafe fn goose_fixup(_p: *mut u8) {}

    /// The Goose type.
    fn goose_ty() -> Ty;

    /// Adds the declarations the type needs to the set, its own and its
    /// fields'.
    fn goose_register(_types: &mut TypeSet) {}
}

/// Reads a `SAME_LAYOUT` value: checked when it has to be, then copied.
///
/// # Safety
/// `p` must point at `size_of::<T>()` readable bytes.
#[inline]
pub unsafe fn read_same_layout<T: Value>(p: *const u8) -> Option<T> {
    if T::CHECKED && !T::goose_valid(p) {
        return None;
    }
    Some(core::ptr::read_unaligned(p as *const T))
}

/// Writes a `SAME_LAYOUT` value, padding included.
///
/// # Safety
/// `p` must point at `size_of::<T>()` writable bytes.
#[inline]
pub unsafe fn write_same_layout<T: Value>(v: &T, p: *mut u8) {
    core::ptr::copy_nonoverlapping(v as *const T as *const u8, p, core::mem::size_of::<T>());
    if T::FIXUP {
        T::goose_fixup(p);
    }
}

macro_rules! plain_scalar {
    ($($t:ty => $name:literal),* $(,)?) => {$(
        unsafe impl Value for $t {
            const SAME_LAYOUT: bool = true;
            const SCALAR: bool = true;
            const CHECKED: bool = false;
            const GOOSE_SIZE: usize = core::mem::size_of::<$t>();
            #[inline]
            unsafe fn goose_valid(_p: *const u8) -> bool { true }
            #[inline]
            unsafe fn goose_read(p: *const u8) -> Option<Self> {
                Some(core::ptr::read_unaligned(p as *const $t))
            }
            #[inline]
            unsafe fn goose_write(&self, p: *mut u8) {
                core::ptr::write_unaligned(p as *mut $t, *self)
            }
            fn goose_ty() -> Ty { Ty::Prim($name) }
        }
    )*};
}

plain_scalar! {
    i8 => "i8", i16 => "i16", i32 => "i32", i64 => "i64",
    u8 => "u8", u16 => "u16", u32 => "u32", u64 => "u64",
    f32 => "f32", f64 => "f64",
}

#[cfg(target_pointer_width = "64")]
plain_scalar! { usize => "u64", isize => "i64" }

unsafe impl Value for bool {
    const SAME_LAYOUT: bool = true;
    const SCALAR: bool = true;
    const CHECKED: bool = true;
    const GOOSE_SIZE: usize = 1;
    #[inline]
    unsafe fn goose_valid(p: *const u8) -> bool {
        *p <= 1
    }
    #[inline]
    unsafe fn goose_read(p: *const u8) -> Option<Self> {
        match *p {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }
    #[inline]
    unsafe fn goose_write(&self, p: *mut u8) {
        *p = *self as u8
    }
    fn goose_ty() -> Ty {
        Ty::Prim("bool")
    }
}

/// A `char` is a Goose `u32` that must be a Unicode scalar value.
unsafe impl Value for char {
    const SAME_LAYOUT: bool = true;
    const SCALAR: bool = true;
    const CHECKED: bool = true;
    const GOOSE_SIZE: usize = 4;
    #[inline]
    unsafe fn goose_valid(p: *const u8) -> bool {
        char::from_u32(core::ptr::read_unaligned(p as *const u32)).is_some()
    }
    #[inline]
    unsafe fn goose_read(p: *const u8) -> Option<Self> {
        char::from_u32(core::ptr::read_unaligned(p as *const u32))
    }
    #[inline]
    unsafe fn goose_write(&self, p: *mut u8) {
        core::ptr::write_unaligned(p as *mut u32, *self as u32)
    }
    fn goose_ty() -> Ty {
        Ty::Prim("u32")
    }
}

unsafe impl<T: Value, const N: usize> Value for [T; N] {
    const SAME_LAYOUT: bool = T::SAME_LAYOUT;
    const CHECKED: bool = T::CHECKED;
    const GOOSE_SIZE: usize = T::GOOSE_SIZE * N;
    const FIXUP: bool = T::FIXUP;

    unsafe fn goose_fixup(p: *mut u8) {
        for i in 0..N {
            T::goose_fixup(p.add(i * T::GOOSE_SIZE));
        }
    }

    unsafe fn goose_valid(p: *const u8) -> bool {
        if !T::CHECKED {
            return true;
        }
        (0..N).all(|i| T::goose_valid(p.add(i * T::GOOSE_SIZE)))
    }

    unsafe fn goose_read(p: *const u8) -> Option<Self> {
        if T::SAME_LAYOUT {
            return read_same_layout::<Self>(p);
        }
        // Every element first, so the array is built only from good ones.
        for i in 0..N {
            T::goose_read(p.add(i * T::GOOSE_SIZE))?;
        }
        Some(core::array::from_fn(|i| T::goose_read(p.add(i * T::GOOSE_SIZE)).unwrap()))
    }

    unsafe fn goose_write(&self, p: *mut u8) {
        if T::SAME_LAYOUT {
            return write_same_layout(self, p);
        }
        for (i, v) in self.iter().enumerate() {
            v.goose_write(p.add(i * T::GOOSE_SIZE));
        }
    }

    fn goose_ty() -> Ty {
        Ty::Array(Box::new(T::goose_ty()), N)
    }

    fn goose_register(types: &mut TypeSet) {
        T::goose_register(types)
    }
}

/// The size of the largest of `sizes`, for an enum's payload area.
pub const fn max_size(sizes: &[usize]) -> usize {
    let mut m = 0;
    let mut i = 0;
    while i < sizes.len() {
        if sizes[i] > m {
            m = sizes[i];
        }
        i += 1;
    }
    m
}

/// `n` rounded up to a multiple of `a` (a power of two), as repr(C) places
/// a field.
pub const fn align_up(n: usize, a: usize) -> usize {
    (n + a - 1) / a * a
}
