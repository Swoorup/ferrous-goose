//! The shims `#[goose::export]` generates, called from Rust as the Goose C
//! calls them: raw slices and builders in, out-pointers and offsets out.

use goose::abi::{GsRhdr, GsStack};
use goose::{GooseSlice, GsRref, Handle};
use goose_conformance as cf;

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
    fn text(&self) -> String {
        let n = self.hdr.len;
        String::from_utf8_lossy(&self.buf[..n as usize]).into_owned()
    }
}

fn bytes(s: &str) -> GooseSlice<u8> {
    GooseSlice::from_slice(s.as_bytes())
}

#[test]
fn scalars_and_bools() {
    unsafe {
        assert_eq!(cf::cf_add_i64(40, 2), 42);
        assert!(cf::cf_not(0));
        assert!(cf::cf_both(1, 1));
        assert_eq!(cf::cf_next_char('a' as u32), 'b' as u32);
        let mut n = 0i64;
        assert!(!cf::cf_char_utf8_len(0xD800, &mut n), "a surrogate is no char: a failure");
    }
}

#[test]
fn slices_and_write_back() {
    unsafe {
        let xs = [1i32, 2, 3];
        assert_eq!(cf::cf_sum_i32(GooseSlice::from_slice(&xs)), 6);
        let mut packed = vec![0u8; 13];
        for (i, v) in [5i32, 6, 7].iter().enumerate() {
            packed[1 + i * 4..5 + i * 4].copy_from_slice(&v.to_le_bytes());
        }
        let raw = GooseSlice::<i32>::new(packed.as_mut_ptr().add(1), 3);
        cf::cf_scale_i32(raw, 2);
        assert_eq!(&packed[1..5], &10i32.to_le_bytes());
        assert_eq!(&packed[9..13], &14i32.to_le_bytes());
    }
}

#[test]
fn borrowed_results_come_back_as_offsets() {
    unsafe {
        let text = "   centred   ";
        let (mut at, mut len) = (0i64, 0i64);
        cf::cf_trim(bytes(text), &mut at, &mut len);
        assert_eq!(&text[at as usize..(at + len) as usize], "centred");
        assert!(cf::cf_first_word(bytes("  hello goose"), &mut at, &mut len));
        assert_eq!((at, len), (2, 5));
    }
}

#[test]
fn results_and_errors() {
    unsafe {
        let mut v = 0i64;
        let mut err = FakeBuilder::new();
        assert!(cf::cf_parse_int(bytes(" 12 "), &mut v, err.rref()));
        assert_eq!(v, 12);
        assert!(!cf::cf_parse_int(bytes("x"), &mut v, err.rref()));
        assert_eq!(err.text(), "invalid digit found in string");
        let mut err = FakeBuilder::new();
        assert!(!cf::cf_panics_fallible(-1, &mut v, err.rref()));
        assert!(err.text().starts_with("panic: negative input -1"), "{}", err.text());
        let mut out = FakeBuilder::new();
        cf::cf_shout(bytes("honk"), out.rref());
        assert_eq!(out.text(), "HONK!");
    }
}

#[test]
fn structs_through_pointers() {
    unsafe {
        let mut h = cf::Hit { t: 1.5, id: 41, hit: true };
        cf::cf_hit_bump(&mut h as *mut cf::Hit as *mut u8);
        assert_eq!(h, cf::Hit { t: 3.0, id: 42, hit: false });
        let mut out = cf::Hit { t: 0.0, id: 0, hit: false };
        cf::cf_make_hit(2.0, 7, &mut out as *mut cf::Hit as *mut u8);
        assert_eq!(out, cf::Hit { t: 2.0, id: 7, hit: true });
    }
}

#[test]
fn enums_with_data_are_converted() {
    unsafe {
        // Goose's Shape: a tag, then the payload packed: Rect { w: 2, h: 3 }.
        let mut g = [0u8; 17];
        g[0] = 1;
        g[1..9].copy_from_slice(&2.0f64.to_le_bytes());
        g[9..17].copy_from_slice(&3.0f64.to_le_bytes());
        assert_eq!(cf::cf_area(g.as_mut_ptr()), 6.0);
        let mut out = [0u8; 17];
        cf::cf_square(4.0, out.as_mut_ptr());
        assert_eq!(out[0], 1);
        assert_eq!(f64::from_le_bytes(out[1..9].try_into().unwrap()), 4.0);
    }
}

#[test]
fn handles_through_the_shims() {
    unsafe {
        let mut h = Handle::default();
        cf::cf_stack_new(&mut h);
        cf::cf_push(h, 5);
        cf::cf_push(h, 6);
        assert_eq!(cf::cf_depth(h), 2);
        let mut top = 0i64;
        assert!(cf::cf_pop(h, &mut top));
        assert_eq!(top, 6);
        assert_eq!(cf::cf_into_sum(h), 5, "takes the object");
        assert_eq!(cf::cf_stack_live_count(), 0);
    }
}

#[test]
fn the_records() {
    let json = goose::metadata_json();
    assert!(json.starts_with("{\"format\":1,\"abi\":1"), "{}", &json[..60]);
    assert!(json.contains("\"namespace\":\"cf\""));
    assert!(json.contains("\"symbol\":\"cf_trim\""));
    assert!(json.contains("\"name\":\"Hit\",\"doc\""));
    assert!(json.contains("\"goose_size\":17"), "Shape's Goose size");
}
