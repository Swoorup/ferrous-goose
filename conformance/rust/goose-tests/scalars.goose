// Every scalar row: integers of each width, floats, bool, char, usize/isize.
import cf;

fn main() {
    print(cf::add_i8(100 as i8, 100 as i8), " ", cf::add_i16(30000 as i16, 10 as i16), " ", cf::add_i32(-5 as i32, 3 as i32), " ", cf::add_i64(1 << 40, 1));
    print(cf::add_u8(250 as u8, 10 as u8), " ", cf::add_u16(65535 as u16, 2 as u16), " ", cf::add_u32(7 as u32, 8 as u32), " ", cf::add_u64(1 as u64, 2 as u64));
    print(cf::mul_f32(1.5, 2.0), " ", cf::mul_f64(0.25, 8.0));
    print(cf::not(true), " ", cf::not(false), " ", cf::both(true, true), " ", cf::both(true, false));
    print(cf::len_plus(10 as u64, -3), " ", cf::next_char('a' as u32));
    let n, ok = cf::char_utf8_len(0x1F600 as u32);
    print("utf8 len ", n, " ", ok);
    let bad, ok2 = cf::char_utf8_len(0xD800 as u32);
    print("surrogate ", ok2);
}
