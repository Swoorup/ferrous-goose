//! Arguments on the way in and results on the way out: the borrows, checks
//! and copies the generated shims make.
//!
//! Each argument is first *held* (validated, and copied where it has to
//! be), then lent to the Rust function for the call. The holders live in
//! the shim's frame, so nothing the function is lent outlives the call.

use crate::abi::{GooseSlice, GsRref, Handle};
use crate::meta::{ParamKind, RetKind};
use crate::shim::BridgeError;
use crate::value::Value;
use core::ops::{Deref, DerefMut};

// ---- overlap ----

/// The memory (or the handle) an argument reaches, for the aliasing check.
#[derive(Clone, Copy, Debug)]
pub struct Span {
    /// 0 for memory; for a handle, a value naming its table.
    pub space: usize,
    pub start: usize,
    pub end: usize,
    pub mutable: bool,
    pub param: &'static str,
}

impl Span {
    pub fn none(param: &'static str) -> Span {
        Span { space: 0, start: 0, end: 0, mutable: false, param }
    }
}

/// Rust's rule that a `&mut` has no alias, checked across a call's
/// arguments: any two that overlap, one of them mutable, end the program.
pub fn check_overlap(spans: &[Span]) -> Result<(), BridgeError> {
    for (i, a) in spans.iter().enumerate() {
        if a.start == a.end {
            continue;
        }
        for b in &spans[i + 1..] {
            if b.start == b.end || a.space != b.space || !(a.mutable || b.mutable) {
                continue;
            }
            if a.start < b.end && b.start < a.end {
                let what = if a.space == 0 { "memory" } else { "a handle" };
                return Err(BridgeError::Abort(format!(
                    "arguments {} and {} share {what}, and one is borrowed mutably: Rust allows no alias of a &mut",
                    a.param, b.param
                )));
            }
        }
    }
    Ok(())
}

pub fn slice_span<T: Value>(s: &GooseSlice<T>, mutable: bool, param: &'static str) -> Span {
    let (data, len) = (s.data, s.len);
    if data.is_null() || len <= 0 {
        return Span::none(param);
    }
    let start = data as usize;
    Span { space: 0, start, end: start + len as usize * T::GOOSE_SIZE, mutable, param }
}

pub fn ptr_span<T: Value>(p: *const u8, mutable: bool, param: &'static str) -> Span {
    if p.is_null() {
        return Span::none(param);
    }
    let start = p as usize;
    Span { space: 0, start, end: start + T::GOOSE_SIZE.max(1), mutable, param }
}

// ---- slices ----

fn slice_parts<T>(raw: GooseSlice<T>, param: &'static str) -> Result<(*mut u8, usize), BridgeError> {
    let (data, len) = (raw.data, raw.len);
    if len < 0 {
        return Err(BridgeError::Abort(format!("argument {param} is a slice of negative length {len}")));
    }
    if len > 0 && data.is_null() {
        return Err(BridgeError::Abort(format!("argument {param} is a null slice of length {len}")));
    }
    Ok((data, len as usize))
}

/// Whether Goose memory at `p` holding `n` values can be lent as `&[T]`:
/// same layout, aligned, and every value valid.
unsafe fn lendable<T: Value>(p: *const u8, n: usize) -> Result<bool, BridgeError> {
    if !T::SAME_LAYOUT || (p as usize) % core::mem::align_of::<T>() != 0 {
        return Ok(false);
    }
    if T::CHECKED {
        for i in 0..n {
            if !T::goose_valid(p.add(i * T::GOOSE_SIZE)) {
                return Err(invalid::<T>());
            }
        }
    }
    Ok(true)
}

fn zero_copy_error<T: Value>(param: &'static str) -> BridgeError {
    BridgeError::Fail(format!(
        "zero_copy argument {param} requires the same layout and alignment {} as Rust; copying is disabled",
        core::mem::align_of::<T>()
    ))
}

fn invalid<T: Value>() -> BridgeError {
    BridgeError::Fail(format!("an argument holds a value that is not a valid {}", crate::ty_name(&T::goose_ty())))
}

