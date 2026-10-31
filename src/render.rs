//! Platform-neutral pixel backend boundary; domain data never owns GPU objects.
use crate::{live2d::core::Drawable, model::effects::Filter};
use anyhow::Result;
use eframe::egui::{self, ColorImage};
use std::{any::Any, cell::RefCell, rc::Rc};
#[cfg(windows)]
pub mod d3d11;
#[cfg(windows)]
pub mod window;
#[derive(Clone)]
pub struct GpuImage {
    pub id: egui::TextureId,
    pub size: [usize; 2],
    pub(crate) allocation: Rc<dyn Any>,
}
impl GpuImage {
    pub fn keep_alive(&self) -> &dyn Any {
        &*self.allocation
    }
}
#[derive(Clone)]
pub enum Image {
    Cpu(ColorImage),
    Gpu(GpuImage),
}
pub struct ModelDraw<'a> {
    pub meshes: Vec<Drawable>,
    /// Stable identity of the model whose textures are drawn; never an allocation address.
    pub model: u64,
    pub textures: &'a [image::RgbaImage],
    pub canvas: [f32; 2],
    pub origin: [f32; 2],
    pub ppu: f32,
    pub opacity: f32,
}
pub trait PixelBackend {
    fn name(&self) -> &str;
    fn filter(&mut self, source: &image::RgbaImage, filter: Filter) -> Result<Image>;
    fn model(&mut self, draw: ModelDraw<'_>) -> Result<Image>;
    fn clear(&mut self);
}
pub struct Renderer {
    gpu: Option<Box<dyn PixelBackend>>,
    pub warning: Option<String>,
}
pub type Shared = Rc<RefCell<Renderer>>;
impl Renderer {
    pub fn cpu() -> Shared {
        Rc::new(RefCell::new(Self {
            gpu: None,
            warning: None,
        }))
    }
    pub fn with_gpu(gpu: Box<dyn PixelBackend>) -> Shared {
        Rc::new(RefCell::new(Self {
            gpu: Some(gpu),
            warning: None,
        }))
    }
    pub fn is_gpu(&self) -> bool {
        self.gpu.is_some()
    }
    pub fn name(&self) -> &str {
        self.gpu.as_ref().map_or("CPU fallback", |g| g.name())
    }
    pub fn fallback(&mut self, reason: String) {
        self.gpu = None;
        self.warning = Some(reason);
    }
    pub fn clear(&mut self) {
        if let Some(gpu) = &mut self.gpu {
            gpu.clear();
        }
    }
    pub fn filter(&mut self, source: &image::RgbaImage, filter: Filter) -> Image {
        if let Some(gpu) = &mut self.gpu {
            match gpu.filter(source, filter) {
                Ok(image) => return image,
                Err(e) => self.fallback(crate::i18n::format(
                    "host-render-0001",
                    &[("arg0", format!("{:#}", e))],
                )),
            }
        }
        let image = crate::assets::filter_image(source, filter);
        Image::Cpu(ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        ))
    }
    pub fn model(&mut self, draw: ModelDraw<'_>) -> Result<Image> {
        if let Some(gpu) = &mut self.gpu {
            let attempt = ModelDraw {
                meshes: draw.meshes.clone(),
                ..draw
            };
            match gpu.model(attempt) {
                Ok(image) => return Ok(image),
                Err(e) => self.fallback(crate::i18n::format(
                    "host-render-0002",
                    &[("arg0", format!("{:#}", e))],
                )),
            }
        }
        Ok(Image::Cpu(crate::live2d::raster::composite(
            draw.meshes,
            draw.textures,
            draw.canvas,
            draw.origin,
            draw.ppu,
            draw.opacity,
        )?))
    }
}
