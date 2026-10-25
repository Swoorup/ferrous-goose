//! The runtime's pieces on their own: argument holders, the aliasing check,
//! builders, handle tables, values and the panic catcher.

use goose::abi::{GsRhdr, GsStack};
use goose::*;

/// A builder over Rust memory, as Goose would hand one over.
struct FakeBuilder {
    buf: Vec<u8>,
    stack: GsStack,
    hdr: GsRhdr,
}

impl FakeBuilder {
    fn new() -> Box<FakeBuilder> {
        let mut b = Box::new(FakeBuilder {
            buf: vec![0u8; 4096],
            stack: GsStack { top: std::ptr::null_mut() },
            hdr: GsRhdr { base: std::ptr::null_mut(), len: 0 },
        });
        let p = b.buf.as_mut_ptr();
        b.stack.top = p;
        b.hdr.base = p;
        b
    }
    fn rref(&mut self) -> GsRref {
        GsRref { hdr: &mut self.hdr, stk: &mut self.stack }
    }
    fn bytes(&self) -> &[u8] {
        let n = self.hdr.len;
        &self.buf[..n as usize]
    }
}

#[test]
fn overlap_check() {
    let a = Span { space: 0, start: 100, end: 200, mutable: true, param: "a" };
    let b = Span { space: 0, start: 150, end: 250, mutable: false, param: "b" };
    let c = Span { space: 0, start: 200, end: 300, mutable: false, param: "c" };
    assert!(check_overlap(&[a, b]).is_err());
    assert!(check_overlap(&[a, c]).is_ok(), "touching ranges do not overlap");
    let shared = Span { mutable: false, ..a };
    assert!(check_overlap(&[shared, b]).is_ok(), "two shared borrows may alias");
    let other_space = Span { space: 7, ..b };
    assert!(check_overlap(&[a, other_space]).is_ok(), "a handle is not memory");
    let msg = check_overlap(&[a, b]).unwrap_err().to_string();
    assert!(msg.contains("arguments a and b share memory"), "{msg}");
}

