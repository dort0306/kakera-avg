//! Bounded PNG metadata stored atomically inside the story save, never as a sidecar.
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{Local, TimeZone};
use eframe::egui::ColorImage;
use image::{ImageEncoder, RgbaImage};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

pub const SIZE: [u32; 2] = [320, 180];
const MAX_PNG: usize = 256 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Screenshot {
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub png_base64: String,
}
impl Screenshot {
    pub fn capture(image: ColorImage) -> Result<Self> {
        let [w, h] = image.size;
        ensure!(
            w > 0
                && h > 0
                && w <= 8192
                && h <= 8192
                && w * h <= 16 * 1024 * 1024
                && image.pixels.len() == w * h,
            "窗口截图尺寸或像素预算错误"
        );
        let data: Vec<u8> = image
            .pixels
            .iter()
            .flat_map(|p| p.to_srgba_unmultiplied())
            .collect();
        let source = RgbaImage::from_raw(w as u32, h as u32, data).context("窗口像素格式")?;
        let scale = (SIZE[0] as f64 / w as f64).min(SIZE[1] as f64 / h as f64);
        let scaled = image::imageops::thumbnail(
            &source,
            (w as f64 * scale).round().max(1.0) as u32,
            (h as f64 * scale).round().max(1.0) as u32,
        );
        let mut target = RgbaImage::from_pixel(SIZE[0], SIZE[1], image::Rgba([0, 0, 0, 255]));
        image::imageops::overlay(
            &mut target,
            &scaled,
            i64::from((SIZE[0] - scaled.width()) / 2),
            i64::from((SIZE[1] - scaled.height()) / 2),
        );
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png).write_image(
            target.as_raw(),
            SIZE[0],
            SIZE[1],
            image::ExtendedColorType::Rgba8,
        )?;
        ensure!(png.len() <= MAX_PNG, "截图 PNG 超过 256KiB");
        Ok(Self {
            format: "png".into(),
            width: SIZE[0],
            height: SIZE[1],
            png_base64: STANDARD.encode(png),
        })
    }
    pub fn decode(&self) -> Result<RgbaImage> {
        ensure!(
            self.format == "png" && [self.width, self.height] == SIZE,
            "存档截图格式/尺寸错误"
        );
        ensure!(
            self.png_base64.len() <= MAX_PNG.div_ceil(3) * 4,
            "截图 Base64 超过预算"
        );
        let bytes = STANDARD
            .decode(&self.png_base64)
            .context("截图 Base64 无效")?;
        ensure!(bytes.len() <= MAX_PNG, "截图 PNG 超过预算");
        validate_chunks(&bytes)?;
        let mut reader =
            image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(SIZE[0]);
        limits.max_image_height = Some(SIZE[1]);
        limits.max_alloc = Some(1024 * 1024);
        reader.limits(limits);
        let image = reader.decode().context("存档截图 PNG 无效")?.to_rgba8();
        ensure!(
            [image.width(), image.height()] == SIZE,
            "截图 PNG 实际尺寸与声明不一致"
        );
        Ok(image)
    }
}
fn validate_chunks(bytes: &[u8]) -> Result<()> {
    ensure!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "截图不是 PNG");
    let mut cursor = 8usize;
    let mut header = false;
    let mut data = false;
    while cursor < bytes.len() {
        ensure!(bytes.len() - cursor >= 12, "PNG 块截断");
        let length = u32::from_be_bytes(bytes[cursor..cursor + 4].try_into()?) as usize;
        let end = cursor
            .checked_add(12)
            .and_then(|v| v.checked_add(length))
            .context("PNG 块长度溢出")?;
        ensure!(end <= bytes.len(), "PNG 块越界");
        match &bytes[cursor + 4..cursor + 8] {
            b"IHDR" => {
                ensure!(!header && cursor == 8 && length == 13, "PNG 头错误");
                header = true;
            }
            b"IDAT" => {
                ensure!(header, "PNG 数据前没有头");
                data = true;
            }
            b"IEND" => {
                ensure!(
                    header && data && length == 0 && end == bytes.len(),
                    "PNG 结束或尾随数据错误"
                );
                return Ok(());
            }
            _ => anyhow::bail!("截图 PNG 不接受额外元数据块"),
        }
        cursor = end;
    }
    anyhow::bail!("PNG 缺少结束块")
}
pub fn saved_at(milliseconds: u64) -> String {
    i64::try_from(milliseconds)
        .ok()
        .and_then(|value| Local.timestamp_millis_opt(value).single())
        .map(|date| date.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "无效时间".into())
}
pub fn image_id(slot: u16, automatic: bool) -> String {
    format!("$save:{}:{slot}", if automatic { "auto" } else { "manual" })
}
pub fn excerpt(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut result: String = line.chars().take(64).collect();
    if line.chars().count() > 64 {
        result.push('…');
    }
    result
}
