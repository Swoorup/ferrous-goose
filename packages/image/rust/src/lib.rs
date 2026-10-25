//! The image crate for Goose: `import img from "github.com/goose-rs/image@v1";`.
//!
//! An image is an RGBA8 buffer on the Rust side behind an `Image` handle.
//! Pixels cross as `Rgba` values, and whole rows or images as slices of
//! them, borrowed in place where Goose's storage allows.

use goose::GooseBuf;
use image::{imageops, ImageFormat, RgbaImage};

/// An RGBA image. Release it with `img::release` when done.
#[goose::handle]
pub struct Image(RgbaImage);

/// One pixel.
#[derive(goose::Flat, Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// How resizing samples.
#[derive(goose::Flat, Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum Filter {
    Nearest,
    Triangle,
    CatmullRom,
    Gaussian,
    Lanczos3,
}

fn filter(f: Filter) -> imageops::FilterType {
    match f {
        Filter::Nearest => imageops::FilterType::Nearest,
        Filter::Triangle => imageops::FilterType::Triangle,
        Filter::CatmullRom => imageops::FilterType::CatmullRom,
        Filter::Gaussian => imageops::FilterType::Gaussian,
        Filter::Lanczos3 => imageops::FilterType::Lanczos3,
    }
}

/// A new image of the size, filled with the colour.
#[goose::export]
pub fn new(width: u32, height: u32, fill: Rgba) -> Image {
    Image(RgbaImage::from_pixel(width, height, image::Rgba([fill.r, fill.g, fill.b, fill.a])))
}

/// Loads a PNG or JPEG file.
#[goose::export]
pub fn load(path: &str) -> Result<Image, image::ImageError> {
    Ok(Image(image::open(path)?.to_rgba8()))
}

/// Decodes PNG or JPEG bytes.
#[goose::export]
pub fn decode(bytes: &[u8]) -> Result<Image, image::ImageError> {
    Ok(Image(image::load_from_memory(bytes)?.to_rgba8()))
}

/// Saves the image; the format follows the file's extension.
#[goose::export]
pub fn save(im: &Image, path: &str) -> Result<(), image::ImageError> {
    im.0.save(path)
}

/// The image as PNG bytes.
#[goose::export]
pub fn encode_png(im: &Image, out: &mut GooseBuf) -> Result<(), image::ImageError> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    im.0.write_to(&mut bytes, ImageFormat::Png)?;
    out.extend_from_slice(bytes.get_ref());
    Ok(())
}

#[goose::export]
pub fn width(im: &Image) -> u32 {
    im.0.width()
}

#[goose::export]
pub fn height(im: &Image) -> u32 {
    im.0.height()
}

/// The pixel at (x, y), if it is inside the image.
#[goose::export]
pub fn pixel(im: &Image, x: u32, y: u32) -> Option<Rgba> {
    im.0.get_pixel_checked(x, y).map(|p| Rgba { r: p[0], g: p[1], b: p[2], a: p[3] })
}

/// Sets the pixel at (x, y); false outside the image.
#[goose::export]
pub fn set_pixel(im: &mut Image, x: u32, y: u32, c: Rgba) -> bool {
    match im.0.get_pixel_mut_checked(x, y) {
        Some(p) => {
            *p = image::Rgba([c.r, c.g, c.b, c.a]);
            true
        }
        None => false,
    }
}

/// Copies the image's pixels, row by row, into `out` (which needs
/// width * height elements); how many it copied.
#[goose::export]
pub fn read_pixels(im: &Image, out: &mut [Rgba]) -> i64 {
    let mut n = 0;
    for (dst, p) in out.iter_mut().zip(im.0.pixels()) {
        *dst = Rgba { r: p[0], g: p[1], b: p[2], a: p[3] };
        n += 1;
    }
    n
}

/// Writes pixels into the image, row by row; how many it wrote.
#[goose::export]
pub fn write_pixels(im: &mut Image, pixels: &[Rgba]) -> i64 {
    let mut n = 0;
    for (p, src) in im.0.pixels_mut().zip(pixels) {
        *p = image::Rgba([src.r, src.g, src.b, src.a]);
        n += 1;
    }
    n
}

/// A resized copy.
#[goose::export]
pub fn resize(im: &Image, width: u32, height: u32, f: Filter) -> Image {
    Image(imageops::resize(&im.0, width, height, filter(f)))
}

/// A grey copy.
#[goose::export]
pub fn grayscale(im: &Image) -> Image {
    Image(image::DynamicImage::ImageRgba8(im.0.clone()).grayscale().to_rgba8())
}

/// A blurred copy.
#[goose::export]
pub fn blur(im: &Image, sigma: f32) -> Image {
    Image(imageops::blur(&im.0, sigma))
}

/// Flips the image left to right, in place.
#[goose::export]
pub fn flip_horizontal(im: &mut Image) {
    imageops::flip_horizontal_in_place(&mut im.0);
}

/// The mean colour.
#[goose::export]
pub fn average(im: &Image) -> Rgba {
    let (mut r, mut g, mut b, mut a, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
    for p in im.0.pixels() {
        r += p[0] as u64;
        g += p[1] as u64;
        b += p[2] as u64;
        a += p[3] as u64;
        n += 1;
    }
    let n = n.max(1);
    Rgba { r: (r / n) as u8, g: (g / n) as u8, b: (b / n) as u8, a: (a / n) as u8 }
}
