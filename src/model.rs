use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetKind {
    Image,
    Font,
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
}

impl Story {
    pub fn start(&self) -> Session {
        Session {
            frame_id: self.entry.clone(),
        }
    }

    pub fn frame(&self, session: &Session) -> Option<&Frame> {
        self.frames.get(&session.frame_id)
    }
}
