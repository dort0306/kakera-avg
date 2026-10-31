use crate::save::{atomic_json, read_json_bytes};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Accepted range of one numeric preference. Shared by the file loader and the UI
/// command validator so a value just outside the interval cannot become legal
/// through the f64 to f32 narrowing.
fn range(name: &str) -> Option<(f64, f64)> {
    Some(match name {
        "auto_delay" => (0.2, 15.0),
        "text_speed" => (0.0, 80.0),
        "master_volume" | "bgm_volume" | "voice_volume" | "se_volume" | "video_volume" => {
            (0.0, 1.0)
        }
        _ => return None,
    })
}

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
    /// Master gain multiplied with every channel; never rewrites the channel values.
    pub master_volume: f32,
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
            master_volume: 1.0,
        }
    }
}

impl Settings {
    /// Range check before any narrowing cast. `ctx.setting` values arrive as f64 and
    /// a value such as `1.0000001` must be rejected rather than rounded into `1.0`.
    pub fn validate_number(name: &str, value: f64) -> Result<()> {
        let (min, max) = range(name).ok_or_else(|| {
            anyhow::anyhow!(
                "{}",
                crate::i18n::format("host-settings-0004", &[("arg0", name.to_string())])
            )
        })?;
        ensure!(
            value.is_finite() && (min..=max).contains(&value),
            "{}",
            crate::i18n::format("host-settings-0005", &[("arg0", name.to_string())])
        );
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.auto_delay.is_finite() && (0.2..=15.0).contains(&self.auto_delay),
            "{}",
            crate::i18n::format("host-settings-0001", &[])
        );
        ensure!(
            self.text_speed.is_finite() && (0.0..=80.0).contains(&self.text_speed),
            "{}",
            crate::i18n::format("host-settings-0002", &[])
        );
        for value in [
            self.bgm_volume,
            self.voice_volume,
            self.se_volume,
            self.video_volume,
            self.master_volume,
        ] {
            ensure!(
                value.is_finite() && (0.0..=1.0).contains(&value),
                "{}",
                crate::i18n::format("host-settings-0003", &[])
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
