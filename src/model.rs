use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub mod effects;
pub mod rules;

/// The controller sees plans, never interpreter handles or content IO.
pub trait Logic {
    fn resolve(
        &self,
        story: &Story,
        frame: &str,
        choice: Option<&str>,
        variables: &Variables,
    ) -> Result<Option<TransitionRule>, String>;
    fn options(&self, story: &Story, session: &Session) -> Result<Vec<String>, String>;
}

pub struct DeclarativeLogic;
impl Logic for DeclarativeLogic {
    fn resolve(
        &self,
        story: &Story,
        frame: &str,
        choice: Option<&str>,
        variables: &Variables,
    ) -> Result<Option<TransitionRule>, String> {
        rules::resolve(story, frame, choice, variables).map(|rule| rule.cloned())
    }
    fn options(&self, story: &Story, session: &Session) -> Result<Vec<String>, String> {
        story
            .available_options(session)
            .map(|items| items.iter().map(|item| item.id.clone()).collect())
    }
}

pub type Variables = BTreeMap<String, VariableValue>;
pub const MAX_VARIABLES: usize = 128;
pub const MAX_STRING_BYTES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "lowercase",
    deny_unknown_fields
)]
pub enum VariableValue {
    Int(i64),
    Bool(bool),
    String(String),
}

impl std::fmt::Display for VariableValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Int(value) => write!(f, "{value}"),
            Self::Bool(value) => f.write_str(if *value { "是" } else { "否" }),
            Self::String(value) => f.write_str(value),
        }
    }
}

#[derive(Clone, Debug)]
pub struct VariableDecl {
    pub initial: VariableValue,
    pub label: String,
    pub min: Option<i64>,
    pub max: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug)]
pub enum Condition {
    Compare {
        variable: String,
        op: CompareOp,
        value: VariableValue,
    },
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Not(Box<Condition>),
}

#[derive(Clone, Debug)]
pub enum Effect {
    Set {
        variable: String,
        value: VariableValue,
    },
    Add {
        variable: String,
        amount: i64,
    },
}

#[derive(Clone, Debug)]
pub struct TransitionRule {
    pub to: String,
    pub when: Option<Condition>,
    pub effects: Vec<Effect>,
}

impl TransitionRule {
    pub fn plain(to: impl Into<String>) -> Self {
        Self {
            to: to.into(),
            when: None,
            effects: Vec::new(),
        }
    }
}

impl From<&str> for TransitionRule {
    fn from(to: &str) -> Self {
        Self::plain(to)
    }
}
impl From<String> for TransitionRule {
    fn from(to: String) -> Self {
        Self::plain(to)
    }
}