static COPIES: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// How many arguments the shims have copied rather than lent in place
/// (unaligned, or laid out unlike Rust): what the `align` directive and the
/// overhead budget are about.
pub fn copy_count() -> u64 {
    COPIES.load(core::sync::atomic::Ordering::Relaxed)
}

fn count_copy() {
    COPIES.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
}

unsafe fn copy_in<T: Value>(p: *const u8, n: usize) -> Result<Vec<T>, BridgeError> {
    count_copy();
    let mut v = Vec::with_capacity(n);
    for i in 0..n {
        v.push(T::goose_read(p.add(i * T::GOOSE_SIZE)).ok_or_else(invalid::<T>)?);
    }
    Ok(v)
}

/// A `const T[:]` held for a `&[T]`: borrowed in place when aligned and laid
/// out as Rust lays it out, else an aligned copy.
pub struct SliceRef<T: Value> {
    ptr: *const T,
    len: usize,
    owned: Option<Vec<T>>,
}

impl<T: Value> SliceRef<T> {
    /// # Safety
    /// `raw` must be a slice Goose passed to this call.
    pub unsafe fn hold(raw: GooseSlice<T>, param: &'static str) -> Result<Self, BridgeError> {
        Self::hold_impl::<true>(raw, param)
    }

    /// Borrow in place, failing instead of copying when layout or alignment differs.
    ///
    /// # Safety
    /// The pointer/slice must meet the same requirements as `hold`.
    pub unsafe fn hold_zero_copy(raw: GooseSlice<T>, param: &'static str) -> Result<Self, BridgeError> {
        Self::hold_impl::<false>(raw, param)
    }

    #[inline]
    unsafe fn hold_impl<const COPY: bool>(raw: GooseSlice<T>, param: &'static str) -> Result<Self, BridgeError> {
        if !COPY && !T::SAME_LAYOUT {
            return Err(zero_copy_error::<T>(param));
        }
        let (p, n) = slice_parts(raw, param)?;
        if n == 0 {
            return Ok(SliceRef { ptr: core::ptr::NonNull::dangling().as_ptr(), len: 0, owned: None });
        }
        if lendable::<T>(p, n)? {
            return Ok(SliceRef { ptr: p as *const T, len: n, owned: None });
        }
        if !COPY {
            return Err(zero_copy_error::<T>(param));
        }
        let v = copy_in::<T>(p, n)?;
        Ok(SliceRef { ptr: v.as_ptr(), len: n, owned: Some(v) })
    }

    pub fn get(&self) -> &[T] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }

    /// Whether the argument was copied rather than borrowed.
    pub fn copied(&self) -> bool {
        self.owned.is_some()
    }

    /// The Rust-owned values, for a parameter taking `Vec<T>`.
    pub fn into_vec(self) -> Vec<T> {
        match self.owned {
            Some(v) => v,
            None => self.get().to_vec(),
        }
    }
}

/// A `T[:]` held for a `&mut [T]`: borrowed in place when it can be, else a
/// copy written back when the call is over.
pub struct SliceMut<T: Value> {
    goose: *mut u8,
    ptr: *mut T,
    len: usize,
    owned: Option<Vec<T>>,
}

impl<T: Value> SliceMut<T> {
    /// # Safety
    /// `raw` must be a mutable slice Goose passed to this call.
    pub unsafe fn hold(raw: GooseSlice<T>, param: &'static str) -> Result<Self, BridgeError> {
        Self::hold_impl::<true>(raw, param)
    }

    /// Borrow in place, failing instead of copying when layout or alignment differs.
    ///
    /// # Safety
    /// The pointer/slice must meet the same requirements as `hold`.
    pub unsafe fn hold_zero_copy(raw: GooseSlice<T>, param: &'static str) -> Result<Self, BridgeError> {
        Self::hold_impl::<false>(raw, param)
    }

