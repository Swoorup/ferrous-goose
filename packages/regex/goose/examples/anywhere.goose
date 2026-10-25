// The regex package from Goose, with no Rust on the machine:
//   goose run anywhere.goose
// The package beside this example is declared by path; a published one is
// declared by its repository, `import rx from "github.com/goose-rs/regex@v1";`.
import rx from "..";

fn main() {
    var err: u8[>..] = [];
    let re, ok = rx::compile("[0-9]+", err);
    guard ok else { abort(err); }
    let hit, found = rx::find(re, "order 1001 shipped");
    if found { print(hit); }
    rx::release(re);

    let date, dok = rx::compile("(?P<y>[0-9]{4})-(?P<m>[0-9]{2})-(?P<d>[0-9]{2})");
    assert(dok);
    let text = "released 2026-10-25, patched 2026-11-02";
    let y, yok = rx::named(date, text, "y");
    print("year ", y, ", groups ", rx::groups(date));
    rx::each_find_all(date, text) { m => print("date: ", m); };
    print(rx::replace_all(date, text, "$d/$m/$y"));
    rx::release(date);

    var opts = rx::Options { case_insensitive: true, multi_line: false, dot_matches_new_line: false,
                             ignore_whitespace: false, unicode: true };
    let goose, gok = rx::compile_with("goose", opts);
    assert(gok);
    print("GOOSE matches: ", rx::is_match(goose, "A GOOSE APPEARS"));
    rx::release(goose);
    print(rx::version());
    print("live handles: ", rx::live_count());
}
