// Two Rust crates in one program, each the native part of a package of its
// own (goose-patches/0004): goose run and goose build build both crates and
// link them as one library.
//   goose run multi.goose      goose build multi.goose
import rx from "./rx";
import geo from "./geo";

fn main() {
    let re, ok = rx::compile("[0-9]+([.][0-9]+)?");
    assert(ok);
    let line = "circle 2.5, rect 3 by 4";
    var nums: f64[>..] = [];
    rx::each_find_all(re, line) { m =>
        var v: f64 = 0.0;
        var scale: f64 = 0.0;
        for c in m {
            if c == '.' { scale = 1.0; }
            else {
                v = v * 10.0 + ((c - '0') as f64);
                if scale > 0.0 { scale *= 10.0; }
            }
        }
        if scale > 0.0 { v = v / scale; }
        nums.push(v);
    };
    print("numbers: ", nums);
    var shapes: geo::Shape[>..] = [];
    shapes.push(geo::Shape.Circle { r: nums[0] });
    shapes.push(geo::Shape.Rect { w: nums[1], h: nums[2] });
    print("area over 30: ", geo::total_area(shapes) > 31.6);
    let big, found = geo::largest(shapes);
    print("largest: ", geo::describe(big, null));
    rx::release(re);
    print("live: ", rx::live_count());
}
