use crate::save::{atomic_json, read_json_bytes};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub autosave: bool,
    pub auto_delay: f32,
    pub video_volume: f32,
    pub text_speed: f32,
    pub muted: bool,
    pub skip_all: bool,
    pub fullscreen: bool,
    pub bgm_volume: f32,
    pub voice_volume: f32,
    pub se_volume: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            autosave: true,
            auto_delay: 1.5,
            video_volume: 0.8,
            text_speed: 30.0,
            muted: false,
            skip_all: false,
            fullscreen: false,
            bgm_volume: 0.35,
            voice_volume: 0.8,
            se_volume: 0.7,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.auto_delay.is_finite() && (0.2..=15.0).contains(&self.auto_delay),
            "自动播放等待须在 0.2–15 秒"
        );
        ensure!(
            self.text_speed.is_finite() && (0.0..=80.0).contains(&self.text_speed),
            "文字速度必须在 0–80 之间"
        );
        for value in [
            self.bgm_volume,
            self.voice_volume,
            self.se_volume,
            self.video_volume,
        ] {
            ensure!(
                value.is_finite() && (0.0..=1.0).contains(&value),
                "音量必须在 0–1 之间"
            );
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self> {
        let settings: Self = serde_json::from_slice(&read_json_bytes(path)?)?;
        settings.validate()?;
        Ok(settings)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        atomic_json(path, self)
    }
}
