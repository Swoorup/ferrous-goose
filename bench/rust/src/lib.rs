//! Pairs of functions doing the same work: a bare `extern "C"` function, as
//! hand-written C or Rust would offer Goose, and a `#[goose::export]` one,
//! whose shim adds the boundary checks. The Goose program in bench/goose
//! times each and reports what the checks cost per call.

use goose::{GooseBuf, GooseSlice};

// ---- bare: no bridge ----

#[no_mangle]
pub extern "C" fn bare_add(a: i64, b: i64) -> i64 {
    a.wrapping_add(b)
}

/// # Safety
/// `xs` must be a valid, aligned slice.
#[no_mangle]
pub unsafe extern "C" fn bare_sum(xs: GooseSlice<i64>) -> i64 {
    let (p, n) = (xs.data, xs.len);
    if n <= 0 {
        return 0;
    }
    std::slice::from_raw_parts(p as *const i64, n as usize).iter().fold(0i64, |a, b| a.wrapping_add(*b))
}

/// # Safety
/// `b` must be a valid slice.
#[no_mangle]
pub unsafe extern "C" fn bare_len_bytes(b: GooseSlice<u8>) -> i64 {
    b.len
}

/// # Safety
/// The slices must be valid, aligned and disjoint.
#[no_mangle]
pub unsafe extern "C" fn bare_axpy(y: GooseSlice<f64>, x: GooseSlice<f64>, a: f64) {
    let (yp, yn) = (y.data, y.len);
    let (xp, xn) = (x.data, x.len);
    let n = yn.min(xn).max(0) as usize;
    let y = std::slice::from_raw_parts_mut(yp as *mut f64, n);
    let x = std::slice::from_raw_parts(xp as *const f64, n);
    for i in 0..n {
        y[i] += a * x[i];
    }
}

// ---- through the bridge ----

/// A scalar call: the panic catcher alone.
#[goose::export]
pub fn add(a: i64, b: i64) -> i64 {
    a.wrapping_add(b)
}

/// A slice: the alignment check, then the same loop.
#[goose::export]
pub fn sum(xs: &[i64]) -> i64 {
    xs.iter().fold(0i64, |a, b| a.wrapping_add(*b))
}

/// Bytes: no check beyond the slice's.
#[goose::export]
pub fn len_bytes(b: &[u8]) -> i64 {
    b.len() as i64
}

/// Text: UTF-8 validation, which grows with the length.
#[goose::export]
pub fn len_str(s: &str) -> i64 {
    s.len() as i64
}

/// A &mut and a & slice: the overlap check besides the alignment ones.
#[goose::export]
pub fn axpy(y: &mut [f64], x: &[f64], a: f64) {
    for (yi, xi) in y.iter_mut().zip(x) {
        *yi += a * xi;
    }
}

/// A Rust object behind a handle.
#[goose::handle]
pub struct Counter(i64);

#[goose::export]
pub fn counter() -> Counter {
    Counter(0)
}

/// A handle, exclusively: the generation check and the lock.
#[goose::export]
pub fn bump(c: &mut Counter, v: i64) -> i64 {
    c.0 = c.0.wrapping_add(v);
    c.0
}

/// A handle, shared.
#[goose::export]
pub fn peek(c: &Counter) -> i64 {
    c.0
}

/// A builder: appending.
#[goose::export]
pub fn fill(n: i64, out: &mut GooseBuf) {
    for _ in 0..n {
        out.push(b'x');
    }
}

/// A Result: the error builder in the signature, nothing to report.
#[goose::export]
pub fn checked(a: i64, b: i64) -> Result<i64, String> {
    a.checked_add(b).ok_or_else(|| "overflow".to_string())
}
