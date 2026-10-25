//! The conformance crate: one export (at least) per row of the type map,
//! each doing something a Goose test can check the result of.

use goose::{GooseBuf, GooseVec};
use std::fmt::Write as _;

// ---- scalars ----

#[goose::export]
pub fn add_i8(a: i8, b: i8) -> i8 {
    a.wrapping_add(b)
}
#[goose::export]
pub fn add_i16(a: i16, b: i16) -> i16 {
    a.wrapping_add(b)
}
#[goose::export]
pub fn add_i32(a: i32, b: i32) -> i32 {
    a.wrapping_add(b)
}
#[goose::export]
pub fn add_i64(a: i64, b: i64) -> i64 {
    a.wrapping_add(b)
}
#[goose::export]
pub fn add_u8(a: u8, b: u8) -> u8 {
    a.wrapping_add(b)
}
#[goose::export]
pub fn add_u16(a: u16, b: u16) -> u16 {
    a.wrapping_add(b)
}
#[goose::export]
pub fn add_u32(a: u32, b: u32) -> u32 {
    a.wrapping_add(b)
}
#[goose::export]
pub fn add_u64(a: u64, b: u64) -> u64 {
    a.wrapping_add(b)
}
#[goose::export]
pub fn mul_f32(a: f32, b: f32) -> f32 {
    a * b
}
#[goose::export]
pub fn mul_f64(a: f64, b: f64) -> f64 {
    a * b
}
#[goose::export]
pub fn not(b: bool) -> bool {
    !b
}
#[goose::export]
pub fn both(a: bool, b: bool) -> bool {
    a && b
}
/// usize and isize are u64 and i64 in Goose.
#[goose::export]
pub fn len_plus(n: usize, d: isize) -> usize {
    (n as isize + d) as usize
}
/// A char is a u32 that must be a Unicode scalar value.
#[goose::export]
pub fn next_char(c: char) -> char {
    char::from_u32(c as u32 + 1).unwrap_or('?')
}
/// Fallible, so an invalid char is a failure rather than an abort.
#[goose::export]
pub fn char_utf8_len(c: char) -> Option<i64> {
    Some(c.len_utf8() as i64)
}

// ---- flat structs ----

/// Rust aligns `id` to 8 and pads the tail to 24 bytes; the Goose
/// declaration gets the same pads.
#[derive(goose::Flat, Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Hit {
    pub t: f32,
    pub id: u64,
    pub hit: bool,
}

#[derive(goose::Flat, Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Nested structs and a fixed array.
#[derive(goose::Flat, Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Particle {
    pub pos: Vec3,
    pub vel: Vec3,
    pub tag: u8,
    pub history: [f32; 3],
}

/// A tuple struct: fields f0, f1 in Goose.
#[derive(goose::Flat, Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Pair(pub i32, pub i64);

#[goose::export]
pub fn hit_size() -> i64 {
    std::mem::size_of::<Hit>() as i64
}

/// By reference.
#[goose::export]
pub fn hit_score(h: &Hit) -> f64 {
    if h.hit {
        h.t as f64 + h.id as f64
    } else {
        -1.0
    }
}

/// By value: a copy of the Goose value.
#[goose::export]
pub fn hit_id(h: Hit) -> u64 {
    h.id
}

/// Through &mut: written back.
#[goose::export]
pub fn hit_bump(h: &mut Hit) {
    h.id += 1;
    h.t *= 2.0;
    h.hit = !h.hit;
}

/// A struct result, through an out-pointer.
#[goose::export]
pub fn make_hit(t: f32, id: u64) -> Hit {
    Hit { t, id, hit: true }
}

#[goose::export]
pub fn step(p: &mut Particle, dt: f32) {
    p.history = [p.history[1], p.history[2], p.pos.x];
    p.pos.x += p.vel.x * dt;
    p.pos.y += p.vel.y * dt;
    p.pos.z += p.vel.z * dt;
    p.tag += 1;
}

#[goose::export]
pub fn pair_sum(p: &Pair) -> i64 {
    p.0 as i64 + p.1
}

#[goose::export]
pub fn swap_pair(p: Pair) -> Pair {
    Pair(p.1 as i32, p.0 as i64)
}