    #[inline]
    unsafe fn hold_impl<const COPY: bool>(raw: GooseSlice<T>, param: &'static str) -> Result<Self, BridgeError> {
        if !COPY && !T::SAME_LAYOUT {
            return Err(zero_copy_error::<T>(param));
        }
        let (p, n) = slice_parts(raw, param)?;
        if n == 0 {
            return Ok(SliceMut { goose: p, ptr: core::ptr::NonNull::dangling().as_ptr(), len: 0, owned: None });
        }
        if lendable::<T>(p, n)? {
            return Ok(SliceMut { goose: p, ptr: p as *mut T, len: n, owned: None });
        }
        if !COPY {
            return Err(zero_copy_error::<T>(param));
        }
        let mut v = copy_in::<T>(p, n)?;
        Ok(SliceMut { goose: p, ptr: v.as_mut_ptr(), len: n, owned: Some(v) })
    }

    pub fn get_mut(&mut self) -> &mut [T] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) }
    }

    pub fn get(&self) -> &[T] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl<T: Value> Drop for SliceMut<T> {
    fn drop(&mut self) {
        if let Some(v) = &self.owned {
            for (i, x) in v.iter().enumerate() {
                unsafe { x.goose_write(self.goose.add(i * T::GOOSE_SIZE)) };
            }
        } else if T::FIXUP {
            for i in 0..self.len {
                unsafe { T::goose_fixup(self.goose.add(i * T::GOOSE_SIZE)) };
            }
        }
    }
}

/// A `const u8[:]` held for a `&str`: checked for UTF-8.
pub struct StrRef {
    bytes: SliceRef<u8>,
}

impl StrRef {
    /// # Safety
    /// `raw` must be a slice Goose passed to this call.
    pub unsafe fn hold(raw: GooseSlice<u8>, param: &'static str) -> Result<Self, BridgeError> {
        Self::checked(SliceRef::hold(raw, param)?, param)
    }

    /// Borrow UTF-8 text in place without a copy fallback.
    ///
    /// # Safety
    /// `raw` must be a slice Goose passed to this call.
    pub unsafe fn hold_zero_copy(raw: GooseSlice<u8>, param: &'static str) -> Result<Self, BridgeError> {
        Self::checked(SliceRef::hold_zero_copy(raw, param)?, param)
    }

    fn checked(bytes: SliceRef<u8>, param: &'static str) -> Result<Self, BridgeError> {
        if let Err(e) = core::str::from_utf8(bytes.get()) {
            return Err(BridgeError::Fail(format!("argument {param} is not valid UTF-8: {e}")));
        }
        Ok(StrRef { bytes })
    }

    pub fn get(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(self.bytes.get()) }
    }

    pub fn bytes(&self) -> &[u8] {
        self.bytes.get()
    }
}

// ---- references to values ----

enum Place<T> {
    InPlace(*mut T),
    Owned(T),
}

/// A `const T&` held for a `&T`.
pub struct ValRef<T: Value> {
    place: Place<T>,
}

impl<T: Value> ValRef<T> {
    /// # Safety
    /// `p` must be a reference Goose passed to this call.
    pub unsafe fn hold(p: *const u8, param: &'static str) -> Result<Self, BridgeError> {
        Self::hold_impl::<true>(p, param)
    }

    /// Borrow in place, failing instead of copying when layout or alignment differs.
    ///
    /// # Safety
    /// The pointer/slice must meet the same requirements as `hold`.
    pub unsafe fn hold_zero_copy(p: *const u8, param: &'static str) -> Result<Self, BridgeError> {
        Self::hold_impl::<false>(p, param)
    }

    #[inline]
    unsafe fn hold_impl<const COPY: bool>(p: *const u8, param: &'static str) -> Result<Self, BridgeError> {
        if !COPY && !T::SAME_LAYOUT {
            return Err(zero_copy_error::<T>(param));
        }
        if p.is_null() {
            return Err(BridgeError::Abort(format!("argument {param} is a null reference")));
        }
        if lendable::<T>(p, 1)? {
            return Ok(ValRef { place: Place::InPlace(p as *mut T) });
        }
        if !COPY {
            return Err(zero_copy_error::<T>(param));
        }
        count_copy();
        Ok(ValRef { place: Place::Owned(T::goose_read(p).ok_or_else(invalid::<T>)?) })
    }
}

