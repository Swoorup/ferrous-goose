//! Exercise actual strict shims, including their failure paths and allocations.

use goose::abi::{GsRhdr, GsStack};
use goose::{GooseSlice, GsRref, Handle};
use goose_conformance::{self as cf, zero_copy as zc};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct Meter;
thread_local! {
    static MEASURE: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}
fn allocation() {
    if MEASURE.try_with(|m| m.get()).unwrap_or(false) {
        let _ = ALLOCS.try_with(|n| n.set(n.get() + 1));
    }
}
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 { allocation(); System.alloc(l) }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 { allocation(); System.alloc_zeroed(l) }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 { allocation(); System.realloc(p, l, n) }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) { System.dealloc(p, l); }
}
#[global_allocator]
static ALLOCATOR: Meter = Meter;

struct Builder { storage: [u64; 512], stack: GsStack, hdr: GsRhdr }
impl Builder {
    fn new() -> Box<Self> {
        let mut b = Box::new(Self {
            storage: [0; 512], stack: GsStack { top: std::ptr::null_mut() },
            hdr: GsRhdr { base: std::ptr::null_mut(), len: 0 },
        });
        b.stack.top = b.storage.as_mut_ptr().cast();
        b.hdr.base = b.stack.top;
        b
    }
    fn raw(&mut self) -> GsRref { GsRref { hdr: &mut self.hdr, stk: &mut self.stack } }
    fn text(&self) -> String {
        String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(self.hdr.base, self.hdr.len as usize) }).into_owned()
    }
}

#[test]
fn steady_state_borrows_and_direct_output_do_not_allocate() {
    let mut xs = [1i64, 2, 3];
    let mut result = 0;
    let mut err = Builder::new();
    let mut words = Builder::new();
    let mut squares = Builder::new();
    let text = "one two three";
    // Warm up the panic catcher/TLS before measuring the steady-state call.
    assert!(unsafe { zc::cf_zc_sum(GooseSlice::from_slice(&xs), &mut result, err.raw()) });
    let before = goose::copy_count();
    ALLOCS.with(|n| n.set(0));
    MEASURE.with(|m| m.set(true));
    let (ok, scaled, ptr) = unsafe {
        let mut held = goose::SliceMut::<i64>::hold_zero_copy(GooseSlice::from_mut_slice(&mut xs), "xs").unwrap();
        let ptr = held.get_mut().as_mut_ptr();
        drop(held);
        let ok = zc::cf_zc_scale(GooseSlice::from_mut_slice(&mut xs), 2, &mut result, err.raw());
        zc::cf_zc_words(GooseSlice::from_slice(text.as_bytes()), words.raw());
        zc::cf_zc_fill(4, squares.raw());
        (ok, result, ptr)
    };
    MEASURE.with(|m| m.set(false));
    assert_eq!(ALLOCS.with(|n| n.get()), 0, "the bridge needs no temporary payload allocation");
    assert_eq!(goose::copy_count(), before);
    assert!(ok);
    assert_eq!(ptr, xs.as_mut_ptr());
    assert_eq!(scaled, 12);
    assert_eq!(xs, [2, 4, 6]);
    assert_eq!({ words.hdr.len }, 3);
    let spans = unsafe { std::slice::from_raw_parts(words.hdr.base.cast::<goose::GooseSpan>(), 3) };
    assert_eq!(spans, &[goose::GooseSpan { at: 0, len: 3 }, goose::GooseSpan { at: 4, len: 3 }, goose::GooseSpan { at: 8, len: 5 }]);
    assert_eq!({ squares.hdr.len }, 4);
    assert_eq!(&squares.storage[..4], &[0, 1, 4, 9]);
}

#[test]
fn misalignment_fails_without_entering_the_body_or_copying() {
    let mut storage = [0u64; 5]; // known aligned base; one byte in is misaligned
    let p = unsafe { storage.as_mut_ptr().cast::<u8>().add(1) };
    let original = 7i64.to_ne_bytes();
    unsafe { std::ptr::copy_nonoverlapping(original.as_ptr(), p, 8); }
    let before = goose::copy_count();
    let mut result = 99;
    let mut err = Builder::new();
    assert!(!unsafe { zc::cf_zc_scale(GooseSlice::new(p, 1), 2, &mut result, err.raw()) });
    assert!(err.text().contains("zero_copy argument xs"), "{}", err.text());
    assert_eq!(result, 99);
    assert!(!unsafe { zc::cf_zc_bump(p, &mut result) }, "Option reports failure too");
    assert!(unsafe { goose::ValRef::<i64>::hold_zero_copy(p, "x") }.is_err());
    assert!(unsafe { goose::SliceRef::<i64>::hold_zero_copy(GooseSlice::new(p, 1), "xs") }.is_err());
    assert_eq!(unsafe { std::slice::from_raw_parts(p, 8) }, &original);
    assert!(unsafe { goose::SliceRef::<cf::Shape>::hold_zero_copy(GooseSlice::new(std::ptr::null_mut(), 0), "shapes") }.is_err(),
        "even an empty slice cannot satisfy a mismatched layout contract");
    assert_eq!(goose::copy_count(), before);
}