/// Arrays by reference and by value.
#[goose::export]
pub fn dot3(a: &[f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[goose::export]
pub fn scalar_ref(x: &mut i64, by: &i64) {
    *x += *by;
}

// ---- enums ----

/// Fieldless, #[repr(u8)], numbered as Goose numbers them: one byte, in place.
#[derive(goose::Flat, Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum Mode {
    Off,
    Slow,
    Fast,
}

/// Explicit discriminants: converted by index at the boundary.
#[derive(goose::Flat, Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum Level {
    Low = 10,
    Mid = 20,
    High = 30,
}

#[goose::export]
pub fn next_mode(m: Mode) -> Mode {
    match m {
        Mode::Off => Mode::Slow,
        Mode::Slow => Mode::Fast,
        Mode::Fast => Mode::Off,
    }
}

#[goose::export]
pub fn level_value(l: Level) -> i64 {
    l as i64
}

#[goose::export]
pub fn raise(l: &mut Level) {
    *l = match *l {
        Level::Low => Level::Mid,
        _ => Level::High,
    }
}

/// An enum with data: 24 bytes in Rust, 17 in Goose (a tag and the largest
/// payload, packed). Converted at the boundary.
#[derive(goose::Flat, Clone, Copy, Debug, PartialEq)]
#[repr(C, u8)]
pub enum Shape {
    Circle { r: f64 },
    Rect { w: f64, h: f64 },
    Dot,
}

#[goose::export]
pub fn area(s: &Shape) -> f64 {
    match s {
        Shape::Circle { r } => std::f64::consts::PI * r * r,
        Shape::Rect { w, h } => w * h,
        Shape::Dot => 0.0,
    }
}

#[goose::export]
pub fn total_area(shapes: &[Shape]) -> f64 {
    shapes.iter().map(area).sum()
}

#[goose::export]
pub fn grow(shapes: &mut [Shape], by: f64) {
    for s in shapes {
        match s {
            Shape::Circle { r } => *r += by,
            Shape::Rect { w, h } => {
                *w += by;
                *h += by;
            }
            Shape::Dot => *s = Shape::Circle { r: by },
        }
    }
}

#[goose::export]
pub fn square(side: f64) -> Shape {
    Shape::Rect { w: side, h: side }
}

/// A struct holding an enum with data: converted too.
#[derive(goose::Flat, Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Tagged {
    pub id: u16,
    pub shape: Shape,
}

#[goose::export]
pub fn tagged_area(t: &Tagged) -> f64 {
    area(&t.shape) + t.id as f64
}

/// An enum with data Goose lays out as Rust does (#[goose(align)],
/// goose-patches/0007): lent in place, never converted.
#[derive(goose::Flat, Clone, Copy, Debug, PartialEq)]
#[repr(C, u8)]
#[goose(align)]
pub enum AShape {
    Circle { r: f64 },
    Rect { w: f64, h: f64 },
    Dot,
}

#[goose::export]
pub fn ashape_area(shapes: &[AShape]) -> f64 {
    shapes
        .iter()
        .map(|s| match s {
            AShape::Circle { r } => std::f64::consts::PI * r * r,
            AShape::Rect { w, h } => w * h,
            AShape::Dot => 0.0,
        })
        .sum()
}

#[goose::export]
pub fn agrow(shapes: &mut [AShape], by: f64) {
    for s in shapes {
        *s = match *s {
            AShape::Circle { r } => AShape::Circle { r: r + by },
            AShape::Rect { w, h } => AShape::Rect { w: w + by, h: h + by },
            AShape::Dot => AShape::Rect { w: by, h: by },
        }
    }
}

#[goose::export]
pub fn asquare(side: f64) -> AShape {
    AShape::Rect { w: side, h: side }
}

// ---- slices ----

#[goose::export]
pub fn sum_i32(xs: &[i32]) -> i64 {
    xs.iter().map(|&x| x as i64).sum()
}

#[goose::export]
pub fn scale_i32(xs: &mut [i32], k: i32) {
    for x in xs {
        *x *= k;
    }
}

#[goose::export]
pub fn sum_f64(xs: &[f64]) -> f64 {
    xs.iter().sum()
}

#[goose::export]
pub fn nearest(hits: &[Hit]) -> Option<Hit> {
    hits.iter().filter(|h| h.hit).min_by(|a, b| a.t.total_cmp(&b.t)).copied()
}

