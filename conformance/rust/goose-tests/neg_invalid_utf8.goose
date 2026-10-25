// Bytes that are not UTF-8 given as a &str to a function that cannot fail.
import cf;

fn main() {
    var bad: u8[>..] = [];
    bad.push(0xc3);
    print(cf::count_vowels(bad));
}
