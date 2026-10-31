use crate::model::effects::Filter;
use crate::model::{AssetKind, Story, VariableValue};
use anyhow::{Context, Result, ensure};
use eframe::egui::{self, ColorImage, FontData, FontDefinitions, FontFamily, TextureHandle};
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct PreparedAssets {
    patches: BTreeMap<String, crate::image_layout::NinePatch>,
    images: BTreeMap<String, image::RgbaImage>,
    deferred: std::collections::BTreeSet<Filter>,
    filtered: BTreeMap<(String, Filter), ColorImage>,
    fonts: BTreeMap<String, Vec<u8>>,
}

impl PreparedAssets {
    pub fn set_font_order(&self, ctx: &egui::Context, preferred: &[String]) {
        ctx.set_fonts(self.font_definitions(preferred));
    }
    pub fn font_definitions(&self, preferred: &[String]) -> FontDefinitions {
        let mut definitions = FontDefinitions::default();
        let mut order = preferred.to_vec();
        for id in self.fonts.keys() {
            if !order.contains(id) {
                order.push(id.clone());
            }
        }
        for id in order.iter().rev() {
            if let Some(bytes) = self.fonts.get(id) {
                definitions
                    .font_data
                    .insert(id.clone(), FontData::from_owned(bytes.clone()).into());
                definitions
                    .families
                    .entry(FontFamily::Proportional)
                    .or_default()
                    .insert(0, id.clone());
            }
        }
        definitions
    }
    pub fn validate_locale(&self, fonts: &[String], text: &str) -> Result<()> {
        let faces = fonts
            .iter()
            .map(|id| {
                self.fonts
                    .get(id)
                    .context(crate::i18n::text("host-i18n-detail-0058"))
                    .and_then(|bytes| ttf_parser::Face::parse(bytes, 0).map_err(Into::into))
            })
            .collect::<Result<Vec<_>>>()?;
        for ch in text.chars().filter(|c| {
            !c.is_ascii() && !c.is_whitespace() && !matches!(c, '\u{2068}' | '\u{2069}')
        }) {
            ensure!(
                faces.iter().any(|face| face.glyph_index(ch).is_some()),
                "{}",
                crate::i18n::format("host-i18n-detail-0057", &[("arg0", ch.to_string())])
            );
        }
        Ok(())
    }
    pub fn validate_interface(&self, def: &mut crate::interface::Definition) -> Result<()> {
        def.image_sizes = self
            .images
            .iter()
            .map(|(id, image)| (id.clone(), [image.width(), image.height()]))
            .collect();
        for id in &def.nine_patches {
            ensure!(
                self.patches.contains_key(id),
                "{}",
                crate::i18n::format("host-assets-0001", &[("arg0", id.to_string())])
            );
        }
        crate::interface::host::State::try_new(def)?;
        Ok(())
    }
    pub fn validate_text(&self, text: &str) -> Result<()> {
        let faces: Vec<_> = self
            .fonts
            .values()
            .map(|bytes| ttf_parser::Face::parse(bytes, 0).expect("validated font"))
            .collect();
        for ch in text.chars().filter(|ch| {
            !ch.is_ascii() && !ch.is_whitespace() && !matches!(ch, '\u{2068}' | '\u{2069}')
        }) {
            ensure!(
                faces.iter().any(|face| face.glyph_index(ch).is_some()),
                "{}",
                crate::i18n::format("host-assets-0002", &[("arg0", ch.to_string())])
            );
        }
        Ok(())
    }
    /// Decode once at startup. This also powers the headless `--check` command.
    pub fn load_from(story: &Story, source: &dyn crate::content::ContentSource) -> Result<Self> {
        Self::load_bytes(story, &|path| source.read(path), false)
    }
    pub fn load_deferred(
        story: &Story,
        source: &dyn crate::content::ContentSource,
    ) -> Result<Self> {
        Self::load_bytes(story, &|path| source.read(path), true)
    }
    fn load_bytes(
        story: &Story,
        read: &dyn Fn(&std::path::Path) -> Result<Vec<u8>>,
        deferred: bool,
    ) -> Result<Self> {
        let mut images = BTreeMap::new();
        let mut patches = BTreeMap::new();
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
            let context = || {
                crate::i18n::format(
                    "host-assets-0003",
                    &[
                        ("arg0", id.to_string()),
                        ("arg1", format!("{}", asset.path.display())),
                    ],
                )
            };
            match asset.kind {
                AssetKind::Audio | AssetKind::Video | AssetKind::Live2D => {}
                AssetKind::Image | AssetKind::Cg | AssetKind::NinePatch => {
                    let bytes = read(&asset.path).with_context(context)?;
                    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
                        .with_guessed_format()
                        .with_context(context)?;
                    let mut limits = image::Limits::default();
                    limits.max_image_width = Some(8192);
                    limits.max_image_height = Some(8192);
                    reader.limits(limits);
                    let image = reader.decode().with_context(context)?.to_rgba8();
                    let image = if asset.kind == AssetKind::NinePatch {
                        let (patch, image) =
                            crate::image_layout::NinePatch::parse(image).with_context(context)?;
                        patches.insert(id.clone(), patch);
                        image
                    } else {
                        image
                    };
                    let size = [image.width() as usize, image.height() as usize];
                    for filter in &filters {
                        if asset.kind == AssetKind::NinePatch {
                            continue;
                        }
                        filtered_bytes = filtered_bytes
                            .checked_add(size[0] * size[1] * 4)
                            .context(crate::i18n::text("host-assets-0008"))?;
                        ensure!(
                            filtered_bytes <= 256 * 1024 * 1024,
                            "{}",
                            crate::i18n::format("host-assets-0004", &[])
                        );
                        if deferred {
                            continue;
                        }
                        let variant = filter_image(&image, *filter);
                        filtered.insert(
                            (id.clone(), *filter),
                            ColorImage::from_rgba_unmultiplied(size, variant.as_raw()),
                        );
                    }
                    images.insert(id.clone(), image);
                }
                AssetKind::Font => {
                    let data = read(&asset.path).with_context(context)?;
                    ttf_parser::Face::parse(&data, 0).map_err(|e| {
                        anyhow::anyhow!(
                            "{}",
                            crate::i18n::format(
                                "host-assets-0005",
                                &[
                                    ("arg0", context().to_string()),
                                    ("arg1", format!("{:?}", e))
                                ]
                            )
                        )
                    })?;
                    fonts.insert(id.clone(), data);
                }
            }
        }
        let faces: Vec<_> = fonts
            .values()
            .map(|data| ttf_parser::Face::parse(data, 0).expect("validated font"))
            .collect();
        let mut text = crate::i18n::font_seed();
        for decl in story.variables.values() {
            text.push_str(&decl.label);
            if let VariableValue::String(value) = &decl.initial {
                text.push_str(value);
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
        for ch in text.chars().filter(|ch| {
            !ch.is_ascii() && !ch.is_whitespace() && !matches!(ch, '\u{2068}' | '\u{2069}')
        }) {
            ensure!(
                faces.iter().any(|face| face.glyph_index(ch).is_some()),
                "{}",
                crate::i18n::format("host-assets-0007", &[("arg0", ch.to_string())])
            );
        }
        Ok(Self {
            patches,
            images,
            deferred: if deferred {
                filters
            } else {
                Default::default()
            },
            filtered,
            fonts,
        })
    }

    pub fn install(self, ctx: &egui::Context) -> AssetCache {
        self.install_render(ctx, &crate::render::Renderer::cpu())
    }
    pub fn install_render(
        mut self,
        ctx: &egui::Context,
        renderer: &crate::render::Shared,
    ) -> AssetCache {
        let mut native_filtered = BTreeMap::new();
        for (id, image) in &self.images {
            if self.patches.contains_key(id) {
                continue;
            }
            for filter in &self.deferred {
                let result = renderer.borrow_mut().filter(image, *filter);
                native_filtered.insert((id.clone(), *filter), Texture::from_image(ctx, result, id));
            }
        }
        self.deferred.clear();
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
                let image = ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                );
                let handle =
                    Texture::Managed(ctx.load_texture(&id, image, egui::TextureOptions::LINEAR));
                (id, handle)
            })
            .collect();
        let mut filtered: BTreeMap<_, _> = self
            .filtered
            .into_iter()
            .map(|(key, image)| {
                let handle = ctx.load_texture(
                    format!("{}:{:?}", key.0, key.1),
                    image,
                    egui::TextureOptions::LINEAR,
                );
                (key, Texture::Managed(handle))
            })
            .collect();
        filtered.extend(native_filtered);
        AssetCache {
            textures,
            filtered,
            patches: self.patches,
        }
    }
}