#[goose::export]
pub fn bytes_xor(data: &[u8]) -> u8 {
    data.iter().fold(0, |a, b| a ^ b)
}

#[goose::export]
pub fn upper_in_place(data: &mut [u8]) {
    data.make_ascii_uppercase();
}

/// Copies one slice into another: overlapping arguments abort.
#[goose::export]
pub fn copy_into(dst: &mut [i32], src: &[i32]) {
    let n = dst.len().min(src.len());
    dst[..n].copy_from_slice(&src[..n]);
}

/// Owned parameters: copies Rust keeps.
#[goose::export]
pub fn owned_len(s: String, v: Vec<i32>, b: Vec<u8>) -> i64 {
    (s.len() + v.len() + b.len()) as i64
}

// ---- text ----

#[goose::export]
pub fn count_vowels(s: &str) -> i64 {
    s.chars().filter(|c| "aeiouAEIOU".contains(*c)).count() as i64
}

/// Fallible: invalid UTF-8 is an error result.
#[goose::export]
pub fn char_count(s: &str) -> Result<i64, String> {
    Ok(s.chars().count() as i64)
}

#[goose::export]
pub fn shout(s: &str, out: &mut GooseBuf) {
    for c in s.chars() {
        out.extend_from_slice(c.to_uppercase().to_string().as_bytes());
    }
    out.push(b'!');
}

#[goose::export]
pub fn repeat(s: &str, n: i64) -> String {
    s.repeat(n.max(0) as usize)
}

#[goose::export]
pub fn reversed_bytes(b: &[u8]) -> Vec<u8> {
    b.iter().rev().copied().collect()
}

#[goose::export]
pub fn format_hit(h: &Hit, out: &mut GooseBuf) {
    let _ = write!(out, "Hit {{ t: {}, id: {}, hit: {} }}", h.t, h.id, h.hit);
}

// ---- borrowed results ----

#[goose::export]
pub fn trim(s: &str) -> &str {
    s.trim()
}

#[goose::export]
pub fn first_word(s: &str) -> Option<&str> {
    s.split_whitespace().next()
}

#[goose::export]
pub fn middle<'a>(xs: &'a [i64], skip: i64) -> &'a [i64] {
    let k = (skip.max(0) as usize).min(xs.len() / 2);
    &xs[k..xs.len() - k]
}

#[goose::export]
pub fn after<'a>(s: &'a str, sep: &str) -> Result<&'a str, String> {
    match s.find(sep) {
        Some(i) => Ok(&s[i + sep.len()..]),
        None => Err(format!("no {sep:?} in {s:?}")),
    }
}

/// A result that claims to borrow from its argument but does not: caught.
#[goose::export]
pub fn bad_borrow(_s: &str) -> &str {
    "not from the argument"
}

// ---- Option and Result ----

#[goose::export]
pub fn checked_div(a: i64, b: i64) -> Option<i64> {
    a.checked_div(b)
}

#[goose::export]
pub fn parse_int(s: &str) -> Result<i64, std::num::ParseIntError> {
    s.trim().parse()
}

#[goose::export]
pub fn must_be_positive(x: i64) -> Result<(), String> {
    if x > 0 {
        Ok(())
    } else {
        Err(format!("{x} is not positive"))
    }
}

#[goose::export]
pub fn parse_hit(s: &str) -> Result<Hit, String> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 2 {
        return Err("expected t,id".into());
    }
    let t = parts[0].trim().parse::<f32>().map_err(|e| e.to_string())?;
    let id = parts[1].trim().parse::<u64>().map_err(|e| e.to_string())?;
    Ok(Hit { t, id, hit: true })
}

#[goose::export]
pub fn maybe_name(n: i64) -> Option<String> {
    (n > 0).then(|| format!("item-{n}"))
}

/// `throws`: the Goose wrapper throws the error text.
#[goose::export(throws)]
pub fn strict_parse(s: &str) -> Result<i64, String> {
    s.parse::<i64>().map_err(|e| format!("cannot parse {s:?}: {e}"))
}

// ---- panics ----

