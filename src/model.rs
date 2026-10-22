use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const MAX_HISTORY: usize = 1000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetKind {
    Image,
    Font,
    Audio,
}

#[derive(Clone, Debug)]
pub struct Asset {
    pub kind: AssetKind,
    pub path: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slot {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Portrait {
    pub character: String,
    pub image: String,
    pub slot: Slot,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Visual {
    pub background: Option<String>,
    pub portraits: Vec<Portrait>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub id: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub id: String,
    pub visual: Visual,
    pub speaker: Option<String>,
    pub text: String,
    pub options: Vec<Choice>,
    pub audio: AudioCue,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioCue {
    pub bgm: Option<String>,
    pub voice: Option<String>,
    pub se: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Route {
    Next(String),
    Choice(BTreeMap<String, String>),
    End,
}

#[derive(Clone, Debug)]
pub struct Story {
    pub id: String,
    pub content_version: u32,
    pub entry: String,
    pub assets: BTreeMap<String, Asset>,
    pub characters: BTreeMap<String, String>,
    pub frames: BTreeMap<String, Frame>,
    pub routes: BTreeMap<String, Route>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub frame_id: String,
    pub history: Vec<HistoryEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryEntry {
    pub frame_id: String,
    pub choice_id: Option<String>,
}

impl Session {
    /// Start at an explicit position, including imported v0.1 saves with no earlier history.
    pub fn at(frame_id: impl Into<String>) -> Self {
        let frame_id = frame_id.into();
        Self {
            history: vec![HistoryEntry {
                frame_id: frame_id.clone(),
                choice_id: None,
            }],
            frame_id,
        }
    }
}

impl Story {
    pub fn start(&self) -> Session {
        Session::at(self.entry.clone())
    }

    pub fn frame(&self, session: &Session) -> Option<&Frame> {
        self.frames.get(&session.frame_id)
    }

    pub fn validate_session(&self, session: &Session) -> Result<(), String> {
        if !self.frames.contains_key(&session.frame_id) {
            return Err(format!("存档场景不存在：{}", session.frame_id));
        }
        if session.history.is_empty() || session.history.len() > MAX_HISTORY {
            return Err(format!("历史条数必须在 1–{MAX_HISTORY} 之间"));
        }
        for entry in &session.history {
            if !self.frames.contains_key(&entry.frame_id) {
                return Err(format!("历史场景不存在：{}", entry.frame_id));
            }
        }
        let last = session.history.last().expect("nonempty history");
        if last.frame_id != session.frame_id || last.choice_id.is_some() {
            return Err("历史末尾必须是未离开的当前场景".into());
        }
        for pair in session.history.windows(2) {
            let previous = &pair[0];
            let target = match (self.routes.get(&previous.frame_id), &previous.choice_id) {
                (Some(Route::Next(to)), None) => Some(to),
                (Some(Route::Choice(choices)), Some(id)) => choices.get(id),
                _ => None,
            };
            if target != Some(&pair[1].frame_id) {
                return Err(format!(
                    "历史路由不一致：{} → {}",
                    previous.frame_id, pair[1].frame_id
                ));
            }
        }
        Ok(())
    }
}