impl<T: Value> Deref for ValRef<T> {
    type Target = T;
    fn deref(&self) -> &T {
        match &self.place {
            Place::InPlace(p) => unsafe { &**p },
            Place::Owned(v) => v,
        }
    }
}

/// A `T&` held for a `&mut T`: written back after the call when copied.
pub struct ValMut<T: Value> {
    goose: *mut u8,
    place: Place<T>,
}

impl<T: Value> ValMut<T> {
    /// # Safety
    /// `p` must be a mutable reference Goose passed to this call.
    pub unsafe fn hold(p: *mut u8, param: &'static str) -> Result<Self, BridgeError> {
        Self::hold_impl::<true>(p, param)
    }

    /// Borrow in place, failing instead of copying when layout or alignment differs.
    ///
    /// # Safety
    /// The pointer/slice must meet the same requirements as `hold`.
    pub unsafe fn hold_zero_copy(p: *mut u8, param: &'static str) -> Result<Self, BridgeError> {
        Self::hold_impl::<false>(p, param)
    }

    #[inline]
    unsafe fn hold_impl<const COPY: bool>(p: *mut u8, param: &'static str) -> Result<Self, BridgeError> {
        if !COPY && !T::SAME_LAYOUT {
            return Err(zero_copy_error::<T>(param));
        }
        if p.is_null() {
            return Err(BridgeError::Abort(format!("argument {param} is a null reference")));
        }
        if lendable::<T>(p, 1)? {
            return Ok(ValMut { goose: p, place: Place::InPlace(p as *mut T) });
        }
        if !COPY {
            return Err(zero_copy_error::<T>(param));
        }
        count_copy();
        Ok(ValMut { goose: p, place: Place::Owned(T::goose_read(p).ok_or_else(invalid::<T>)?) })
    }
}

impl<T: Value> Deref for ValMut<T> {
    type Target = T;
    fn deref(&self) -> &T {
        match &self.place {
            Place::InPlace(p) => unsafe { &**p },
            Place::Owned(v) => v,
        }
    }
}

impl<T: Value> DerefMut for ValMut<T> {
    fn deref_mut(&mut self) -> &mut T {
        match &mut self.place {
            Place::InPlace(p) => unsafe { &mut **p },
            Place::Owned(v) => v,
        }
    }
}

impl<T: Value> Drop for ValMut<T> {
    fn drop(&mut self) {
        match &self.place {
            Place::Owned(v) => unsafe { v.goose_write(self.goose) },
            Place::InPlace(_) if T::FIXUP => unsafe { T::goose_fixup(self.goose) },
            Place::InPlace(_) => {}
        }
    }
}

// ---- named types as parameters and results ----