/// A panic in a function returning Result is an error result.
#[goose::export]
pub fn panics_fallible(x: i64) -> Result<i64, String> {
    if x < 0 {
        panic!("negative input {x}");
    }
    Ok(x)
}

/// A panic anywhere else ends the program as a Goose abort.
#[goose::export]
pub fn panics(x: i64) -> i64 {
    if x < 0 {
        panic!("negative input {x}");
    }
    x
}

// ---- handles ----

/// A Rust object Goose holds by handle.
#[goose::handle]
pub struct Stack {
    items: Vec<i64>,
}

#[goose::export]
pub fn stack_new() -> Stack {
    Stack { items: Vec::new() }
}

#[goose::export]
pub fn push(s: &mut Stack, v: i64) {
    s.items.push(v);
}

#[goose::export]
pub fn pop(s: &mut Stack) -> Option<i64> {
    s.items.pop()
}

#[goose::export]
pub fn depth(s: &Stack) -> i64 {
    s.items.len() as i64
}

/// Takes the object: the handle is gone afterwards.
#[goose::export]
pub fn into_sum(s: Stack) -> i64 {
    s.items.iter().sum()
}

/// Two handles of one type, one of them mutable: the same handle twice aborts.
#[goose::export]
pub fn move_all(to: &mut Stack, from: &Stack) {
    to.items.extend_from_slice(&from.items);
}

/// A handle type that is not Send + Sync: pinned to its thread.
#[goose::handle(pinned)]
pub struct Local {
    count: std::rc::Rc<std::cell::Cell<i64>>,
}

#[goose::export]
pub fn local_new() -> Local {
    Local { count: std::rc::Rc::new(std::cell::Cell::new(0)) }
}

#[goose::export]
pub fn local_bump(l: &Local) -> i64 {
    l.count.set(l.count.get() + 1);
    l.count.get()
}

// ---- Vec results (typed builders, or counted and copied) ----

#[goose::export]
pub fn primes(n: i64) -> Vec<i64> {
    (2..).filter(|k: &i64| (2..*k).take_while(|d| d * d <= *k).all(|d| k % d != 0)).take(n.max(0) as usize).collect()
}

#[goose::export]
pub fn hits_above(hits: &[Hit], t: f32) -> Vec<Hit> {
    hits.iter().filter(|h| h.t > t).copied().collect()
}

#[goose::export]
pub fn shapes_of(n: i64) -> Vec<Shape> {
    (0..n).map(|i| if i % 2 == 0 { Shape::Circle { r: i as f64 } } else { Shape::Rect { w: 1.0, h: i as f64 } }).collect()
}

#[goose::export]
pub fn try_primes(n: i64) -> Result<Vec<i64>, String> {
    if n < 0 {
        Err("n must not be negative".into())
    } else {
        Ok(primes(n))
    }
}

/// A typed builder as a parameter (goose-patches/0001).
#[goose::export]
pub fn fill_squares(n: i64, out: &mut GooseVec<i64>) {
    for i in 0..n {
        out.push(i * i);
    }
}

// ---- iterators ----

/// Spans: each word a view of the text.
#[goose::export]
pub fn words(text: &str) -> impl Iterator<Item = &str> + '_ {
    text.split_whitespace()
}

/// Spans over a slice of values.
#[goose::export]
pub fn runs(xs: &[i64]) -> impl Iterator<Item = &[i64]> + '_ {
    xs.chunk_by(|a, b| a == b)
}

/// A lazy iterator making values: a handle with next.
#[goose::export]
pub fn countdown(from: i64) -> impl Iterator<Item = i64> {
    (0..=from).rev()
}

#[goose::export]
pub fn labels(n: i64) -> impl Iterator<Item = String> {
    (1..=n).map(|i| format!("label {i}"))
}

#[goose::export]
pub fn hits_iter(n: u64) -> impl Iterator<Item = Hit> {
    (0..n).map(|i| Hit { t: i as f32 * 0.5, id: i, hit: i % 2 == 0 })
}

/// An iterator borrowing its argument but making values: collected.
#[goose::export]
pub fn doubled(xs: &[i64]) -> impl Iterator<Item = i64> + '_ {
    xs.iter().map(|x| x * 2)
}

// ---- generics ----

