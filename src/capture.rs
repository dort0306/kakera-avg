//! Presentation-only capture interface. A host supplies readers for rendered client frames.
use anyhow::{Result, anyhow};
use eframe::egui::ColorImage;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone)]
pub struct Frame {
    pub revision: u64,
    pub read: Rc<dyn Fn() -> Result<ColorImage>>,
}
#[derive(Default)]
pub struct Cache {
    pub latest: Option<Frame>,
    pub game: Option<Frame>,
    pub error: Option<String>,
}
pub type Shared = Rc<RefCell<Cache>>;
impl Cache {
    pub fn frame(&self, revision: u64) -> Result<Option<Frame>> {
        if let Some(error) = &self.error {
            return Err(anyhow!(
                "{}",
                crate::i18n::format("host-capture-0001", &[("arg0", error.to_string())])
            ));
        }
        Ok(self
            .game
            .as_ref()
            .filter(|f| f.revision == revision)
            .or_else(|| self.latest.as_ref().filter(|f| f.revision == revision))
            .cloned())
    }
}