/// A type that may be a parameter by reference or by value: values (scalars,
/// `#[derive(goose::Flat)]` types) and handles. Implemented by the macros.
///
/// # Safety
/// `Raw` and `ValRaw` must be the C shapes the generated Goose passes for
/// `&T`/`&mut T` and `T`, and the holders must lend only call-scoped access.
/// The zero-copy constants certify that the corresponding strict methods
/// never duplicate a bulk payload, including on a misaligned address.
pub unsafe trait Arg: Sized + 'static {
    type Raw: Copy;
    type ValRaw: Copy;
    type Shared: Deref<Target = Self>;
    type Exclusive: DerefMut<Target = Self>;
    /// What a shared argument becomes when it must outlive the call (a
    /// function taking a callback runs between Goose calls): a copy of a
    /// value, a handle's guard.
    type Kept: Deref<Target = Self> + 'static;
    /// The C shape of `&T` and of `T`, for the record.
    const C_REF: &'static str;
    const C_VAL: &'static str;
    /// Strict references borrow the original storage without payload copies.
    const ZERO_COPY: bool = false;
    /// By-value scalars and owned handles can cross without a bulk payload copy.
    const ZERO_COPY_OWNED: bool = false;

    /// # Safety
    /// As for `shared`; implementations must never allocate a fallback copy.
    unsafe fn shared_zero_copy(_raw: Self::Raw, param: &'static str) -> Result<Self::Shared, BridgeError> {
        Err(BridgeError::Fail(format!("zero_copy argument {param} does not support borrowing in place")))
    }
    /// # Safety
    /// As for `exclusive`; implementations must never allocate a fallback copy.
    unsafe fn exclusive_zero_copy(_raw: Self::Raw, param: &'static str) -> Result<Self::Exclusive, BridgeError> {
        Err(BridgeError::Fail(format!("zero_copy argument {param} does not support borrowing in place")))
    }
    /// # Safety
    /// As for `take`; implementations must never duplicate the bulk payload.
    unsafe fn take_zero_copy(_raw: Self::ValRaw, param: &'static str) -> Result<Self, BridgeError> {
        Err(BridgeError::Fail(format!("zero_copy argument {param} cannot be taken without copying")))
    }

    fn keep(s: Self::Shared) -> Self::Kept;

    /// # Safety
    /// `raw` must be what Goose passed for this parameter in this call.
    unsafe fn shared(raw: Self::Raw, param: &'static str) -> Result<Self::Shared, BridgeError>;
    /// # Safety
    /// As for `shared`.
    unsafe fn exclusive(raw: Self::Raw, param: &'static str) -> Result<Self::Exclusive, BridgeError>;
    /// # Safety
    /// As for `shared`.
    unsafe fn take(raw: Self::ValRaw, param: &'static str) -> Result<Self, BridgeError>;
    fn span(raw: &Self::Raw, mutable: bool, param: &'static str) -> Span;
    fn val_span(raw: &Self::ValRaw, param: &'static str) -> Span;
    fn ref_kind(mutable: bool) -> ParamKind;
    fn val_kind() -> ParamKind;
    fn register(types: &mut crate::meta::TypeSet);
}

/// A type that may be a result through an out-pointer: values and handles.
///
/// # Safety
/// `OutRaw` must be the out-pointer the generated Goose passes. A true
/// `ZERO_COPY` certifies that `put` does not duplicate a bulk payload.
pub unsafe trait Out: Sized + 'static {
    type OutRaw: Copy;
    const C_OUT: &'static str;
    /// `put` writes a scalar or publishes a handle, without duplicating a bulk payload.
    const ZERO_COPY: bool = false;
    /// # Safety
    /// `out` must be the out-pointer Goose passed to this call.
    unsafe fn put(self, out: Self::OutRaw) -> Result<(), BridgeError>;
    fn ret_kind() -> RetKind;
    fn register(types: &mut crate::meta::TypeSet);
}

/// `Arg` and `Out` for a value type, written by `#[derive(goose::Flat)]` and
/// for the scalars here.
#[macro_export]
#[doc(hidden)]
macro_rules! __value_arg {
    ($t:ty) => {
        unsafe impl $crate::Arg for $t {
            type Raw = *mut u8;
            type ValRaw = *const u8;
            type Shared = $crate::ValRef<$t>;
            type Exclusive = $crate::ValMut<$t>;
            type Kept = $crate::Kept<$t>;
            const C_REF: &'static str = "ptr";
            const C_VAL: &'static str = "ptr";
            const ZERO_COPY: bool = <$t as $crate::Value>::SAME_LAYOUT;
            const ZERO_COPY_OWNED: bool = <$t as $crate::Value>::SCALAR;
            unsafe fn shared_zero_copy(raw: *mut u8, param: &'static str) -> Result<Self::Shared, $crate::BridgeError> {
                $crate::ValRef::hold_zero_copy(raw, param)
            }
            unsafe fn exclusive_zero_copy(raw: *mut u8, param: &'static str) -> Result<Self::Exclusive, $crate::BridgeError> {
                $crate::ValMut::hold_zero_copy(raw, param)
            }
            unsafe fn take_zero_copy(raw: *const u8, param: &'static str) -> Result<Self, $crate::BridgeError> {
                if !Self::ZERO_COPY_OWNED {
                    return Err($crate::BridgeError::Fail(::std::format!("zero_copy argument {param} must be borrowed")));
                }
                Ok(*$crate::ValRef::<$t>::hold_zero_copy(raw, param)?)
            }
            fn keep(s: $crate::ValRef<$t>) -> $crate::Kept<$t> {
                $crate::Kept(*s)
            }
            unsafe fn shared(raw: *mut u8, param: &'static str) -> Result<Self::Shared, $crate::BridgeError> {
                $crate::ValRef::hold(raw, param)
            }
            unsafe fn exclusive(raw: *mut u8, param: &'static str) -> Result<Self::Exclusive, $crate::BridgeError> {
                $crate::ValMut::hold(raw, param)
            }
            unsafe fn take(raw: *const u8, param: &'static str) -> Result<Self, $crate::BridgeError> {
                Ok(*$crate::ValRef::<$t>::hold(raw, param)?)
            }
            fn span(raw: &*mut u8, mutable: bool, param: &'static str) -> $crate::Span {
                $crate::ptr_span::<$t>(*raw, mutable, param)
            }
            fn val_span(_raw: &*const u8, param: &'static str) -> $crate::Span {
                // Copied before the call: no alias of anything.
                $crate::Span::none(param)
            }
            fn ref_kind(mutable: bool) -> $crate::meta::ParamKind {
                let t = <$t as $crate::Value>::goose_ty();
                if mutable { $crate::meta::ParamKind::RefMut(t) } else { $crate::meta::ParamKind::Ref(t) }
            }
            fn val_kind() -> $crate::meta::ParamKind {
                $crate::meta::ParamKind::Value(<$t as $crate::Value>::goose_ty())
            }
            fn register(types: &mut $crate::meta::TypeSet) {
                <$t as $crate::Value>::goose_register(types)
            }
        }
        unsafe impl $crate::Out for $t {
            type OutRaw = *mut u8;
            const C_OUT: &'static str = "out:ptr";
            const ZERO_COPY: bool = <$t as $crate::Value>::SCALAR;
            unsafe fn put(self, out: *mut u8) -> Result<(), $crate::BridgeError> {
                $crate::Value::goose_write(&self, out);
                Ok(())
            }
            fn ret_kind() -> $crate::meta::RetKind {
                $crate::meta::RetKind::Value(<$t as $crate::Value>::goose_ty())
            }
            fn register(types: &mut $crate::meta::TypeSet) {
                <$t as $crate::Value>::goose_register(types)
            }
        }
    };
}

__value_arg!(i8);
__value_arg!(i16);
__value_arg!(i32);
__value_arg!(i64);
__value_arg!(u8);
__value_arg!(u16);
__value_arg!(u32);
__value_arg!(u64);
__value_arg!(f32);
__value_arg!(f64);
__value_arg!(bool);
__value_arg!(char);
#[cfg(target_pointer_width = "64")]
__value_arg!(usize);
#[cfg(target_pointer_width = "64")]
__value_arg!(isize);

unsafe impl<T: Value, const N: usize> Arg for [T; N] {
    type Raw = *mut u8;
    type ValRaw = *const u8;
    type Shared = ValRef<[T; N]>;
    type Exclusive = ValMut<[T; N]>;
    type Kept = Kept<[T; N]>;
    const C_REF: &'static str = "ptr";
    const C_VAL: &'static str = "ptr";
    const ZERO_COPY: bool = <Self as Value>::SAME_LAYOUT;
    unsafe fn shared_zero_copy(raw: *mut u8, param: &'static str) -> Result<Self::Shared, BridgeError> {
        ValRef::hold_zero_copy(raw, param)
    }
    unsafe fn exclusive_zero_copy(raw: *mut u8, param: &'static str) -> Result<Self::Exclusive, BridgeError> {
        ValMut::hold_zero_copy(raw, param)
    }
    fn keep(s: ValRef<[T; N]>) -> Kept<[T; N]> {
        Kept(*s)
    }
    unsafe fn shared(raw: *mut u8, param: &'static str) -> Result<Self::Shared, BridgeError> {
        ValRef::hold(raw, param)
    }
    unsafe fn exclusive(raw: *mut u8, param: &'static str) -> Result<Self::Exclusive, BridgeError> {
        ValMut::hold(raw, param)
    }
    unsafe fn take(raw: *const u8, param: &'static str) -> Result<Self, BridgeError> {
        Ok(*ValRef::<[T; N]>::hold(raw, param)?)
    }
    fn span(raw: &*mut u8, mutable: bool, param: &'static str) -> Span {
        ptr_span::<[T; N]>(*raw, mutable, param)
    }
    fn val_span(_raw: &*const u8, param: &'static str) -> Span {
        Span::none(param)
    }
    fn ref_kind(mutable: bool) -> ParamKind {
        let t = <[T; N] as Value>::goose_ty();
        if mutable {
            ParamKind::RefMut(t)
        } else {
            ParamKind::Ref(t)
        }
    }
    fn val_kind() -> ParamKind {
        ParamKind::Value(<[T; N] as Value>::goose_ty())
    }
    fn register(types: &mut crate::meta::TypeSet) {
        <[T; N] as Value>::goose_register(types)
    }
}

// ---- results ----

/// Writes a scalar or value result through an out-pointer.
///
/// # Safety
/// `out` must be an out-pointer Goose passed to this call.
pub unsafe fn put<T: Out>(v: T, out: T::OutRaw) -> Result<(), BridgeError> {
    v.put(out)
}

/// Writes an `i64` through an out-pointer that may be unaligned.
///
/// # Safety
/// `out` must be an out-pointer Goose passed to this call.
pub unsafe fn put_i64(out: *mut i64, v: i64) {
    core::ptr::write_unaligned(out, v)
}

/// Where `part` lies in `whole`, in elements: the offset and length a
/// borrowed result leaves as, for the Goose wrapper to re-slice the
/// argument with.
pub fn offset_in<T>(whole: &[T], part: &[T], param: &'static str) -> Result<(i64, i64), BridgeError> {
    let size = core::mem::size_of::<T>().max(1);
    let w = whole.as_ptr() as usize;
    let p = part.as_ptr() as usize;
    let inside = part.is_empty() && p >= w && p <= w + whole.len() * size
        || (p >= w && p + part.len() * size <= w + whole.len() * size);
    if !inside || (p - w) % size != 0 {
        return Err(BridgeError::Abort(format!(
            "a result declared as borrowed from {param} does not lie inside it"
        )));
    }
    Ok((((p - w) / size) as i64, part.len() as i64))
}

/// A builder argument, held for the `&mut GooseBuf` the function is lent.
pub struct BufArg(pub GsRref);

impl BufArg {
    pub fn get(&mut self) -> crate::GooseBuf<'_> {
        unsafe { crate::GooseBuf::from_raw(self.0) }
    }
}

/// A typed builder argument, for `&mut GooseVec<T>`.
pub struct VecArg<T: Value>(pub GsRref, pub core::marker::PhantomData<T>);

impl<T: Value> VecArg<T> {
    pub fn new(raw: GsRref) -> Self {
        VecArg(raw, core::marker::PhantomData)
    }
    pub fn get(&mut self) -> crate::GooseVec<'_, T> {
        unsafe { crate::GooseVec::from_raw(self.0) }
    }
}

/// Builds a handle span: the table names the space, the slot the place.
pub fn handle_span(table: usize, h: &Handle, mutable: bool, param: &'static str) -> Span {
    let slot = h.slot as usize;
    Span { space: table, start: slot, end: slot + 1, mutable, param }
}

unsafe impl<T: Value, const N: usize> Out for [T; N] {
    type OutRaw = *mut u8;
    const C_OUT: &'static str = "out:ptr";
    unsafe fn put(self, out: *mut u8) -> Result<(), BridgeError> {
        self.goose_write(out);
        Ok(())
    }
    fn ret_kind() -> RetKind {
        RetKind::Value(<[T; N] as Value>::goose_ty())
    }
    fn register(types: &mut crate::meta::TypeSet) {
        <[T; N] as Value>::goose_register(types)
    }
}

/// A value kept past a call: a copy.
pub struct Kept<T>(pub T);

impl<T> Deref for Kept<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

/// Where `part` lies in the slice at `base` of `len` elements: the offset
/// and length a borrowed result leaves as.
pub fn offset_in_raw<T>(base: *const T, len: usize, part: &[T], param: &'static str) -> Result<(i64, i64), BridgeError> {
    let whole = unsafe { core::slice::from_raw_parts(base, len) };
    offset_in(whole, part, param)
}

/// A part of an argument, as a span list carries it: Goose's
/// `struct Span { at: i64, len: i64 }`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GooseSpan {
    pub at: i64,
    pub len: i64,
}