#[goose::export(instantiate(T = f32, f64, i64))]
pub fn dot<T>(a: &[T], b: &[T]) -> T
where
    T: Copy + std::iter::Sum + std::ops::Mul<Output = T>,
{
    a.iter().zip(b).map(|(x, y)| *x * *y).sum()
}

#[goose::export(instantiate(T = i32, u8))]
pub fn largest<T: Copy + Ord + Default>(xs: &[T]) -> T {
    xs.iter().copied().max().unwrap_or_default()
}

// ---- optional parameters ----

#[derive(goose::Flat, Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Config {
    pub level: i32,
    pub verbose: bool,
}

#[goose::export]
pub fn describe_config(c: Option<&Config>, out: &mut GooseBuf) {
    match c {
        Some(c) => {
            let _ = write!(out, "level {} verbose {}", c.level, c.verbose);
        }
        None => out.push_str("default config"),
    }
}

#[goose::export]
pub fn add_opt(a: i64, b: Option<i64>, c: Option<i64>) -> i64 {
    a + b.unwrap_or(100) + c.unwrap_or(1000)
}

#[goose::export]
pub fn greet(name: Option<&str>) -> String {
    format!("hello, {}", name.unwrap_or("stranger"))
}

// ---- callbacks ----

/// Calls back for each value; the callback says whether to keep it.
#[goose::export]
pub fn filter_sum(xs: &[i64], mut keep: impl FnMut(i64) -> bool) -> i64 {
    let mut s = 0;
    for &x in xs {
        if keep(x) {
            s += x;
        }
    }
    s
}

/// Calls back with text and a struct; the callback returns nothing.
#[goose::export]
pub fn visit_hits(hits: &[Hit], visit: &mut dyn FnMut(&str, &Hit)) -> i64 {
    for (i, h) in hits.iter().enumerate() {
        visit(&format!("hit #{i}"), h);
    }
    hits.len() as i64
}

/// A callback whose result is a struct, from a function over a handle.
#[goose::export]
pub fn map_stack(s: &Stack, f: impl Fn(i64) -> Hit) -> f64 {
    s.items.iter().map(|&v| f(v)).map(|h| h.t as f64).sum()
}

// ---- main thread ----

#[goose::export(main_thread)]
pub fn on_main() -> bool {
    goose::on_main_thread()
}

// ---- calling Goose back (goose-patches/0008) ----

goose::callbacks! {
    /// What a node is worth, as the Goose program says.
    fn on_node(depth: i64, label: &str) -> i64;
    /// A Goose export fn touching a var global: refused.
    fn tally(x: i64) -> i64;
    /// No such export fn.
    fn missing(x: i64) -> i64;
    /// Declared here otherwise than the program declares it.
    fn wrong(x: i64) -> i64;
    /// Moves a hit, in place.
    fn nudge(p: &mut Hit, by: f32);
}

/// Walks `depth` levels, asking the Goose program what each is worth.
#[goose::export]
pub fn walk(depth: i64, goose: &goose::Reentry) -> Result<i64, goose::ReentryError> {
    let mut total = 0;
    for d in 0..depth {
        total += on_node(goose, d, &format!("node {d}"))?;
    }
    Ok(total)
}

#[goose::export]
pub fn call_tally(x: i64, goose: &goose::Reentry) -> Result<i64, goose::ReentryError> {
    tally(goose, x)
}

#[goose::export]
pub fn call_missing(goose: &goose::Reentry) -> Result<i64, goose::ReentryError> {
    missing(goose, 1)
}

#[goose::export]
pub fn call_wrong(goose: &goose::Reentry) -> Result<i64, goose::ReentryError> {
    wrong(goose, 1)
}

/// Has the Goose program nudge each hit, in place.
#[goose::export]
pub fn nudge_all(hits: &mut [Hit], goose: &goose::Reentry) -> Result<(), goose::ReentryError> {
    for h in hits {
        nudge(goose, h, 0.5)?;
    }
    Ok(())
}

// ---- the bridge itself ----

/// Whether the last slice argument was lent in place (true) or copied.
#[goose::export]
pub fn lent_in_place(xs: &[i32]) -> bool {
    (xs.as_ptr() as usize) % std::mem::align_of::<i32>() == 0
}

/// Strict no-fallback exports used by the conformance tests.
pub mod zero_copy;
