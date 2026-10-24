//! Reproducible geometric demo skins; authors can replace these PNG files.
use image::{Rgba, RgbaImage};
use std::path::Path;
fn skin(path: &Path, size: [u32; 2], base: [u8; 4], edge: [u8; 4]) -> image::ImageResult<()> {
    let mut image = RgbaImage::new(size[0], size[1]);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let border = x < 2 || y < 2 || x + 2 >= size[0] || y + 2 >= size[1];
        let mut value = if border { edge } else { base };
        if !border {
            for channel in value.iter_mut().take(3) {
                *channel = channel.saturating_add(((size[1] - y) * 8 / size[1]) as u8);
            }
        }
        *pixel = Rgba(value);
    }
    image.save(path)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/ui");
    std::fs::create_dir_all(&root)?;
    for (name, size, base, edge) in [
        (
            "dialogue.png",
            [1208, 180],
            [10, 19, 33, 240],
            [111, 145, 164, 240],
        ),
        (
            "panel.png",
            [1060, 580],
            [12, 21, 35, 250],
            [142, 166, 181, 255],
        ),
        (
            "button.png",
            [256, 64],
            [37, 58, 79, 255],
            [118, 151, 172, 255],
        ),
        (
            "button-hover.png",
            [256, 64],
            [62, 91, 112, 255],
            [240, 202, 137, 255],
        ),
        (
            "button-disabled.png",
            [256, 64],
            [32, 39, 50, 255],
            [69, 78, 88, 255],
        ),
    ] {
        skin(&root.join(name), size, base, edge)?;
    }
    Ok(())
}
