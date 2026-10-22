use crate::model::{AssetKind, Story};
use anyhow::{Context, Result, ensure};
use eframe::egui::{self, ColorImage, FontData, FontDefinitions, FontFamily, TextureHandle};
use std::collections::BTreeMap;

pub const UI_TEXT: &str = "保存读档重新开始继续已结束请选择存档成功读档成功提示关闭台词超出对话框请拆句选项文字过长请缩短图片字体加载失败";

pub struct PreparedAssets {
    images: BTreeMap<String, ColorImage>,
    fonts: BTreeMap<String, Vec<u8>>,
}

impl PreparedAssets {
    /// Decode once at startup. This also powers the headless `--check` command.
    pub fn load(story: &Story) -> Result<Self> {
        let mut images = BTreeMap::new();
        let mut fonts = BTreeMap::new();
        for (id, asset) in &story.assets {
            let context = || format!("资源 {id}：{}", asset.path.display());
            match asset.kind {
                AssetKind::Image => {
                    let mut reader = image::ImageReader::open(&asset.path).with_context(context)?;
                    let mut limits = image::Limits::default();
                    limits.max_image_width = Some(8192);
                    limits.max_image_height = Some(8192);
                    reader.limits(limits);
                    let image = reader.decode().with_context(context)?.to_rgba8();
                    let size = [image.width() as usize, image.height() as usize];
                    images.insert(
                        id.clone(),
                        ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
                    );
                }
                AssetKind::Font => {
                    let data = std::fs::read(&asset.path).with_context(context)?;
                    ttf_parser::Face::parse(&data, 0)
                        .map_err(|e| anyhow::anyhow!("{}：字体无效：{e:?}", context()))?;
                    fonts.insert(id.clone(), data);
                }
            }
        }
        let faces: Vec<_> = fonts
            .values()
            .map(|data| ttf_parser::Face::parse(data, 0).expect("validated font"))
            .collect();
        let mut text = UI_TEXT.to_owned();
        for frame in story.frames.values() {
            text.push_str(&frame.text);
            for option in &frame.options {
                text.push_str(&option.text);
            }
        }
        for name in story.characters.values() {
            text.push_str(name);
        }
        for ch in text
            .chars()
            .filter(|ch| !ch.is_ascii() && !ch.is_whitespace())
        {
            ensure!(
                faces.iter().any(|face| face.glyph_index(ch).is_some()),
                "配置的字体缺少字符：{ch}"
            );
        }
        Ok(Self { images, fonts })
    }

    pub fn install(self, ctx: &egui::Context) -> AssetCache {
        let mut definitions = FontDefinitions::default();
        for (id, data) in self.fonts {
            definitions
                .font_data
                .insert(id.clone(), FontData::from_owned(data).into());
            definitions
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .push(id);
        }
        ctx.set_fonts(definitions);
        let textures = self
            .images
            .into_iter()
            .map(|(id, image)| {
                let handle = ctx.load_texture(&id, image, egui::TextureOptions::LINEAR);
                (id, handle)
            })
            .collect();
        AssetCache { textures }
    }
}

pub struct AssetCache {
    textures: BTreeMap<String, TextureHandle>,
}

impl AssetCache {
    pub fn image(&self, id: &str) -> Option<&TextureHandle> {
        self.textures.get(id)
    }
}
