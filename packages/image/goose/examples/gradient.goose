// The image crate from Goose: draws a gradient in Goose, hands it to Rust
// in one slice, resizes and encodes it there. goose run gradient.goose
import img from "..";

fn main() {
    let w: u32 = 64;
    let h: u32 = 32;
    let im = img::new(w, h, img::Rgba { r: 0, g: 0, b: 0, a: 255 });
    var px: img::Rgba[>..] = [];
    for y in h as i64 {
        for x in w as i64 {
            px.push(img::Rgba { r: (x * 4) as u8, g: (y * 8) as u8, b: 128, a: 255 });
        }
    }
    print("wrote ", img::write_pixels(im, px), " pixels");
    let p, ok = img::pixel(im, 10, 5);
    print("pixel (10, 5): ", p.r, " ", p.g, " ", p.b);
    let small = img::resize(im, 16, 8, img::Filter.Triangle);
    print("resized to ", img::width(small), "x", img::height(small));
    let avg = img::average(small);
    print("average blue ", avg.b);
    var png: u8[>..] = [];
    let encoded = img::encode_png(small, png);
    print("png: ", encoded, ", starts with the PNG signature: ", png[1] == 'P' && png[2] == 'N' && png[3] == 'G');
    let back, dok = img::decode(png);
    print("decoded ", img::width(back), "x", img::height(back));
    img::release(back);
    img::release(small);
    img::release(im);
    print("live: ", img::live_count());
}
