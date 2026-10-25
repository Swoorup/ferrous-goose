// test: needs emit-interface
// test: needs align
// test: needs typed-builders
import cf;

fn main() {
    let before = cf::copy_count();
    var xs: i64[>..] = [1, 2, 3];
    let sum, ok = cf::zc_sum(xs);
    assert(ok);
    print("sum ", sum);
    let scaled, changed = cf::zc_scale(xs, 2);
    assert(changed);
    print("scaled ", xs, " = ", scaled);
    var hit = cf::Hit { t: 1.0, id: 7, hit: true };
    let id, bumped = cf::zc_hit(hit);
    assert(bumped);
    print("hit ", id, " ", hit.id);
    assert(cf::zc_optional_address(hit) == cf::zc_address(hit));
    assert(cf::zc_optional_address(null) == 0);
    assert(cf::zc_alias(9) == 10);
    assert(cf::zc_generic(xs) == 3);
    print("view ", cf::zc_trim("  view  "));
    cf::each_zc_words("one two three") { word => print("word ", word); };
    print("squares ", cf::zc_fill(4));
    var shapes: cf::AShape[>..] = [];
    shapes.push(cf::AShape.Dot);
    let area, grew = cf::zc_shapes(shapes);
    assert(grew);
    match shapes[0] { Rect r => print("rect ", r.w, " ", r.h), Circle c => assert(false), Dot => assert(false) }
    print("copies ", cf::copy_count() - before);
}
