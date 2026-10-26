// test: needs align
// align n (goose-patches/0006): structs declared with Rust's alignment stay
// aligned in Goose, so Rust borrows them in place; a #[goose(align)] enum
// with data is laid out as Rust's #[repr(C, u8)] and crosses in place too.
import cf;

fn main() {
    let before = cf::copy_count();
    var hits: cf::Hit[>..] = [];
    var junk: u8[>..] = [];
    junk.push(1);
    hits.push(cf::Hit { t: 3.5, id: 7, hit: true });
    hits.push(cf::Hit { t: 1.25, id: 42, hit: true });
    let best, found = cf::nearest(hits);
    print("nearest ", best.id);
    var h = cf::make_hit(1.0, 1);
    cf::hit_bump(h);
    print("bumped ", h.id, " ", h.hit);
    print("copies so far ", cf::copy_count() - before);

    var shapes: cf::AShape[>..] = [];
    shapes.push(cf::AShape.Rect { w: 2.0, h: 3.0 });
    shapes.push(cf::AShape.Dot);
    print("area ", cf::ashape_area(shapes));
    cf::agrow(shapes, 1.0);
    print("grown area ", cf::ashape_area(shapes));
    match shapes[1] { Circle c => print("circle"), Rect r => print("dot became rect ", r.w), Dot => print("dot") }
    let sq = cf::asquare(2.0);
    match sq { Circle c => print("circle"), Rect r => print("square ", r.w * r.h), Dot => print("dot") }
    print("copies in all ", cf::copy_count() - before);
}
