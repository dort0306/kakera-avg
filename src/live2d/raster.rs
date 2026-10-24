//! Bounded, premultiplied software compositor. All source textures are decoded once.
use super::core::{Drawable, V2};
use anyhow::{Result, ensure};
use image::RgbaImage;
use std::collections::BTreeMap;
const SIDE: usize = 512;
fn edge(a: V2, b: V2, p: V2) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}
fn top_left(a: V2, b: V2) -> bool {
    b.y < a.y || (b.y == a.y && b.x > a.x)
}
fn sample(texture: &RgbaImage, uv: V2) -> [f32; 4] {
    let x = (uv.x.clamp(0.0, 1.0) * (texture.width() - 1) as f32).round() as u32;
    let y = ((1.0 - uv.y).clamp(0.0, 1.0) * (texture.height() - 1) as f32).round() as u32;
    texture.get_pixel(x, y).0.map(|c| c as f32 / 255.0)
}
fn pixels(
    mesh: &Drawable,
    texture: &RgbaImage,
    size: [usize; 2],
    budget: &mut usize,
    mut f: impl FnMut(usize, [f32; 4]),
) -> Result<()> {
    for idx in mesh.indices.as_chunks::<3>().0 {
        let mut i = [idx[0] as usize, idx[1] as usize, idx[2] as usize];
        let a = mesh.vertices[i[0]];
        let mut b = mesh.vertices[i[1]];
        let mut c = mesh.vertices[i[2]];
        let area = edge(a, b, c);
        if area.abs() < 0.0001 {
            continue;
        }
        if mesh.flags & 4 == 0 && area > 0.0 {
            continue;
        }
        if area < 0.0 {
            i.swap(1, 2);
            std::mem::swap(&mut b, &mut c);
        }
        let area = area.abs();
        let left = a.x.min(b.x).min(c.x).floor().max(0.0) as usize;
        let right = a.x.max(b.x).max(c.x).ceil().clamp(0.0, size[0] as f32) as usize;
        let top = a.y.min(b.y).min(c.y).floor().max(0.0) as usize;
        let bottom = a.y.max(b.y).max(c.y).ceil().clamp(0.0, size[1] as f32) as usize;
        for y in top..bottom {
            for x in left..right {
                *budget += 1;
                ensure!(*budget <= 32_000_000, "Live2D 单帧光栅预算超过 3200 万采样");
                let p = V2 {
                    x: x as f32 + 0.5,
                    y: y as f32 + 0.5,
                };
                let e = [edge(b, c, p), edge(c, a, p), edge(a, b, p)];
                let included = [(b, c), (c, a), (a, b)]
                    .iter()
                    .enumerate()
                    .all(|(j, (a, b))| e[j] > 0.0 || (e[j] == 0.0 && top_left(*a, *b)));
                if !included {
                    continue;
                }
                let w = e.map(|v| v / area);
                let uv = V2 {
                    x: mesh.uvs[i[0]].x * w[0] + mesh.uvs[i[1]].x * w[1] + mesh.uvs[i[2]].x * w[2],
                    y: mesh.uvs[i[0]].y * w[0] + mesh.uvs[i[1]].y * w[1] + mesh.uvs[i[2]].y * w[2],
                };
                f(y * size[0] + x, sample(texture, uv));
            }
        }
    }
    Ok(())
}
pub fn composite(
    mut meshes: Vec<Drawable>,
    textures: &[RgbaImage],
    canvas: [f32; 2],
    origin: [f32; 2],
    ppu: f32,
    opacity: f32,
) -> Result<eframe::egui::ColorImage> {
    let scale = SIDE as f32 / canvas[0].max(canvas[1]);
    let size = [
        (canvas[0] * scale).round().max(1.0) as usize,
        (canvas[1] * scale).round().max(1.0) as usize,
    ];
    for mesh in &mut meshes {
        for p in &mut mesh.vertices {
            p.x = (p.x * ppu + origin[0]) / canvas[0] * size[0] as f32;
            p.y = (1.0 - (p.y * ppu + origin[1]) / canvas[1]) * size[1] as f32;
        }
    }
    let mut masks: BTreeMap<Vec<usize>, Vec<f32>> = BTreeMap::new();
    let mut budget = 0;
    for mesh in &meshes {
        if mesh.masks.is_empty() || masks.contains_key(&mesh.masks) {
            continue;
        }
        ensure!(masks.len() < 32, "Live2D 裁剪组超过 32 项");
        let mut mask = vec![0.0; size[0] * size[1]];
        for index in &mesh.masks {
            let source = &meshes[*index];
            pixels(
                source,
                &textures[source.texture],
                size,
                &mut budget,
                |i, c| {
                    // Cubism clipping uses source texture alpha even when that drawable is hidden.
                    let a = c[3];
                    mask[i] = a + mask[i] * (1.0 - a);
                },
            )?;
        }
        masks.insert(mesh.masks.clone(), mask);
    }
    let mut output = vec![[0.0f32; 4]; size[0] * size[1]];
    let mut order: Vec<_> = (0..meshes.len()).collect();
    order.sort_by_key(|i| meshes[*i].order);
    for i in order {
        let mesh = &meshes[i];
        if !mesh.visible || mesh.opacity <= 0.0 {
            continue;
        }
        let mask = masks.get(&mesh.masks);
        pixels(mesh, &textures[mesh.texture], size, &mut budget, |i, c| {
            let coverage = mask
                .map(|v| {
                    if mesh.flags & 8 != 0 {
                        1.0 - v[i]
                    } else {
                        v[i]
                    }
                })
                .unwrap_or(1.0);
            let a = (c[3] * mesh.opacity * coverage * opacity).clamp(0.0, 1.0);
            let mult = [mesh.multiply.r, mesh.multiply.g, mesh.multiply.b];
            let screen = [mesh.screen.r, mesh.screen.g, mesh.screen.b];
            let rgb = std::array::from_fn::<_, 3, _>(|j| {
                (c[j] * mult[j] + screen[j] * (1.0 - c[j] * mult[j])).clamp(0.0, 1.0) * a
            });
            let dst = &mut output[i];
            for j in 0..3 {
                dst[j] = if mesh.flags & 1 != 0 {
                    dst[j] + rgb[j]
                } else if mesh.flags & 2 != 0 {
                    dst[j] * (rgb[j] + 1.0 - a)
                } else {
                    rgb[j] + dst[j] * (1.0 - a)
                };
            }
            if mesh.flags & 3 == 0 {
                dst[3] = a + dst[3] * (1.0 - a);
            }
        })?;
    }
    let bytes: Vec<u8> = output
        .into_iter()
        .flat_map(|p| p.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
        .collect();
    Ok(eframe::egui::ColorImage::from_rgba_premultiplied(
        size, &bytes,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live2d::core::V4;
    fn quad(texture: usize) -> Drawable {
        Drawable {
            vertices: vec![
                V2 { x: -1.0, y: -1.0 },
                V2 { x: 1.0, y: -1.0 },
                V2 { x: 1.0, y: 1.0 },
                V2 { x: -1.0, y: 1.0 },
            ],
            uvs: vec![
                V2 { x: 0.0, y: 0.0 },
                V2 { x: 1.0, y: 0.0 },
                V2 { x: 1.0, y: 1.0 },
                V2 { x: 0.0, y: 1.0 },
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            masks: Vec::new(),
            texture,
            flags: 0,
            visible: true,
            opacity: 1.0,
            order: 1,
            multiply: V4 {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
            screen: V4 {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            },
        }
    }
    #[test]
    fn invisible_mask_uses_texture_alpha_and_inversion_clips_instead_of_revealing() {
        let textures = vec![
            image::RgbaImage::from_pixel(2, 2, image::Rgba([255; 4])),
            image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255])),
        ];
        let mut mask = quad(0);
        mask.visible = false;
        mask.opacity = 0.0;
        let mut target = quad(1);
        target.masks = vec![0];
        let image = composite(
            vec![mask.clone(), target.clone()],
            &textures,
            [2.0, 2.0],
            [1.0, 1.0],
            1.0,
            1.0,
        )
        .unwrap();
        assert_eq!(image.pixels[256 * 512 + 256], eframe::egui::Color32::RED);
        target.flags |= 8;
        let image = composite(
            vec![mask, target],
            &textures,
            [2.0, 2.0],
            [1.0, 1.0],
            1.0,
            1.0,
        )
        .unwrap();
        assert!(image.pixels.iter().all(|p| p.a() == 0));
    }
}
