// Flat structs: rustc's layout with explicit pads, by reference, by value,
// through &mut (in place), as results; nested structs, arrays, tuple structs.
import cf;

fn main() {
    print("Hit is ", cf::hit_size(), " bytes in Rust");
    var h = cf::make_hit(1.5, 41);
    print("made: t=", h.t, " id=", h.id, " hit=", h.hit);
    print("score ", cf::hit_score(h), " id ", cf::hit_id(h));
    cf::hit_bump(h);
    print("bumped: t=", h.t, " id=", h.id, " hit=", h.hit);
    print("text: ", cf::format_hit(h));

    var p = cf::Particle { pos: cf::Vec3 { x: 0.0, y: 0.0, z: 0.0 }, vel: cf::Vec3 { x: 1.0, y: 2.0, z: 3.0 }, tag: 7, history: [0.5, 0.25, 0.125] };
    cf::step(p, 0.5);
    cf::step(p, 0.5);
    print("particle at ", p.pos.x, ",", p.pos.y, ",", p.pos.z, " tag ", p.tag, " history ", p.history);

    let pr = cf::Pair { f0: 3, f1: 39 };
    print("pair sum ", cf::pair_sum(pr));
    let sw = cf::swap_pair(pr);
    print("swapped ", sw.f0, " ", sw.f1);

    let a: f32[3] = [1.0, 2.0, 3.0];
    print("dot3 ", cf::dot3(a, [4.0, 5.0, 6.0]));
    var x: i64 = 40;
    let by: i64 = 2;
    cf::scalar_ref(x, by);
    print("scalar ref ", x);
}
