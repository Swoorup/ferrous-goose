// The embed conformance program for any compiler: the shapes a stock
// export takes (spec §7.11).

// Adds two integers.
export fn add(a: i64, b: i64) -> i64 { return a + b; }
export fn mul_f(a: f32, b: f32) -> f32 { return a * b; }
export fn negate(b: bool) -> bool { return !b; }
export fn small(a: u8, b: i16) -> i32 { return (a as i32) + (b as i32); }

export fn sum_bytes(data: const u8[:]) -> u64 {
    var s: u64 = 0;
    for b in data { s += b as u64; }
    return s;
}

export fn reverse_in_place(xs: i32[:]) {
    var i = 0;
    var j = xs.len - 1;
    while i < j {
        let t = xs[i];
        xs[i] = xs[j];
        xs[j] = t;
        i++;
        j--;
    }
}

struct Vec3 { x: f32, y: f32, z: f32 }
struct Particle { pos: Vec3, vel: Vec3, tag: u8 }

export fn make_vec(x: f32, y: f32, z: f32) -> Vec3 { return Vec3 { x: x, y: y, z: z }; }
export fn dot(a: const Vec3&, b: const Vec3&) -> f32 { return a.x * b.x + a.y * b.y + a.z * b.z; }
export fn step(p: Particle&, dt: f32) {
    p.pos.x += p.vel.x * dt;
    p.pos.y += p.vel.y * dt;
    p.pos.z += p.vel.z * dt;
    p.tag++;
}
export fn bump(x: i64&) { x += 1; }

export fn greeting() -> const u8[:] { return "hello from goose"; }

// A view of the argument: Rust ties it to the argument's borrow.
export fn trim(text: const u8[:]) -> const u8[:] {
    var a = 0;
    var b = text.len;
    while a < b && text[a] == ' ' { a++; }
    while b > a && text[b - 1] == ' ' { b--; }
    return text[a..b];
}

enum Mode { Off, Slow, Fast }
export fn next_mode(m: Mode) -> Mode {
    return match m { Off => Mode.Slow, Slow => Mode.Fast, Fast => Mode.Off };
}

// Goose globals keep their values between calls.
var calls: i64 = 0;
export fn count_call() -> i64 {
    calls++;
    return calls;
}
