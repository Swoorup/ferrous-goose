// Phase 3 from Goose: cargo goose run goose/main.goose --manifest-path rust/Cargo.toml
import geo;

fn main() {
    var shapes: geo::Shape[>..] = [];
    shapes.push(geo::Shape.Rect { w: 2.0, h: 3.0 });
    shapes.push(geo::Shape.Circle { r: 1.0 });
    shapes.push(geo::Shape.Dot);
    print("total area ", geo::total_area(shapes) > 9.14);
    let big, found = geo::largest(shapes);
    print("largest is a ", geo::describe(big, null));

    var style = geo::Style { scale: 10.0, outline: true };
    print(geo::describe(shapes[1], style));

    let xs: f32[3] = [1.0, 2.0, 6.0];
    let ys: f64[2] = [0.5, 1.5];
    print("means ", geo::mean(xs), " and ", geo::mean(ys));

    geo::each_fields("goose, rust, no c") { f => print("field [", f, "]"); };
    var count = 0;
    geo::each_circle_points(4, 2.0) { p => count++; };
    print(count, " points on a circle");

    let round = geo::keep_area(shapes) { s => match s { Circle c => true, Rect r => false, Dot => false } };
    print("round area ", round > 3.14 && round < 3.15);
    print("live handles ", geo::live_count());
}
