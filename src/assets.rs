use crate::model::effects::Filter;
use crate::model::{AssetKind, Effect, Story, VariableValue};
use anyhow::{Context, Result, ensure};
use eframe::egui::{self, ColorImage, FontData, FontDefinitions, FontFamily, TextureHandle};
use std::collections::BTreeMap;

pub const UI_TEXT: &str = "跳过转场保存读档重新开始继续已结束请选择存档成功读档成功提示关闭台词超出对话框请拆句选项文字过长请缩短图片字体加载失败历史记录旁白设置补全取消确认覆盖空槽损坏导入旧档不含仅恢复当前位置文字速度整句每秒字素簇静音背景音乐语音音效设备不可用禁用输出正常正在播放暂停刚刚分钟前小时前天前尚未保存槽位音频返回";

pub struct PreparedAssets {
    images: BTreeMap<String, ColorImage>,
    filtered: BTreeMap<(String, Filter), ColorImage>,
    fonts: BTreeMap<String, Vec<u8>>,
}

impl PreparedAssets {
    pub fn validate_text(&self, text: &str) -> Result<()> {
        let faces: Vec<_> = self
            .fonts
            .values()
            .map(|bytes| ttf_parser::Face::parse(bytes, 0).expect("validated font"))
            .collect();
        for ch in text
            .chars()
            .filter(|ch| !ch.is_ascii() && !ch.is_whitespace())
        {
            ensure!(
                faces.iter().any(|face| face.glyph_index(ch).is_some()),
                "配置字体缺少 UI 字符：{ch}"
            );
        }
        Ok(())
    }
    /// Decode once at startup. This also powers the headless `--check` command.
    pub fn load(story: &Story) -> Result<Self> {
        Self::load_bytes(story, &|path| Ok(std::fs::read(path)?))
    }
    pub fn load_from(story: &Story, source: &dyn crate::content::ContentSource) -> Result<Self> {
        Self::load_bytes(story, &|path| source.read(path))
    }
    fn load_bytes(
        story: &Story,
        read: &dyn Fn(&std::path::Path) -> Result<Vec<u8>>,
    ) -> Result<Self> {
        let mut images = BTreeMap::new();
        let mut filtered = BTreeMap::new();
        let filters: std::collections::BTreeSet<_> = story
            .effects
            .values()
            .map(|e| e.filter)
            .filter(|f| *f != Filter::None)
            .collect();
        let mut filtered_bytes = 0usize;
        let mut fonts = BTreeMap::new();
        for (id, asset) in &story.assets {
            let context = || format!("资源 {id}：{}", asset.path.display());
            match asset.kind {
                AssetKind::Audio | AssetKind::Video | AssetKind::Live2D => {}
                AssetKind::Image => {
                    let bytes = read(&asset.path).with_context(context)?;
                    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
                        .with_guessed_format()
                        .with_context(context)?;
                    let mut limits = image::Limits::default();
                    limits.max_image_width = Some(8192);
                    limits.max_image_height = Some(8192);
                    reader.limits(limits);
                    let image = reader.decode().with_context(context)?.to_rgba8();
                    let size = [image.width() as usize, image.height() as usize];
                    for filter in &filters {
                        filtered_bytes = filtered_bytes
                            .checked_add(size[0] * size[1] * 4)
                            .context("滤镜缓存溢出")?;
                        ensure!(
                            filtered_bytes <= 256 * 1024 * 1024,
                            "滤镜图片缓存超过 256MiB，请减少图片/滤镜组合"
                        );
                        let variant = filter_image(&image, *filter);
                        filtered.insert(
                            (id.clone(), *filter),
                            ColorImage::from_rgba_unmultiplied(size, variant.as_raw()),
                        );
                    }
                    images.insert(
                        id.clone(),
                        ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
                    );
                }
                AssetKind::Font => {
                    let data = read(&asset.path).with_context(context)?;
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
        let mut text = format!("{UI_TEXT}当前状态没有选项是与否");
        for decl in story.variables.values() {
            text.push_str(&decl.label);
            if let VariableValue::String(value) = &decl.initial {
                text.push_str(value);
            }
        }
        for route in story.routes.values() {
            for rule in route.rules() {
                for effect in &rule.effects {
                    if let Effect::Set {
                        value: VariableValue::String(value),
                        ..
                    } = effect
                    {
                        text.push_str(value);
                    }
                }
            }
        }
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
        Ok(Self {
            images,
            filtered,
            fonts,
        })
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
        let filtered = self
            .filtered
            .into_iter()
            .map(|(key, image)| {
                let handle = ctx.load_texture(
                    format!("{}:{:?}", key.0, key.1),
                    image,
                    egui::TextureOptions::LINEAR,
                );
                (key, handle)
            })
            .collect();
        AssetCache { textures, filtered }
    }
}

pub struct AssetCache {
    textures: BTreeMap<String, TextureHandle>,
    filtered: BTreeMap<(String, Filter), TextureHandle>,
}

impl AssetCache {
    pub fn has_stream(&self, id: &str) -> bool {
        self.textures.contains_key(id)
    }
    pub fn update_stream(&mut self, ctx: &egui::Context, id: &str, image: ColorImage) {
        if let Some(texture) = self.textures.get_mut(id) {
            texture.set(image, egui::TextureOptions::LINEAR);
        } else {
            self.textures.insert(
                id.into(),
                ctx.load_texture(id, image, egui::TextureOptions::LINEAR),
            );
        }
    }
    pub fn effected(&self, id: &str, filter: Filter) -> Option<&TextureHandle> {
        if filter == Filter::None {
            self.image(id)
        } else {
            self.filtered.get(&(id.to_owned(), filter))
        }
    }
    pub fn image(&self, id: &str) -> Option<&TextureHandle> {
        self.textures.get(id)
    }
}

/// Startup-only CPU filter. Blur uses premultiplied RGB to avoid transparent color halos.
pub fn filter_image(source: &image::RgbaImage, filter: Filter) -> image::RgbaImage {
    use image::imageops;
    let mut result = source.clone();
    match filter {
        Filter::None => {}
        Filter::Grayscale | Filter::Sepia => {
            for p in result.pixels_mut() {
                let [r, g, b, a] = p.0;
                let (r, g, b) = (r as f32, g as f32, b as f32);
                let channels = if filter == Filter::Grayscale {
                    let v = 0.299 * r + 0.587 * g + 0.114 * b;
                    [v, v, v]
                } else {
                    [
                        0.393 * r + 0.769 * g + 0.189 * b,
                        0.349 * r + 0.686 * g + 0.168 * b,
                        0.272 * r + 0.534 * g + 0.131 * b,
                    ]
                };
                *p = image::Rgba([
                    channels[0].min(255.0) as u8,
                    channels[1].min(255.0) as u8,
                    channels[2].min(255.0) as u8,
                    a,
                ]);
            }
        }
        Filter::Pixelate(size) => {
            let size = u32::from(size).max(1);
            let small = imageops::resize(
                source,
                source.width().div_ceil(size).max(1),
                source.height().div_ceil(size).max(1),
                imageops::FilterType::Nearest,
            );
            result = imageops::resize(
                &small,
                source.width(),
                source.height(),
                imageops::FilterType::Nearest,
            );
        }
        Filter::Blur(radius) => {
            for p in result.pixels_mut() {
                for i in 0..3 {
                    p[i] = (u16::from(p[i]) * u16::from(p[3]) / 255) as u8;
                }
            }
            result = imageops::blur(&result, radius as f32);
            for p in result.pixels_mut() {
                if p[3] > 0 {
                    for i in 0..3 {
                        p[i] = (u16::from(p[i]) * 255 / u16::from(p[3])).min(255) as u8;
                    }
                } else {
                    p.0 = [0; 4];
                }
            }
        }
    }
    result
}