pub const MAX_HISTORY: usize = 1000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetKind {
    Image,
    Font,
    Audio,
    Video,
    Live2D,
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

#[derive(Clone, Debug, PartialEq)]
pub struct Portrait {
    pub live2d: Option<Live2DPose>,
    pub pose: effects::PortraitPose,
    pub enter: effects::PortraitAnimation,
    pub exit: effects::PortraitAnimation,
    pub character: String,
    pub image: String,
    pub slot: Slot,
    pub effect: effects::ImageEffect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Live2DPose {
    pub model: String,
    pub motion: Option<String>,
    pub expression: Option<String>,
    pub parameters: BTreeMap<String, f32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoCue {
    pub asset: String,
    pub skippable: bool,
    pub advance: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Visual {
    pub background: Option<String>,
    pub portraits: Vec<Portrait>,
    pub background_effect: effects::ImageEffect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub id: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub video: Option<VideoCue>,
    pub id: String,
    pub visual: Visual,
    pub transition: effects::Transition,
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
    Next(TransitionRule),
    Choice(BTreeMap<String, TransitionRule>),
    Branch {
        cases: Vec<TransitionRule>,
        otherwise: TransitionRule,
    },
    End,
    Script(ScriptRoute),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptRoute {
    Advance,
    Choice,
    End,
}

impl Route {
    pub fn rules(&self) -> Vec<&TransitionRule> {
        match self {
            Self::Next(rule) => vec![rule],
            Self::Choice(choices) => choices.values().collect(),
            Self::Branch { cases, otherwise } => {
                cases.iter().chain(std::iter::once(otherwise)).collect()
            }
            Self::End | Self::Script(_) => Vec::new(),
        }
    }
    pub fn targets(&self) -> Vec<&str> {
        match self {
            Self::Next(rule) => vec![&rule.to],
            Self::Choice(choices) => choices.values().map(|rule| rule.to.as_str()).collect(),
            Self::Branch { cases, otherwise } => cases
                .iter()
                .chain(std::iter::once(otherwise))
                .map(|rule| rule.to.as_str())
                .collect(),
            Self::End | Self::Script(_) => Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Story {
    pub effects: BTreeMap<String, effects::ImageEffect>,
    pub id: String,
    pub content_version: u32,
    pub entry: String,
    pub assets: BTreeMap<String, Asset>,
    pub characters: BTreeMap<String, String>,
    pub frames: BTreeMap<String, Frame>,
    pub routes: BTreeMap<String, Route>,
    pub variables: BTreeMap<String, VariableDecl>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub frame_id: String,
    pub history: Vec<HistoryEntry>,
    pub variables: Variables,
    pub history_base_variables: Variables,
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
            variables: Variables::new(),
            history_base_variables: Variables::new(),
        }
    }
}

impl Story {
    pub fn start(&self) -> Session {
        let mut session = Session::at(self.entry.clone());
        session.variables = self.initial_variables();
        session.history_base_variables = session.variables.clone();
        session
    }

    pub fn initial_variables(&self) -> Variables {
        self.variables
            .iter()
            .map(|(id, decl)| (id.clone(), decl.initial.clone()))
            .collect()
    }

    pub fn available_options(&self, session: &Session) -> Result<Vec<&Choice>, String> {
        let frame = self
            .frame(session)
            .ok_or_else(|| "当前场景不存在".to_owned())?;
        if matches!(self.routes.get(&session.frame_id), Some(Route::Script(_))) {
            return Err("脚本选项需要注入逻辑执行器".into());
        }
        let Some(Route::Choice(routes)) = self.routes.get(&session.frame_id) else {
            return Ok(Vec::new());
        };
        frame
            .options
            .iter()
            .filter_map(|option| {
                let result = routes
                    .get(&option.id)
                    .ok_or_else(|| format!("选项缺少路由：{}", option.id))
                    .and_then(|rule| rules::matches(rule.when.as_ref(), &session.variables));
                match result {
                    Ok(true) => Some(Ok(option)),
                    Ok(false) => None,
                    Err(error) => Some(Err(error)),
                }
            })
            .collect()
    }

    pub fn frame(&self, session: &Session) -> Option<&Frame> {
        self.frames.get(&session.frame_id)
    }

    pub fn validate_session(&self, session: &Session) -> Result<(), String> {
        self.validate_session_with(session, &DeclarativeLogic)
    }

    pub fn validate_session_with(
        &self,
        session: &Session,
        logic: &dyn Logic,
    ) -> Result<(), String> {
        rules::validate_variables(self, &session.variables)?;
        rules::validate_variables(self, &session.history_base_variables)?;
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
        let mut variables = session.history_base_variables.clone();
        for pair in session.history.windows(2) {
            let previous = &pair[0];
            let rule = logic.resolve(
                self,
                &previous.frame_id,
                previous.choice_id.as_deref(),
                &variables,
            )?;
            if rule.as_ref().map(|rule| &rule.to) != Some(&pair[1].frame_id) {
                return Err(format!(
                    "历史路由不一致：{} → {}",
                    previous.frame_id, pair[1].frame_id
                ));
            }
            variables = rules::apply_effects(
                self,
                &variables,
                &rule.expect("matching history target").effects,
            )?;
        }
        if variables != session.variables {
            return Err("当前变量与历史重放结果不一致".into());
        }
        logic.options(self, session)?;
        Ok(())
    }
}
