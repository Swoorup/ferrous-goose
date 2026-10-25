// Enums: fieldless and numbered as Goose numbers them (in place), fieldless
// with explicit discriminants (converted), and with data (converted: 17
// bytes in Goose, 24 in Rust), alone, in slices and inside a struct.
import cf;

fn shape_name(s: cf::Shape) -> const u8[:] {
    return match s { Circle c => "circle", Rect r => "rect", Dot => "dot" };
}

fn main() {
    var m = cf::Mode.Off;
    m = cf::next_mode(m);
    m = cf::next_mode(m);
    print("mode is fast: ", m == cf::Mode.Fast);
    print("levels ", cf::level_value(cf::Level.Low), " ", cf::level_value(cf::Level.High));
    var l = cf::Level.Low;
    cf::raise(l);
    print("raised to mid: ", l == cf::Level.Mid);

    let c: cf::Shape = cf::Shape.Circle { r: 1.0 };
    print("circle area ", cf::area(c) > 3.14 && cf::area(c) < 3.15);
    var shapes: cf::Shape[>..] = [];
    shapes.push(cf::Shape.Rect { w: 2.0, h: 3.0 });
    shapes.push(cf::Shape.Dot);
    shapes.push(cf::Shape.Rect { w: 1.0, h: 1.0 });
    print("total ", cf::total_area(shapes));
    cf::grow(shapes, 1.0);
    print("grown: ", shape_name(shapes[0]), " ", shape_name(shapes[1]), " total ", cf::total_area(shapes) > 19.1);
    let sq = cf::square(4.0);
    print("square ", shape_name(sq), " ", cf::area(sq));
    let t = cf::Tagged { id: 2, shape: cf::Shape.Rect { w: 5.0, h: 2.0 } };
    print("tagged ", cf::tagged_area(t));
}
