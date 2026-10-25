// The regex crate from Goose: cargo goose run goose/find_emails.goose
// (from examples/regex, or --manifest-path rust/Cargo.toml from elsewhere).
import rx;          // generated from the crate's exports

fn main() {
    var err: u8[>..] = [];
    let re, ok = rx::compile("[a-z]+@[a-z]+[.]com", err);
    guard ok else { abort(err); }
    let text = "write to ann@example.com or bob@example.com today";
    if rx::is_match(re, text) { print("has an address"); }
    let hit, found = rx::find(re, text);
    if found { print("found ", hit, ", a view into text at ", hit.len, " bytes"); }
    print(rx::replace_all(re, text, "[redacted]"));
    print("matches: ", rx::count(re, text));
    rx::each_find_all(re, text) { m => print("  address: ", m); };
    let words, wok = rx::compile("[ ]+");
    assert(wok);
    rx::each_split(words, "no heap  no gc   no lifetimes") { w => print("  word: ", w); };
    print("pattern back from Rust: ", rx::pattern(re));
    print("escaped: ", rx::escape("a.b*c"));

    // A bad pattern is a failure with Rust's error text, not a crash.
    var bad: u8[>..] = [];
    let bad_re, ok2 = rx::compile("([unclosed", bad);
    if !ok2 { print("compile failed: ", bad.len > 0); }

    rx::release(re);
    rx::release(words);
    print("live handles: ", rx::live_count());
}
