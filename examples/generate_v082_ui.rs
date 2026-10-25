//! Exact geometric source PNGs: marker borders and atlas cells are reproducible.
use image::{Rgba, RgbaImage};
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/ui");
    let mut patch = RgbaImage::new(66, 34);
    for y in 1..33 {
        for x in 1..65 {
            patch[(x, y)] = Rgba(if x <= 2 || x >= 63 || y <= 2 || y >= 31 {
                [111, 145, 164, 245]
            } else {
                [12, 21, 35, 245]
            });
        }
    }
    for x in 5..61 {
        patch[(x, 0)] = Rgba([0, 0, 0, 255]);
    }
    for y in 5..29 {
        patch[(0, y)] = Rgba([0, 0, 0, 255]);
    }
    // Full content extent keeps the existing dialogue's explicit child offsets.
    for x in 1..65 {
        patch[(x, 33)] = Rgba([0, 0, 0, 255]);
    }
    for y in 1..33 {
        patch[(65, y)] = Rgba([0, 0, 0, 255]);
    }
    patch.save(root.join("dialogue.9.png"))?;
    let mut atlas = RgbaImage::new(192, 64);
    for (x, y, pixel) in atlas.enumerate_pixels_mut() {
        let cell = x / 64;
        let x = x % 64;
        let border = x < 3 || y < 3 || x >= 61 || y >= 61;
        let arrow = ((16..=44).contains(&x) && y.abs_diff(32) <= 5)
            || ((32..=46).contains(&x) && y.abs_diff(32) <= 46 - x);
        *pixel = Rgba(if border {
            [240, 202, 137, 255]
        } else if arrow {
            [245, 245, 245, 255]
        } else {
            match cell {
                0 => [45, 85, 135, 255],
                1 => [40, 145, 105, 255],
                _ => [175, 75, 85, 255],
            }
        });
    }
    atlas.save(root.join("sprites.png"))?;
    Ok(())
}
