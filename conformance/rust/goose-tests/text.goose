// Text and bytes: &str (checked for UTF-8), &[u8], builders, String and
// Vec<u8> results; and results borrowed from an argument, re-sliced in Goose.
import cf;

fn main() {
    print("vowels ", cf::count_vowels("the quick brown goose"));
    var loud: u8[>..] = [];
    format(loud, "> ");
    cf::shout("honk at the stack", loud);
    print(loud, " (", loud.len, " bytes)");
    print(cf::shout("fresh"));
    print(cf::repeat("ab", 3));
    print(cf::reversed_bytes("stressed"));

    // Invalid UTF-8 to a fallible function: an error result, with the reason.
    var bad: u8[>..] = [];
    bad.push(0x66);
    bad.push(0xff);
    var err: u8[>..] = [];
    let n, ok = cf::char_count(bad, err);
    print("char_count of bad bytes: ", ok, ", says UTF-8: ", err.len > 10);
    let n2, ok2 = cf::char_count("héllo");
    print("char_count ", n2, " ", ok2);

    // Borrowed results: views of the argument.
    let padded = "   centred   ";
    let t = cf::trim(padded);
    print("[", t, "] ", t.len);
    let w, found = cf::first_word("  hello goose");
    print("first word ", w, " ", found);
    let w2, found2 = cf::first_word("    ");
    print("no word ", found2, " ", w2.len);
    var xs: i64[>..] = [1, 2, 3, 4, 5, 6];
    print("middle ", cf::middle(xs, 2));
    var e: u8[>..] = [];
    let rest, ok3 = cf::after("key=value", "=", e);
    print("after ", rest, " ", ok3);
    let rest2, ok4 = cf::after("novalue", "=", e);
    print("after missing ", ok4, ": ", e);
}
