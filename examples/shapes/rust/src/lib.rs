//! A small geometry library for Goose, showing the Phase 3 shapes: an enum
//! with data, a generic function instantiated per type, an iterator over
//! parts of an argument, a lazy iterator, an optional parameter and a
//! callback.

/// A shape, converted at the boundary (17 bytes in Goose, 24 in Rust).
#[derive(goose::Flat, Clone, Copy, Debug, PartialEq)]
#[repr(C, u8)]
pub enum Shape {
    Circle { r: f64 },
    Rect { w: f64, h: f64 },
    Dot,
}

/// A point.
#[derive(goose::Flat, Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// How to draw.
#[derive(goose::Flat, Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Style {
    pub scale: f64,
    pub outline: bool,
}

fn area_of(s: &Shape) -> f64 {
    match s {
        Shape::Circle { r } => std::f64::consts::PI * r * r,
        Shape::Rect { w, h } => w * h,
        Shape::Dot => 0.0,
    }
}

/// The total area of the shapes.
#[goose::export]
pub fn total_area(shapes: &[Shape]) -> f64 {
    shapes.iter().map(area_of).sum()
}

/// The largest shape, if there is one.
#[goose::export]
pub fn largest(shapes: &[Shape]) -> Option<Shape> {
    shapes.iter().copied().max_by(|a, b| area_of(a).total_cmp(&area_of(b)))
}

/// The centroid of points, one version per number type Goose may call.
#[goose::export(instantiate(T = f32, f64))]
pub fn mean<T>(xs: &[T]) -> T
where
    T: Copy + Default + std::ops::Add<Output = T> + std::ops::Div<Output = T> + From<u8>,
{
    let mut sum = T::default();
    let mut n = T::default();
    for &x in xs {
        sum = sum + x;
        n = n + T::from(1u8);
    }
    if xs.is_empty() {
        T::default()
    } else {
        sum / n
    }
}

/// The comma-separated fields of a line, each a view of it.
#[goose::export]
pub fn fields(line: &str) -> impl Iterator<Item = &str> + '_ {
    line.split(',').map(str::trim)
}

/// Points on a circle, made one at a time.
#[goose::export]
pub fn circle_points(n: i64, r: f64) -> impl Iterator<Item = Point> {
    (0..n.max(0)).map(move |i| {
        let a = i as f64 / n as f64 * std::f64::consts::TAU;
        Point { x: r * a.cos(), y: r * a.sin() }
    })
}

/// A description of the shape, in a style or the default one.
#[goose::export]
pub fn describe(s: &Shape, style: Option<&Style>) -> String {
    let scale = style.map(|st| st.scale).unwrap_or(1.0);
    let outline = style.map(|st| st.outline).unwrap_or(false);
    let what = match s {
        Shape::Circle { r } => format!("circle of radius {}", r * scale),
        Shape::Rect { w, h } => format!("{} by {} rectangle", w * scale, h * scale),
        Shape::Dot => "dot".to_string(),
    };
    if outline {
        format!("{what}, outlined")
    } else {
        what
    }
}

/// Walks the shapes, asking the caller's block which to keep; the kept
/// ones' area.
#[goose::export]
pub fn keep_area(shapes: &[Shape], mut keep: impl FnMut(&Shape) -> bool) -> f64 {
    shapes.iter().filter(|s| keep(s)).map(area_of).sum()
}
