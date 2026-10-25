// test: needs emit-interface
// test: needs reentrant
// test: needs typed-builders
import cf;

export fn on_node(depth: i64, label: const u8[:]) -> i64 {
    var work: i64[>..] = [];
    for i in 4 { work.push(i); }
    let n, ok = cf::zc_text(label);
    assert(ok);
    return n + depth + work.len;
}

fn main() {
    var keep: i64[>..] = [7, 8];
    let view, ok = cf::zc_reenter_view("  borrowed  ");
    assert(ok);
    print("view ", view);
    cf::each_zc_reenter_words("one two") { word => print("word ", word); };
    keep.push(9);
    print("kept ", keep);
}