#[derive(Clone)]
pub struct AssetCache {
    patches: BTreeMap<String, crate::image_layout::NinePatch>,
    textures: BTreeMap<String, Texture>,
    filtered: BTreeMap<(String, Filter), Texture>,
}

impl AssetCache {
    pub fn nine_patch(&self, id: &str) -> Option<&crate::image_layout::NinePatch> {
        self.patches.get(id)
    }
    pub fn clear_save_previews(&mut self) {
        self.textures.retain(|id, _| !id.starts_with("$save:"));
    }
    pub(crate) fn restore_streams_from(&mut self, previous: &Self) {
        for (id, texture) in &previous.textures {
            if (id == "$video" || id.starts_with("$save:"))
                && matches!(texture, Texture::Managed(_))
            {
                self.textures.insert(id.clone(), texture.clone());
            }
        }
    }

    pub fn has_stream(&self, id: &str) -> bool {
        self.textures.contains_key(id)
    }
    pub fn update_render(&mut self, ctx: &egui::Context, id: &str, image: crate::render::Image) {
        match image {
            crate::render::Image::Cpu(image) => self.update_stream(ctx, id, image),
            crate::render::Image::Gpu(image) => {
                self.textures.insert(id.into(), Texture::Native(image));
            }
        }
    }
    pub fn update_stream(&mut self, ctx: &egui::Context, id: &str, image: ColorImage) {
        if let Some(Texture::Managed(texture)) = self.textures.get_mut(id) {
            texture.set(image, egui::TextureOptions::LINEAR);
        } else {
            self.textures.insert(
                id.into(),
                Texture::Managed(ctx.load_texture(id, image, egui::TextureOptions::LINEAR)),
            );
        }
    }
    pub fn effected(&self, id: &str, filter: Filter) -> Option<&Texture> {
        if filter == Filter::None {
            self.image(id)
        } else {
            self.filtered.get(&(id.to_owned(), filter))
        }
    }
    pub fn image(&self, id: &str) -> Option<&Texture> {
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

#[derive(Clone)]
pub enum Texture {
    Managed(TextureHandle),
    Native(crate::render::GpuImage),
}
impl Texture {
    fn from_image(ctx: &egui::Context, image: crate::render::Image, name: &str) -> Self {
        match image {
            crate::render::Image::Cpu(image) => {
                Self::Managed(ctx.load_texture(name, image, egui::TextureOptions::LINEAR))
            }
            crate::render::Image::Gpu(image) => Self::Native(image),
        }
    }
    pub fn id(&self) -> egui::TextureId {
        match self {
            Self::Managed(t) => t.id(),
            Self::Native(t) => t.id,
        }
    }
    pub fn size(&self) -> [usize; 2] {
        match self {
            Self::Managed(t) => t.size(),
            Self::Native(t) => t.size,
        }
    }
    pub fn size_vec2(&self) -> egui::Vec2 {
        let [x, y] = self.size();
        egui::vec2(x as f32, y as f32)
    }
}
