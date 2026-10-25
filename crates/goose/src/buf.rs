//! Builders: a Goose `u8[>..]&` (and, with typed builders, `T[>..]&`)
//! that Rust appends to. The resizable's elements top their data stack
//! (spec §1.3), so appending writes at the stack top, bumps the top and
//! grows the count, exactly as `gs_bld_append` in `goose.h` does.

use crate::abi::GsRref;
use crate::value::Value;
use core::marker::PhantomData;

/// A Goose `u8[>..]&`: append-only, valid for one call.
pub struct GooseBuf<'call> {
    raw: GsRref,
    _call: PhantomData<&'call mut ()>,
}

impl<'call> GooseBuf<'call> {
    /// # Safety
    /// `raw` must be a builder Goose passed to the current call, and no other
    /// `GooseBuf` over it may be live.
    pub unsafe fn from_raw(raw: GsRref) -> GooseBuf<'call> {
        GooseBuf { raw, _call: PhantomData }
    }

    pub fn extend_from_slice(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        unsafe {
            let stk = self.raw.stk;
            let hdr = self.raw.hdr;
            let top = (*stk).top;
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), top, bytes.len());
            (*stk).top = top.add(bytes.len());
            let len = core::ptr::read_unaligned(core::ptr::addr_of!((*hdr).len));
            core::ptr::write_unaligned(core::ptr::addr_of_mut!((*hdr).len), len + bytes.len() as i64);
        }
    }

    pub fn push(&mut self, b: u8) {
        self.extend_from_slice(&[b]);
    }

    pub fn push_str(&mut self, s: &str) {
        self.extend_from_slice(s.as_bytes());
    }

    /// How many bytes the builder holds, what was there before the call
    /// included.
    pub fn len(&self) -> usize {
        unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*self.raw.hdr).len)) as usize }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl core::fmt::Write for GooseBuf<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.push_str(s);
        Ok(())
    }
}

impl std::io::Write for GooseBuf<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Extend<u8> for GooseBuf<'_> {
    fn extend<I: IntoIterator<Item = u8>>(&mut self, iter: I) {
        for b in iter {
            self.push(b);
        }
    }
}

/// A typed builder, Goose's `T[>..]&` (goose-patches/0002): values are
/// appended as Goose stores them, and the count grows by one per value.
pub struct GooseVec<'call, T: Value> {
    raw: GsRref,
    _call: PhantomData<&'call mut T>,
}

impl<'call, T: Value> GooseVec<'call, T> {
    /// # Safety
    /// `raw` must be a `T[>..]&` builder Goose passed to the current call.
    pub unsafe fn from_raw(raw: GsRref) -> GooseVec<'call, T> {
        GooseVec { raw, _call: PhantomData }
    }

    pub fn push(&mut self, v: T) {
        unsafe {
            let stk = self.raw.stk;
            let hdr = self.raw.hdr;
            let top = (*stk).top;
            v.goose_write(top);
            (*stk).top = top.add(T::GOOSE_SIZE);
            let len = core::ptr::read_unaligned(core::ptr::addr_of!((*hdr).len));
            core::ptr::write_unaligned(core::ptr::addr_of_mut!((*hdr).len), len + 1);
        }
    }

    pub fn extend_from_slice(&mut self, vs: &[T]) {
        for v in vs {
            self.push(*v);
        }
    }

    pub fn len(&self) -> usize {
        unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*self.raw.hdr).len)) as usize }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T: Value> Extend<T> for GooseVec<'_, T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for v in iter {
            self.push(v);
        }
    }
}
