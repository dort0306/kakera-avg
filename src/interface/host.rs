//! Pure UI plans and independent collection persistence; IO is invoked by App.
use super::Definition;
use crate::{
    model::{VariableValue, Variables, rules},
    save::{SlotId, atomic_json, read_json_bytes},
    settings::Settings,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeSet, path::Path};

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub slot_page: u16,
    pub stack: Vec<String>,
    pub variables: Variables,
}
impl State {
    pub fn new(def: &Definition) -> Self {
        Self {
            slot_page: 1,
            stack: vec![def.initial.clone()],
            variables: def.initial_variables(),
        }
    }
    pub fn paused(&self, def: &Definition) -> bool {
        self.stack.iter().any(|id| def.screens[id].pause)
    }
    pub fn keyboard(&self, def: &Definition) -> bool {
        self.stack.len() == 1 && !self.paused(def)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub schema_version: u32,
    pub story_id: String,
    pub unlocked: BTreeSet<String>,
}
impl Profile {
    pub fn new(id: &str, def: &Definition) -> Self {
        Self {
            schema_version: 1,
            story_id: id.into(),
            unlocked: def
                .gallery
                .iter()
                .filter(|(_, cg)| cg.unlocked)
                .map(|(id, _)| id.clone())
                .collect(),
        }
    }
    pub fn load(path: &Path, id: &str, def: &Definition) -> Result<Self> {
        let profile: Self = serde_json::from_slice(&read_json_bytes(path)?)?;
        ensure!(
            profile.schema_version == 1 && profile.story_id == id,
            "CG 进度版本或故事不匹配"
        );
        ensure!(
            profile
                .unlocked
                .iter()
                .all(|id| def.gallery.contains_key(id)),
            "CG 进度含未知 ID"
        );
        let mut result = profile;
        result.unlocked.extend(Self::new(id, def).unlocked);
        Ok(result)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        atomic_json(path, self)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "lowercase",
    deny_unknown_fields
)]
pub enum UiValue {
    Int(String),
    Bool(bool),
    String(String),
}
impl UiValue {
    fn value(self) -> Result<VariableValue> {
        Ok(match self {
            Self::Int(value) => VariableValue::Int(value.parse()?),
            Self::Bool(value) => VariableValue::Bool(value),
            Self::String(value) => VariableValue::String(value),
        })
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Live2d {
        slot: String,
        motion: String,
    },
    SkipVideo,
    SlotsPage {
        page: u16,
    },
    LoadAuto {
        slot: u16,
    },
    AutoPlay {
        enabled: bool,
    },
    Portrait {
        slot: String,
        animation: crate::model::effects::PortraitAnimation,
    },
    Transition {
        transition: crate::model::effects::Transition,
    },
    Animate {
        target: String,
        animation: crate::model::effects::Animation,
    },
    Open {
        screen: String,
    },
    Close,
    SetUi {
        name: String,
        value: UiValue,
    },
    Save {
        slot: u16,
        #[serde(default)]
        overwrite: bool,
    },
    Load {
        slot: u16,
    },
    ImportLegacy,
    Restart,
    Advance,
    Choose {
        option: String,
    },
    Setting {
        name: String,
        value: Value,
    },
    Unlock {
        id: String,
    },
    Message {
        text: String,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum Operation {
    SkipVideo,
    None,
    Save(SlotId, bool),
    Load(SlotId),
    ImportLegacy,
    LoadAuto(u16),
    Restart,
    Advance,
    Choose(String),
    Settings,
    Profile,
}
pub struct Plan {
    pub live2d: Vec<(crate::model::Slot, String)>,
    pub auto_play: Option<bool>,
    pub portraits: Vec<(crate::model::Slot, crate::model::effects::PortraitAnimation)>,
    pub transition: Option<crate::model::effects::Transition>,
    pub animations: Vec<(String, crate::model::effects::Animation)>,
    pub state: State,
    pub settings: Settings,
    pub profile: Profile,
    pub operation: Operation,
    pub message: Option<String>,
}
fn operation(current: &mut Operation, next: Operation) -> Result<()> {
    let aggregate = matches!(
        (&*current, &next),
        (Operation::Settings, Operation::Settings) | (Operation::Profile, Operation::Profile)
    );
    ensure!(
        matches!(current, Operation::None) || aggregate,
        "一次 UI 事件不能混合多个 IO/剧情操作"
    );
    *current = next;
    Ok(())
}
pub fn prepare(
    def: &Definition,
    state: &State,
    settings: &Settings,
    profile: &Profile,
    commands: Vec<Command>,
    entering: bool,
) -> Result<Plan> {
    ensure!(commands.len() <= 32, "UI 命令超过 32 条");
    let mut plan = Plan {
        live2d: Vec::new(),
        auto_play: None,
        portraits: Vec::new(),
        transition: None,
        animations: Vec::new(),
        state: state.clone(),
        settings: settings.clone(),
        profile: profile.clone(),
        operation: Operation::None,
        message: None,
    };
    for command in commands {
        if entering {
            ensure!(
                matches!(
                    command,
                    Command::Unlock { .. }
                        | Command::SetUi { .. }
                        | Command::Message { .. }
                        | Command::Transition { .. }
                        | Command::Animate { .. }
                        | Command::Portrait { .. }
                        | Command::Live2d { .. }
                ),
                "game.enter 仅允许 unlock/setUi/message/transition/animate/portrait"
            );
        }
        match command {
            Command::SkipVideo => operation(&mut plan.operation, Operation::SkipVideo)?,
            Command::Live2d { slot, motion } => {
                let slot = match slot.as_str() {
                    "left" => crate::model::Slot::Left,
                    "center" => crate::model::Slot::Center,
                    "right" => crate::model::Slot::Right,
                    _ => anyhow::bail!("未知 Live2D 槽位"),
                };
                ensure!(motion.len() <= 256, "Live2D 动作名过长");
                plan.live2d.push((slot, motion));
            }
            Command::SlotsPage { page } => {
                ensure!(
                    (1..=crate::save::PAGE_COUNT).contains(&page),
                    "存档页须在 1–20"
                );
                plan.state.slot_page = page;
            }
            Command::LoadAuto { slot } => {
                ensure!(
                    (1..=crate::save::AUTO_COUNT).contains(&slot),
                    "自动槽须在 1–10"
                );
                operation(&mut plan.operation, Operation::LoadAuto(slot))?;
            }
            Command::AutoPlay { enabled } => {
                plan.auto_play = Some(enabled);
            }

            Command::Portrait { slot, animation } => {
                let slot = match slot.as_str() {
                    "left" => crate::model::Slot::Left,
                    "center" => crate::model::Slot::Center,
                    "right" => crate::model::Slot::Right,
                    _ => anyhow::bail!("未知立绘槽位：{slot}"),
                };
                crate::effects::validate_portrait(animation).map_err(anyhow::Error::msg)?;
                plan.portraits.push((slot, animation));
            }
            Command::Transition { transition } => {
                ensure!(entering, "transition 仅可用于 game.enter");
                crate::effects::validate_transition(transition).map_err(anyhow::Error::msg)?;
                ensure!(
                    plan.transition.replace(transition).is_none(),
                    "一次进入不能重复指定转场"
                );
            }
            Command::Animate { target, animation } => {
                fn valid(nodes: &[super::Node], target: &str, repeated: bool) -> bool {
                    nodes.iter().any(|n| {
                        (!repeated
                            && n.id == target
                            && (matches!(n.kind, super::Kind::Scene)
                                || matches!(n.kind, super::Kind::Image | super::Kind::Panel)
                                    && (n.image.is_some() || n.image_bind.is_some())))
                            || valid(
                                &n.children,
                                target,
                                repeated || matches!(n.kind, super::Kind::Repeat { .. }),
                            )
                    })
                }
                ensure!(
                    def.screens
                        .values()
                        .any(|s| valid(&s.nodes, &target, false)),
                    "动画目标不存在、在 repeat 内或不是 scene/图片节点：{target}"
                );
                crate::effects::validate_animation(animation).map_err(anyhow::Error::msg)?;
                plan.animations.push((target, animation));
            }
            Command::Open { screen } => {
                let target = def
                    .screens
                    .get(&screen)
                    .ok_or_else(|| anyhow::anyhow!("未知 UI 屏幕：{screen}"))?;
                if target.modal {
                    ensure!(
                        plan.state.stack.len() < 8 && !plan.state.stack.contains(&screen),
                        "UI 模态栈重复或超过 8 层"
                    );
                    plan.state.stack.push(screen);
                } else {
                    plan.state.stack = vec![screen];
                }
            }
            Command::Close => {
                ensure!(plan.state.stack.len() > 1, "不能关闭唯一根屏幕");
                plan.state.stack.pop();
            }
            Command::SetUi { name, value } => {
                let decl = def
                    .variables
                    .get(&name)
                    .ok_or_else(|| anyhow::anyhow!("未知 UI 变量：{name}"))?;
                let value = value.value()?;
                rules::validate_value(decl, &value).map_err(anyhow::Error::msg)?;
                plan.state.variables.insert(name, value);
            }
            Command::Save { slot, overwrite } => operation(
                &mut plan.operation,
                Operation::Save(SlotId::new(slot)?, overwrite),
            )?,
            Command::Load { slot } => {
                operation(&mut plan.operation, Operation::Load(SlotId::new(slot)?))?
            }
            Command::ImportLegacy => operation(&mut plan.operation, Operation::ImportLegacy)?,
            Command::Restart => operation(&mut plan.operation, Operation::Restart)?,
            Command::Advance => operation(&mut plan.operation, Operation::Advance)?,
            Command::Choose { option } => {
                operation(&mut plan.operation, Operation::Choose(option))?
            }
            Command::Unlock { id } => {
                ensure!(def.gallery.contains_key(&id), "未知 CG：{id}");
                if plan.profile.unlocked.insert(id) {
                    operation(&mut plan.operation, Operation::Profile)?;
                }
            }
            Command::Setting { name, value } => {
                if name == "muted" || name == "autosave" {
                    let enabled = value
                        .as_bool()
                        .ok_or_else(|| anyhow::anyhow!("设置需要 bool"))?;
                    if name == "muted" {
                        plan.settings.muted = enabled;
                    } else {
                        plan.settings.autosave = enabled;
                    }
                } else {
                    let value = value
                        .as_f64()
                        .ok_or_else(|| anyhow::anyhow!("设置需要有限数值"))?
                        as f32;
                    match name.as_str() {
                        "auto_delay" => plan.settings.auto_delay = value,
                        "video_volume" => plan.settings.video_volume = value,
                        "text_speed" => plan.settings.text_speed = value,
                        "bgm_volume" => plan.settings.bgm_volume = value,
                        "voice_volume" => plan.settings.voice_volume = value,
                        "se_volume" => plan.settings.se_volume = value,
                        _ => anyhow::bail!("未知设置：{name}"),
                    }
                }
                plan.settings.validate()?;
                operation(&mut plan.operation, Operation::Settings)?;
            }
            Command::Message { text } => {
                ensure!(text.len() <= 4096, "UI 提示超过 4096 字节");
                plan.message = Some(text);
            }
        }
    }
    validate_images(def, &plan.state)?;
    Ok(plan)
}
pub fn validate_images(def: &Definition, state: &State) -> Result<()> {
    fn walk(def: &Definition, state: &State, nodes: &[super::Node]) -> Result<()> {
        for node in nodes {
            if let Some(id) = node
                .image_bind
                .as_deref()
                .and_then(|path| path.strip_prefix("ui."))
            {
                let Some(VariableValue::String(image)) = state.variables.get(id) else {
                    anyhow::bail!("UI 图片变量类型错误")
                };
                ensure!(def.images.contains(image), "UI 图片变量引用不存在：{image}");
            }
            walk(def, state, &node.children)?;
        }
        Ok(())
    }
    for screen in def.screens.values() {
        walk(def, state, &screen.nodes)?;
    }
    Ok(())
}
