//! Prepared image metadata and pure source/destination geometry, independent of the GPU.
use anyhow::{Result, ensure};
use image::RgbaImage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region(pub [u32; 4]);
impl Region {
    pub fn parse(text: &str) -> Result<Self> {
        ensure!(
            text.len() <= 128,
            "{}",
            crate::i18n::format("host-image-layout-0001", &[])
        );
        let values = text
            .split(',')
            .map(|v| {
                let v = v.trim();
                ensure!(
                    !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()),
                    "{}",
                    crate::i18n::format("host-image-layout-0002", &[])
                );
                Ok(v.parse::<u32>()?)
            })
            .collect::<Result<Vec<_>>>()?;
        let rect: [u32; 4] = values.try_into().map_err(|_| {
            anyhow::anyhow!("{}", crate::i18n::format("host-image-layout-0003", &[]))
        })?;
        ensure!(
            rect[2] > 0
                && rect[3] > 0
                && rect[0].checked_add(rect[2]).is_some()
                && rect[1].checked_add(rect[3]).is_some(),
            "{}",
            crate::i18n::format("host-image-layout-0004", &[])
        );
        Ok(Self(rect))
    }
    pub fn validate(self, size: [u32; 2]) -> Result<()> {
        let [x, y, w, h] = self.0;
        ensure!(
            x.checked_add(w).is_some_and(|r| r <= size[0])
                && y.checked_add(h).is_some_and(|b| b <= size[1])
                && w > 0
                && h > 0,
            "{}",
            crate::i18n::format(
                "host-image-layout-0005",
                &[
                    ("arg0", format!("{}", size[0])),
                    ("arg1", format!("{}", size[1]))
                ]
            )
        );
        Ok(())
    }
    pub fn uv(self, size: [f32; 2]) -> [f32; 4] {
        let [x, y, w, h] = self.0.map(|n| n as f32);
        // Inset to texel centres, so linear interpolation cannot sample the next sprite.
        [
            (x + 0.5) / size[0],
            (y + 0.5) / size[1],
            (x + w - 0.5) / size[0],
            (y + h - 0.5) / size[1],
        ]
    }
}

#[derive(Clone, Debug)]
pub struct NinePatch {
    pub size: [u32; 2],
    pub horizontal: Vec<[u32; 2]>,
    pub vertical: Vec<[u32; 2]>,
    pub content: [u32; 4],
}
#[derive(Clone, Debug)]
pub struct Patch {
    pub source: [f32; 4],
    pub target: [f32; 4],
}
struct Span {
    source: [f32; 2],
    target: [f32; 2],
}
fn black(pixel: [u8; 4]) -> Result<bool> {
    ensure!(
        pixel == [0, 0, 0, 255] || pixel == [255; 4] || pixel[3] == 0,
        "{}",
        crate::i18n::format("host-image-layout-0006", &[])
    );
    Ok(pixel == [0, 0, 0, 255])
}
fn runs(pixels: impl Iterator<Item = [u8; 4]>) -> Result<Vec<[u32; 2]>> {
    let mut result = Vec::new();
    let mut start = None;
    let mut end = 0;
    for (i, pixel) in pixels.enumerate() {
        let i = i as u32;
        end = i + 1;
        if black(pixel)? {
            start.get_or_insert(i);
        } else if let Some(start) = start.take() {
            result.push([start, i]);
        }
    }
    if let Some(start) = start {
        result.push([start, end]);
    }
    Ok(result)
}
impl NinePatch {
    pub fn parse(image: RgbaImage) -> Result<(Self, RgbaImage)> {
        let (w, h) = image.dimensions();
        ensure!(
            w >= 3 && h >= 3,
            "{}",
            crate::i18n::format("host-image-layout-0007", &[])
        );
        for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
            ensure!(
                !black(image[(x, y)].0)?,
                "{}",
                crate::i18n::format("host-image-layout-0008", &[])
            );
        }
        let horizontal = runs((1..w - 1).map(|x| image[(x, 0)].0))?;
        let vertical = runs((1..h - 1).map(|y| image[(0, y)].0))?;
        ensure!(
            !horizontal.is_empty()
                && !vertical.is_empty()
                && horizontal.len() <= 8
                && vertical.len() <= 8,
            "{}",
            crate::i18n::format("host-image-layout-0009", &[])
        );
        let bottom = runs((1..w - 1).map(|x| image[(x, h - 1)].0))?;
        let right = runs((1..h - 1).map(|y| image[(w - 1, y)].0))?;
        ensure!(
            bottom.len() <= 1 && right.len() <= 1,
            "{}",
            crate::i18n::format("host-image-layout-0010", &[])
        );
        let x = bottom
            .first()
            .copied()
            .unwrap_or([horizontal[0][0], horizontal.last().unwrap()[1]]);
        let y = right
            .first()
            .copied()
            .unwrap_or([vertical[0][0], vertical.last().unwrap()[1]]);
        let patch = Self {
            size: [w - 2, h - 2],
            horizontal,
            vertical,
            content: [x[0], y[0], x[1], y[1]],
        };
        Ok((
            patch,
            image::imageops::crop_imm(&image, 1, 1, w - 2, h - 2).to_image(),
        ))
    }
    pub fn layout(&self, size: [f32; 2], scale: f32) -> (Vec<Patch>, [f32; 4]) {
        let x = axis(self.size[0], &self.horizontal, size[0], scale);
        let y = axis(self.size[1], &self.vertical, size[1], scale);
        let mut patches = Vec::new();
        for row in &y {
            for col in &x {
                if col.target[1] > col.target[0] && row.target[1] > row.target[0] {
                    patches.push(Patch {
                        source: [col.source[0], row.source[0], col.source[1], row.source[1]],
                        target: [col.target[0], row.target[0], col.target[1], row.target[1]],
                    });
                }
            }
        }
        let [l, t, r, b] = self.content;
        (patches, [map(&x, l), map(&y, t), map(&x, r), map(&y, b)])
    }
}
fn axis(length: u32, stretches: &[[u32; 2]], size: f32, scale: f32) -> Vec<Span> {
    let total: f32 = stretches.iter().map(|s| (s[1] - s[0]) as f32).sum();
    let fixed = (length as f32 - total) * scale;
    let shrink = if fixed > size { size / fixed } else { 1.0 };
    let mut cuts = vec![0, length];
    for span in stretches {
        cuts.extend(span);
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut target = 0.0;
    cuts.windows(2)
        .map(|span| {
            let source = [span[0] as f32, span[1] as f32];
            let flexible = stretches.iter().any(|s| span[0] >= s[0] && span[1] <= s[1]);
            let width = if flexible {
                (size - fixed).max(0.0) * (source[1] - source[0]) / total
            } else {
                (source[1] - source[0]) * scale * shrink
            };
            let result = Span {
                source,
                target: [target, target + width],
            };
            target += width;
            result
        })
        .collect()
}
fn map(spans: &[Span], value: u32) -> f32 {
    let value = value as f32;
    for span in spans {
        if value <= span.source[1] {
            return span.target[0]
                + (value - span.source[0]) / (span.source[1] - span.source[0])
                    * (span.target[1] - span.target[0]);
        }
    }
    spans.last().map_or(0.0, |s| s.target[1])
}
