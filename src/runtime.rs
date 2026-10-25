//! Preparation shared by the player, project watcher and packer.
use crate::{
    app::RuntimeAssets,
    assets::PreparedAssets,
    audio::PreparedAudio,
    content::ContentSource,
    loader::{self, LoadedStory},
    script::ScriptLogic,
};
use anyhow::Result;
use std::path::PathBuf;
#[derive(Clone)]
pub struct LoadOptions {
    pub core: Option<PathBuf>,
    pub no_live2d: bool,
}
pub fn host_core() -> Option<PathBuf> {
    std::env::var_os("CUBISM_CORE")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.join("Live2DCubismCore.dll")))
                .filter(|p| p.is_file())
        })
}
pub fn default_core() -> Option<PathBuf> {
    host_core().or_else(|| {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("runtime/live2d/windows-x64/Live2DCubismCore.dll");
        p.is_file().then_some(p)
    })
}
pub struct Bundle {
    pub loaded: LoadedStory,
    pub logic: ScriptLogic,
    pub assets: RuntimeAssets,
}
impl Bundle {
    pub fn load(source: &dyn ContentSource, options: &LoadOptions) -> Result<Self> {
        let (mut loaded, logic) = loader::load_game(source)?;
        let images = PreparedAssets::load_deferred(&loaded.story, source)?;
        images.validate_interface(&mut loaded.interface)?;
        images.validate_text(&loaded.interface.text())?;
        let mut audio = PreparedAudio::load_from(&loaded.story, source)?;
        let media = crate::media::PreparedMedia::load(&loaded.story, source)?;
        for (id, video) in &media.videos {
            if let Some(clip) = &video.audio {
                audio.insert_video(id, clip.clone());
            }
        }
        let live2d = crate::live2d::PreparedLive2D::load(
            &loaded.story,
            source,
            options.core.as_deref(),
            options.no_live2d,
        )?;
        if let Some(plan) = loader::initial_ui_plan(&loaded, &logic)? {
            live2d.validate_commands(&loaded.story.frames[&loaded.story.entry], &plan.live2d)?;
        }
        Ok(Self {
            loaded,
            logic,
            assets: RuntimeAssets {
                images,
                audio,
                media,
                live2d,
            },
        })
    }
}