unsafe impl Value for GooseSpan {
    const SAME_LAYOUT: bool = true;
    const CHECKED: bool = false;
    const GOOSE_SIZE: usize = 16;
    unsafe fn goose_valid(_p: *const u8) -> bool {
        true
    }
    unsafe fn goose_read(p: *const u8) -> Option<Self> {
        Some(core::ptr::read_unaligned(p as *const GooseSpan))
    }
    unsafe fn goose_write(&self, p: *mut u8) {
        core::ptr::write_unaligned(p as *mut GooseSpan, *self)
    }
    fn goose_ty() -> crate::meta::Ty {
        crate::meta::Ty::Named("Span".to_string())
    }
}

/// The bytes of a text or byte result.
pub trait AsBytes {
    fn goose_bytes(&self) -> &[u8];
}

impl AsBytes for String {
    fn goose_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
}
impl AsBytes for str {
    fn goose_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
}
impl AsBytes for [u8] {
    fn goose_bytes(&self) -> &[u8] {
        self
    }
}
impl AsBytes for Vec<u8> {
    fn goose_bytes(&self) -> &[u8] {
        self
    }
}
impl AsBytes for Box<str> {
    fn goose_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
}
impl AsBytes for Box<[u8]> {
    fn goose_bytes(&self) -> &[u8] {
        self
    }
}
impl AsBytes for std::borrow::Cow<'_, str> {
    fn goose_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
}
impl AsBytes for std::borrow::Cow<'_, [u8]> {
    fn goose_bytes(&self) -> &[u8] {
        self
    }
}
impl<T: AsBytes + ?Sized> AsBytes for &T {
    fn goose_bytes(&self) -> &[u8] {
        (**self).goose_bytes()
    }
}

