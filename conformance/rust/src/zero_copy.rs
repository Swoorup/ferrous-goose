//! Strict exports: the body sees the caller's storage or is never entered.

use crate::{AShape, Hit, Stack};
use goose::{GooseBuf, GooseVec};

#[goose::export(zero_copy)]
pub fn zc_sum(xs: &[i64]) -> Result<i64, String> { Ok(xs.iter().sum()) }

#[goose::export(zero_copy)]
pub fn zc_scale(xs: &mut [i64], by: i64) -> Result<i64, String> {
    for x in xs.iter_mut() { *x *= by; }
    Ok(xs.iter().sum())
}

#[goose::export(zero_copy)]
pub fn zc_axpy(y: &mut [i64], x: &[i64]) -> Result<i64, String> {
    for (y, x) in y.iter_mut().zip(x) { *y += x; }
    Ok(y.iter().sum())
}

#[goose::export(zero_copy)]
pub fn zc_bump(x: &mut i64) -> Option<i64> { *x += 1; Some(*x) }

#[goose::export(zero_copy)]
pub fn zc_fixed(xs: &mut [i64; 3]) -> i64 { xs[0] += 1; xs.iter().sum() }

#[goose::export(zero_copy)]
pub fn zc_text(s: &str) -> Result<i64, String> { Ok(s.len() as i64) }

#[goose::export(zero_copy)]
pub fn zc_bools(xs: &[bool]) -> Result<i64, String> { Ok(xs.iter().filter(|x| **x).count() as i64) }

#[goose::export(zero_copy)]
pub fn zc_chars(xs: &[char]) -> Result<i64, String> { Ok(xs.len() as i64) }

#[goose::export(zero_copy)]
pub fn zc_trim(s: &str) -> &str { s.trim() }

#[goose::export(zero_copy)]
pub fn zc_tail(xs: &[i64]) -> Option<&[i64]> { xs.get(1..) }

#[goose::export(zero_copy)]
pub fn zc_hit(hit: &mut Hit) -> Result<i64, String> { hit.id += 1; Ok(hit.id as i64) }

#[goose::export(zero_copy)]
pub fn zc_optional(hit: Option<&Hit>) -> Option<u64> { hit.map(|h| h.id) }

#[goose::export(zero_copy)]
pub fn zc_address(hit: &Hit) -> usize { hit as *const Hit as usize }

#[goose::export(zero_copy)]
pub fn zc_optional_address(hit: Option<&Hit>) -> usize { hit.map_or(0, |h| h as *const Hit as usize) }

#[goose::export(zero_copy)]
pub fn zc_shapes(shapes: &mut [AShape]) -> Result<f64, String> {
    crate::agrow(shapes, 1.0);
    Ok(crate::ashape_area(shapes))
}

#[goose::export(zero_copy)]
pub fn zc_fill(n: i64, out: &mut GooseVec<i64>) {
    for i in 0..n { out.push(i * i); }
}

#[goose::export(zero_copy)]
pub fn zc_fill_text(out: &mut GooseBuf) { out.push_str("direct output"); }

#[goose::export(zero_copy)]
pub fn zc_words(s: &str) -> impl Iterator<Item = &str> { s.split_whitespace() }

type Number = i64;
#[goose::export(zero_copy)]
pub fn zc_alias(n: Number) -> Number { n + 1 }

#[goose::export(zero_copy)]
pub fn zc_stack_new() -> Stack { crate::stack_new() }

#[goose::export(zero_copy)]
pub fn zc_depth(stack: &Stack) -> i64 { crate::depth(stack) }

#[goose::export(zero_copy)]
pub fn zc_take(stack: Stack) -> i64 { crate::into_sum(stack) }

#[goose::export(zero_copy, instantiate(T = i32, i64))]
pub fn zc_generic<T: Copy>(xs: &[T]) -> i64 { xs.len() as i64 }

// The hidden token precedes the source slice: metadata must still root the
// result at parameter zero of the public Goose signature.
#[goose::export(zero_copy)]
pub fn zc_reenter_view<'a>(goose: &goose::Reentry, s: &'a str) -> Result<&'a str, goose::ReentryError> {
    crate::on_node(goose, 0, s)?;
    Ok(s.trim())
}

#[goose::export(zero_copy)]
pub fn zc_reenter_words<'a>(goose: &goose::Reentry, s: &'a str) -> impl Iterator<Item = &'a str> {
    crate::on_node(goose, 0, s).unwrap();
    s.split_whitespace()
}
