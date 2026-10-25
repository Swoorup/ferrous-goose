// A u32 that is not a Unicode scalar value given as a char.
import cf;

fn main() {
    print(cf::next_char(0xD800));
}