#[test]
fn validity_checks_are_preserved() {
    let mut result = 99;
    let mut err = Builder::new();
    let bad_text = [0xffu8];
    assert!(!unsafe { zc::cf_zc_text(GooseSlice::from_slice(&bad_text), &mut result, err.raw()) });
    assert!(err.text().contains("not valid UTF-8"));
    let mut err = Builder::new();
    let bad_bool = [2u8];
    assert!(!unsafe { zc::cf_zc_bools(GooseSlice::new(bad_bool.as_ptr() as *mut u8, 1), &mut result, err.raw()) });
    let mut err = Builder::new();
    let surrogate = [0xd800u32];
    assert!(!unsafe { zc::cf_zc_chars(GooseSlice::new(surrogate.as_ptr() as *mut u8, 1), &mut result, err.raw()) });
    let mut err = Builder::new();
    let mut hit_bytes = [0u64, 7, 0];
    let p = hit_bytes.as_mut_ptr().cast::<u8>();
    unsafe { *p.add(std::mem::offset_of!(cf::Hit, hit)) = 2; }
    assert!(!unsafe { zc::cf_zc_hit(p, &mut result, err.raw()) });
    assert_eq!(hit_bytes[1], 7);
    let mut bad_tag = [255u64, 0, 0];
    let mut area = 99.0;
    let mut err = Builder::new();
    assert!(!unsafe { zc::cf_zc_shapes(GooseSlice::new(bad_tag.as_mut_ptr().cast(), 1), &mut area, err.raw()) });
    assert_eq!(area, 99.0);
    assert_eq!(result, 99);
}

#[test]
fn aggregates_views_optionals_aliases_and_handles() {
    let mut hit = cf::Hit { t: 1.0, id: 7, hit: true };
    let mut result = 0;
    let mut err = Builder::new();
    assert!(unsafe { zc::cf_zc_hit((&mut hit as *mut cf::Hit).cast(), &mut result, err.raw()) });
    assert_eq!(hit.id, 8);
    let mut id = 0u64;
    assert!(unsafe { zc::cf_zc_optional_some((&hit as *const cf::Hit as *mut cf::Hit).cast(), &mut id) });
    assert_eq!(id, 8);
    assert!(!unsafe { zc::cf_zc_optional_none(&mut id) });
    let mut shape = [cf::AShape::Dot];
    let mut area = 0.0;
    assert!(unsafe { zc::cf_zc_shapes(GooseSlice::from_mut_slice(&mut shape), &mut area, err.raw()) });
    assert_eq!(area, 1.0);
    assert_eq!(shape[0], cf::AShape::Rect { w: 1.0, h: 1.0 });
    assert_eq!(unsafe { *(&shape[0] as *const cf::AShape).cast::<u64>() }, 1, "mutable enum tag is fixed up");
    let (mut at, mut len) = (99, 99);
    unsafe { zc::cf_zc_trim(GooseSlice::from_slice(b"  view  "), &mut at, &mut len); }
    assert_eq!((at, len), (2, 4));
    assert!(unsafe { zc::cf_zc_tail(GooseSlice::from_slice(&[1i64, 2, 3]), &mut at, &mut len) });
    assert_eq!((at, len), (1, 2));
    let input = 9i64;
    unsafe { zc::cf_zc_alias((&input as *const i64).cast(), (&mut result as *mut i64).cast()); }
    assert_eq!(result, 10);
    let mut fixed = [1i64, 2, 3];
    assert_eq!(unsafe { zc::cf_zc_fixed((&mut fixed as *mut [i64; 3]).cast()) }, 7);
    assert_eq!(fixed, [2, 2, 3]);
    let mut handle = Handle::default();
    unsafe { zc::cf_zc_stack_new(&mut handle); }
    assert_eq!(unsafe { zc::cf_zc_depth(handle) }, 0);
    assert_eq!(unsafe { zc::cf_zc_take(handle) }, 0);
}