// ---- results counted, then copied ----

thread_local! {
    static STASH: std::cell::RefCell<std::collections::HashMap<&'static str, Box<dyn std::any::Any>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Keeps a result for the `__take` call that follows (a compiler without
/// typed builders sizes the Goose array first); its length.
pub fn stash_put<T: Value>(key: &'static str, v: Vec<T>) -> i64 {
    let n = v.len() as i64;
    STASH.with(|s| s.borrow_mut().insert(key, Box::new(v)));
    n
}

/// Copies the kept result into the slice Goose sized for it.
///
/// # Safety
/// `out` must be a mutable slice Goose passed to this call.
pub unsafe fn stash_take<T: Value>(key: &'static str, out: GooseSlice<T>) -> Result<(), BridgeError> {
    let v = STASH
        .with(|s| s.borrow_mut().remove(key))
        .and_then(|b| b.downcast::<Vec<T>>().ok())
        .ok_or_else(|| BridgeError::Abort(format!("{key}: nothing to take; the count call comes first")))?;
    let (data, len) = (out.data, out.len);
    if len as usize != v.len() {
        return Err(BridgeError::Abort(format!(
            "{key}: the array has {len} slots for {} values",
            v.len()
        )));
    }
    for (i, x) in v.iter().enumerate() {
        x.goose_write(data.add(i * T::GOOSE_SIZE));
    }
    Ok(())
}