#[test]
fn slices_lent_in_place_or_copied() {
    let xs: Vec<i32> = vec![1, 2, 3, 4];
    let before = copy_count();
    let s = unsafe { SliceRef::<i32>::hold(GooseSlice::from_slice(&xs), "xs") }.unwrap();
    assert_eq!(s.get(), &[1, 2, 3, 4]);
    assert!(!s.copied());
    assert_eq!(copy_count(), before);

    // Goose's packed storage: four i32s one byte in.
    let mut packed = vec![0u8; 17];
    for (i, v) in [10i32, 20, 30, 40].iter().enumerate() {
        packed[1 + i * 4..1 + i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    let raw = GooseSlice::<i32>::new(unsafe { packed.as_mut_ptr().add(1) }, 4);
    let s = unsafe { SliceRef::<i32>::hold(raw, "xs") }.unwrap();
    assert!(s.copied());
    assert_eq!(s.get(), &[10, 20, 30, 40]);
    assert_eq!(s.get().as_ptr() as usize % 4, 0, "Rust sees an aligned slice");
    drop(s);
    {
        let mut m = unsafe { SliceMut::<i32>::hold(raw, "xs") }.unwrap();
        for x in m.get_mut() {
            *x += 1;
        }
    }
    assert_eq!(&packed[1..5], &11i32.to_le_bytes(), "written back after the call");
    assert_eq!(&packed[13..17], &41i32.to_le_bytes());
}

#[test]
fn empty_and_bad_slices() {
    let s = unsafe { SliceRef::<i64>::hold(GooseSlice::new(std::ptr::null_mut(), 0), "xs") }.unwrap();
    assert!(s.get().is_empty());
    assert!(unsafe { SliceRef::<i64>::hold(GooseSlice::new(std::ptr::null_mut(), 3), "xs") }.is_err());
    assert!(unsafe { SliceRef::<i64>::hold(GooseSlice::new(8 as *mut u8, -1), "xs") }.is_err());
}

#[test]
fn text_is_checked() {
    let ok = "héllo";
    let s = unsafe { StrRef::hold(GooseSlice::from_slice(ok.as_bytes()), "s") }.unwrap();
    assert_eq!(s.get(), ok);
    let bad = [0x66u8, 0xff];
    match unsafe { StrRef::hold(GooseSlice::from_slice(&bad), "s") } {
        Err(BridgeError::Fail(m)) => assert!(m.contains("argument s is not valid UTF-8"), "{m}"),
        other => panic!("expected a failure, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn bools_and_chars_are_validated() {
    let b = [2u8];
    assert!(unsafe { <bool as Value>::goose_read(b.as_ptr()) }.is_none());
    let b = [1u8];
    assert_eq!(unsafe { <bool as Value>::goose_read(b.as_ptr()) }, Some(true));
    let c = 0xD800u32.to_le_bytes();
    assert!(unsafe { <char as Value>::goose_read(c.as_ptr()) }.is_none());
    let c = ('é' as u32).to_le_bytes();
    assert_eq!(unsafe { <char as Value>::goose_read(c.as_ptr()) }, Some('é'));
    let arr = [1u8, 0, 2];
    assert!(unsafe { <[bool; 3] as Value>::goose_read(arr.as_ptr()) }.is_none());
}

#[test]
fn builders_append_at_the_top() {
    let mut fb = FakeBuilder::new();
    {
        let mut b = unsafe { GooseBuf::from_raw(fb.rref()) };
        b.push_str("goose");
        b.push(b'!');
        use std::fmt::Write;
        write!(b, " {}", 42).unwrap();
        assert_eq!(b.len(), 9);
    }
    assert_eq!(fb.bytes(), b"goose! 42");
    let top = fb.stack.top as usize - fb.buf.as_ptr() as usize;
    assert_eq!(top, 9, "the stack top moved with the elements");

    let mut fb = FakeBuilder::new();
    {
        let mut v = unsafe { GooseVec::<i64>::from_raw(fb.rref()) };
        v.extend([1i64, 2, 3]);
        assert_eq!(v.len(), 3, "a typed builder counts elements");
    }
    let top = fb.stack.top as usize - fb.buf.as_ptr() as usize;
    assert_eq!(top, 24);
    assert_eq!(&fb.buf[8..16], &2i64.to_le_bytes());
}

struct Thing(i64);

#[test]
fn handle_tables() {
    static TABLE: HandleTable<Thing> = HandleTable::new("test::Thing");
    let h = TABLE.insert(Thing(5));
    assert_eq!(TABLE.live(), 1);
    assert_eq!(TABLE.shared(h).unwrap().0, 5);
    TABLE.exclusive(h).unwrap().0 += 1;
    assert_eq!(TABLE.shared(h).unwrap().0, 6);
    {
        let _a = TABLE.shared(h).unwrap();
        let _b = TABLE.shared(h).unwrap();
        // Exclusive while shared on this thread: refused, not a deadlock.
        assert!(TABLE.exclusive(h).is_err());
    }
    TABLE.release(h).unwrap();
    assert_eq!(TABLE.live(), 0);
    match TABLE.shared(h) {
        Err(BridgeError::Abort(m)) => assert!(m.contains("stale test::Thing handle (slot 0, generation 1)"), "{m}"),
        _ => panic!("a released handle must be stale"),
    }
    let h2 = TABLE.insert(Thing(9));
    assert_eq!({ h2.slot }, 0, "the slot is reused");
    assert_eq!({ h2.generation }, 2, "under a new generation");
    assert!(TABLE.shared(h).is_err(), "the old handle stays stale");
    assert_eq!(TABLE.take(h2).unwrap().0, 9);
    assert_eq!(TABLE.live(), 0);
}

struct Local(std::rc::Rc<i64>);

#[test]
fn pinned_handles_stay_on_their_thread() {
    static TABLE: HandleTable<Local> = unsafe { HandleTable::new_pinned("test::Local") };
    let h = TABLE.insert(Local(std::rc::Rc::new(3)));
    assert_eq!(*TABLE.shared(h).unwrap().0, 3);
    let other = std::thread::spawn(move || match TABLE.shared(h) {
        Err(BridgeError::Abort(m)) => m,
        _ => String::new(),
    })
    .join()
    .unwrap();
    assert!(other.contains("used on a thread other than the one that made it"), "{other}");
    TABLE.release(h).unwrap();
}

#[test]
fn borrowed_results_lie_inside() {
    let text = "hello goose";
    let part = &text[6..];
    assert_eq!(offset_in(text.as_bytes(), part.as_bytes(), "t").unwrap(), (6, 5));
    assert!(offset_in(text.as_bytes(), "elsewhere".as_bytes(), "t").is_err());
    let xs = [1i64, 2, 3, 4];
    assert_eq!(offset_in(&xs, &xs[1..3], "xs").unwrap(), (1, 2), "in elements");
}

#[test]
fn the_panic_catcher() {
    static FALLIBLE: Site = Site { name: "test::f", fallible: true, main_thread: false };
    let r = call(&FALLIBLE, || -> Result<i64, BridgeError> { panic!("boom {}", 1) }, |m| {
        assert!(m.starts_with("panic: boom 1"), "{m}");
        -1
    });
    assert_eq!(r, -1);
    let r = call(&FALLIBLE, || Err(BridgeError::Fail("bad input".into())), |m: &str| m.len() as i64);
    assert_eq!(r, 9);
    let r = call(&FALLIBLE, || Ok(7), |_| -1);
    assert_eq!(r, 7);
}
