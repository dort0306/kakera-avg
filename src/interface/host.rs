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
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub window_title: String,
    pub regions: BTreeMap<String, crate::image_layout::Region>,
    pub slot_page: u16,
    pub stack: Vec<String>,
    pub variables: Variables,
    /// Committed player shortcut mapping; separate from story slots and settings.json.
    pub keybindings: crate::shortcuts::Keybindings,
    /// Player overrides for author-declared palette slots (sparse by design).
    pub colors: crate::interface::palette::Colors,
    /// Resolved palette values used by `colors.<slot>` bindings and `ctx.colors`.
    pub resolved_colors: BTreeMap<String, crate::color::Rgba>,
    /// Prepared color for each `color-bind` node (never re-parsed while drawing).
    pub color_binds: BTreeMap<String, crate::color::Rgba>,
}
impl State {
    pub fn try_new(def: &Definition) -> Result<Self> {
        let variables = def.initial_variables();
        let mut state = Self {
            window_title: def.window_title.resolve(&variables)?.to_owned(),
            regions: BTreeMap::new(),
            slot_page: 1,
            stack: vec![def.initial.clone()],
            variables,
            keybindings: crate::shortcuts::Keybindings::defaults(),
            colors: crate::interface::palette::Colors::default(),
            resolved_colors: BTreeMap::new(),
            color_binds: BTreeMap::new(),
        };
        state.refresh(def)?;
        Ok(state)
    }
    pub fn new(def: &Definition) -> Self {
        Self::try_new(def).expect("validated initial UI state")
    }
    /// Rebuilds every derived cache after variable, palette or definition changes.
    pub fn refresh(&mut self, def: &Definition) -> Result<()> {
        validate_images(def, self)?;
        self.resolved_colors = self.colors.resolve(&def.palette)?;
        self.color_binds = resolved_color_binds(def, self)?;
        self.regions = resolved_regions(def, self)?;
        Ok(())
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
    PlaySound {
        id: String,
    },
    Sequence {
        id: String,
    },
    SoundEnabled {
        enabled: bool,
    },
    SoundActor {
        actor: String,
        enabled: bool,
    },
    SoundExclude {
        asset: String,
        excluded: bool,
    },
    Live2d {
        slot: String,
        motion: String,
    },
    Present {
        screen: String,
    },
    Resume {
        frame: Option<String>,
    },
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
    Quit {},
    SetUi {
        name: String,
        value: UiValue,
    },
    CaptureShortcut {
        action: String,
        result_event: String,
    },
    CancelShortcutCapture {},
    Keybinding {
        action: String,
        binding: Option<String>,
    },
    ResetKeybindings {},
    ColorPreference {
        slot: String,
        value: Option<String>,
    },
    ResetColors {},
    Save {
        slot: u16,
        overwrite: bool,
    },
    Load {
        slot: u16,
    },
    Restart {
        frame: Option<String>,
    },
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
    Quit,
    UiAudio,
    Present,
    Resume(Option<String>),
    None,
    Save(SlotId, bool),
    Load(SlotId),
    LoadAuto(u16),
    Restart(Option<String>),
    Advance,
    Choose(String),
    Settings,
    Profile,
    Keybindings,
    Colors,
}
#[derive(Clone)]
pub struct CaptureRequest {
    pub action: crate::shortcuts::Action,
    pub result_event: String,
}
pub struct Plan {
    pub command_count: usize,
    pub ui_actions: Vec<super::presentation::Action>,
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
    pub capture: Option<CaptureRequest>,
    pub cancel_capture: bool,
}
fn operation(current: &mut Operation, next: Operation) -> Result<()> {
    let aggregate = matches!(
        (&*current, &next),
        (Operation::Settings, Operation::Settings)
            | (Operation::Profile, Operation::Profile)
            | (Operation::UiAudio, Operation::UiAudio)
            | (Operation::Keybindings, Operation::Keybindings)
            | (Operation::Colors, Operation::Colors)
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
        command_count: commands.len(),
        ui_actions: Vec::new(),
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
        capture: None,
        cancel_capture: false,
    };
    let mut keybinding_modified = BTreeSet::new();
    let mut keybinding_reset = false;
    let mut color_modified = BTreeSet::new();
    let mut color_reset = false;
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
            Command::PlaySound { id } => {
                ensure!(
                    !entering && def.media.sound_exists(&id),
                    "UI 音效不存在或生命周期禁止"
                );
                plan.ui_actions.push(super::presentation::Action::Sound(id));
            }
            Command::Sequence { id } => {
                ensure!(
                    !entering && def.media.sequences.contains_key(&id),
                    "UI 演出不存在或生命周期禁止"
                );
                ensure!(
                    !plan
                        .ui_actions
                        .iter()
                        .any(|a| matches!(a, super::presentation::Action::Sequence(_))),
                    "一次只能启动一个演出"
                );
                plan.ui_actions
                    .push(super::presentation::Action::Sequence(id));
            }
            Command::SoundEnabled { enabled } => {
                ensure!(!entering, "生命周期禁止修改音效偏好");
                operation(&mut plan.operation, Operation::UiAudio)?;
                plan.ui_actions
                    .push(super::presentation::Action::Enabled(enabled));
            }
            Command::SoundActor { actor, enabled } => {
                ensure!(
                    !entering && def.media.actors.contains(&actor),
                    "未知音效角色或生命周期禁止"
                );
                operation(&mut plan.operation, Operation::UiAudio)?;
                plan.ui_actions
                    .push(super::presentation::Action::Actor(actor, enabled));
            }
            Command::SoundExclude { asset, excluded } => {
                ensure!(
                    !entering && def.media.audio.contains(&asset),
                    "未知音效资源或生命周期禁止"
                );
                operation(&mut plan.operation, Operation::UiAudio)?;
                plan.ui_actions
                    .push(super::presentation::Action::Exclude(asset, excluded));
            }
            Command::Present { screen } => {
                ensure!(!entering, "game.enter 禁止交接");
                let target = def
                    .screens
                    .get(&screen)
                    .ok_or_else(|| anyhow::anyhow!("交接页面不存在"))?;
                if target.modal {
                    ensure!(
                        plan.state.stack.len() < 8 && !plan.state.stack.contains(&screen),
                        "交接页面重复或栈超限"
                    );
                    plan.state.stack.push(screen);
                } else {
                    plan.state.stack = vec![screen];
                }
                operation(&mut plan.operation, Operation::Present)?;
            }
            Command::Resume { frame } => {
                ensure!(!entering, "game.enter 禁止返回剧情");
                operation(&mut plan.operation, Operation::Resume(frame))?;
            }

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
            Command::Quit {} => operation(&mut plan.operation, Operation::Quit)?,
            Command::SetUi { name, value } => {
                let decl = def
                    .variables
                    .get(&name)
                    .ok_or_else(|| anyhow::anyhow!("未知 UI 变量：{name}"))?;
                let value = value.value()?;
                rules::validate_value(decl, &value).map_err(anyhow::Error::msg)?;
                plan.state.variables.insert(name, value);
            }
            Command::CaptureShortcut {
                action,
                result_event,
            } => {
                ensure!(!entering, "game.enter 不能启动快捷键捕获");
                let action = crate::shortcuts::Action::from_id(&action)
                    .ok_or_else(|| anyhow::anyhow!("未知快捷键动作：{action}"))?;
                ensure!(
                    def.ui_handlers.contains(&result_event),
                    "快捷键捕获结果事件未注册：{result_event}"
                );
                ensure!(
                    plan.capture.is_none() && !plan.cancel_capture,
                    "一次 UI 事件只能有一个捕获请求"
                );
                plan.capture = Some(CaptureRequest {
                    action,
                    result_event,
                });
            }
            Command::CancelShortcutCapture {} => {
                ensure!(!entering, "game.enter 不能取消快捷键捕获");
                ensure!(!plan.cancel_capture, "重复取消快捷键捕获");
                plan.cancel_capture = true;
            }
            Command::Keybinding { action, binding } => {
                ensure!(!entering, "game.enter 不能修改快捷键");
                ensure!(!keybinding_reset, "恢复默认不能与单项修改同批");
                let action = crate::shortcuts::Action::from_id(&action)
                    .ok_or_else(|| anyhow::anyhow!("未知快捷键动作：{action}"))?;
                ensure!(
                    keybinding_modified.insert(action),
                    "同一批重复修改快捷键：{}",
                    action.id()
                );
                let binding = binding
                    .map(|text| crate::shortcuts::Binding::parse(&text))
                    .transpose()
                    .map_err(anyhow::Error::msg)?;
                plan.state.keybindings.set(action, binding);
                operation(&mut plan.operation, Operation::Keybindings)?;
            }
            Command::ResetKeybindings {} => {
                ensure!(!entering, "game.enter 不能恢复默认快捷键");
                ensure!(keybinding_modified.is_empty(), "恢复默认不能与单项修改同批");
                ensure!(!keybinding_reset, "重复恢复默认快捷键");
                keybinding_reset = true;
                plan.state.keybindings = crate::shortcuts::Keybindings::defaults();
                operation(&mut plan.operation, Operation::Keybindings)?;
            }
            Command::ColorPreference { slot, value } => {
                ensure!(!entering, "game.enter 不能修改配色偏好");
                ensure!(def.palette.contains_key(&slot), "未知配色槽位：{slot}");
                ensure!(!color_reset, "恢复默认配色不能与单项修改同批");
                ensure!(
                    color_modified.insert(slot.clone()),
                    "同一批重复修改配色槽位：{slot}"
                );
                let value = value
                    .map(|text| crate::color::Rgba::parse(&text))
                    .transpose()
                    .map_err(|reason| anyhow::anyhow!("槽位 {slot}：{reason}"))?;
                plan.state.colors.set(&slot, value);
                operation(&mut plan.operation, Operation::Colors)?;
            }
            Command::ResetColors {} => {
                ensure!(!entering, "game.enter 不能恢复默认配色");
                ensure!(color_modified.is_empty(), "恢复默认配色不能与单项修改同批");
                ensure!(!color_reset, "重复恢复默认配色");
                color_reset = true;
                plan.state.colors = crate::interface::palette::Colors::default();
                operation(&mut plan.operation, Operation::Colors)?;
            }
            Command::Save { slot, overwrite } => operation(
                &mut plan.operation,
                Operation::Save(SlotId::new(slot)?, overwrite),
            )?,
            Command::Load { slot } => {
                operation(&mut plan.operation, Operation::Load(SlotId::new(slot)?))?
            }
            Command::Restart { frame } => {
                operation(&mut plan.operation, Operation::Restart(frame))?
            }
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
                if matches!(
                    name.as_str(),
                    "muted" | "autosave" | "skip_all" | "fullscreen"
                ) {
                    let enabled = value
                        .as_bool()
                        .ok_or_else(|| anyhow::anyhow!("设置需要 bool"))?;
                    match name.as_str() {
                        "muted" => plan.settings.muted = enabled,
                        "autosave" => plan.settings.autosave = enabled,
                        "fullscreen" => plan.settings.fullscreen = enabled,
                        _ => plan.settings.skip_all = enabled,
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
    if matches!(plan.operation, Operation::Keybindings) {
        ensure!(
            !keybinding_modified.is_empty() || keybinding_reset,
            "快捷键事务缺少修改"
        );
        plan.state
            .keybindings
            .validate()
            .map_err(anyhow::Error::msg)?;
    } else {
        ensure!(
            keybinding_modified.is_empty() && !keybinding_reset,
            "快捷键修改事务状态错误"
        );
    }
    if matches!(plan.operation, Operation::Colors) {
        ensure!(
            !color_modified.is_empty() || color_reset,
            "配色事务缺少修改"
        );
    } else {
        ensure!(
            color_modified.is_empty() && !color_reset,
            "配色修改事务状态错误"
        );
    }
    ensure!(
        !(plan.capture.is_some() && plan.cancel_capture),
        "不能同时启动和取消快捷键捕获"
    );
    if plan.capture.is_some() || plan.cancel_capture {
        ensure!(
            matches!(plan.operation, Operation::None)
                && plan.ui_actions.is_empty()
                && plan.animations.is_empty()
                && plan.portraits.is_empty()
                && plan.live2d.is_empty()
                && plan.auto_play.is_none()
                && plan.transition.is_none()
                && plan.profile == *profile,
            "快捷键捕获不能混合持久 IO、演出或其他剧情操作"
        );
    }
    plan.state.refresh(def)?;
    plan.state.window_title = def.window_title.resolve(&plan.state.variables)?.to_owned();
    Ok(plan)
}
pub fn validate_images(def: &Definition, state: &State) -> Result<()> {
    fn walk(def: &Definition, state: &State, nodes: &[super::Node]) -> Result<()> {
        for node in nodes {
            let check = || -> Result<()> {
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
                Ok(())
            };
            check().map_err(|e| anyhow::anyhow!("{}：节点 {}：{e:#}", node.location, node.id))?;
            walk(def, state, &node.children)?;
        }
        Ok(())
    }
    for screen in def.screens.values() {
        walk(def, state, &screen.nodes)?;
    }
    resolved_regions(def, state)?;
    Ok(())
}

fn resolved_color_binds(
    def: &Definition,
    state: &State,
) -> Result<BTreeMap<String, crate::color::Rgba>> {
    fn walk(
        state: &State,
        nodes: &[super::Node],
        output: &mut BTreeMap<String, crate::color::Rgba>,
    ) -> Result<()> {
        for node in nodes {
            let check = |output: &mut BTreeMap<String, crate::color::Rgba>| -> Result<()> {
                if let Some(path) = &node.color_bind {
                    let color = if let Some(id) = path.strip_prefix("ui.") {
                        match state.variables.get(id) {
                            Some(VariableValue::String(text)) => {
                                crate::color::Rgba::parse(text).map_err(anyhow::Error::msg)?
                            }
                            _ => anyhow::bail!("color-bind 的 UI 变量类型错误"),
                        }
                    } else if let Some(id) = path.strip_prefix("colors.") {
                        *state
                            .resolved_colors
                            .get(id)
                            .ok_or_else(|| anyhow::anyhow!("未知配色槽位：{id}"))?
                    } else {
                        anyhow::bail!("color-bind 需要 ui 字符串或 colors 槽位")
                    };
                    output.insert(node.id.clone(), color);
                }
                if matches!(node.kind, super::Kind::ColorPicker { .. }) {
                    let path = node
                        .bind
                        .as_deref()
                        .ok_or_else(|| anyhow::anyhow!("color-picker 缺少 value 绑定"))?;
                    let id = path
                        .strip_prefix("ui.")
                        .ok_or_else(|| anyhow::anyhow!("color-picker 需要 ui 字符串"))?;
                    match state.variables.get(id) {
                        Some(VariableValue::String(text)) => {
                            crate::color::Rgba::parse(text).map_err(anyhow::Error::msg)?;
                        }
                        _ => anyhow::bail!("color-picker 的值类型错误"),
                    }
                }
                Ok(())
            };
            check(output)
                .map_err(|e| anyhow::anyhow!("{}：节点 {}：{e:#}", node.location, node.id))?;
            walk(state, &node.children, output)?;
        }
        Ok(())
    }
    let mut output = BTreeMap::new();
    for screen in def.screens.values() {
        walk(state, &screen.nodes, &mut output)?;
    }
    Ok(output)
}

fn resolved_regions(
    def: &Definition,
    state: &State,
) -> Result<BTreeMap<String, crate::image_layout::Region>> {
    fn walk(
        def: &Definition,
        state: &State,
        nodes: &[super::Node],
        output: &mut BTreeMap<String, crate::image_layout::Region>,
    ) -> Result<()> {
        for node in nodes {
            let result = (|| -> Result<()> {
                let region = if let Some(path) = &node.region_bind {
                    let Some(VariableValue::String(text)) =
                        state.variables.get(path.strip_prefix("ui.").unwrap_or(""))
                    else {
                        anyhow::bail!("region-bind 类型错误");
                    };
                    Some(crate::image_layout::Region::parse(text)?)
                } else {
                    node.region
                };
                if let Some(region) = region {
                    let image = node.image.as_deref().or_else(|| {
                        node.image_bind
                            .as_deref()
                            .and_then(|p| p.strip_prefix("ui."))
                            .and_then(|name| match state.variables.get(name) {
                                Some(VariableValue::String(id)) => Some(id.as_str()),
                                _ => None,
                            })
                    });
                    for id in [
                        image,
                        node.hover_image.as_deref(),
                        node.disabled_image.as_deref(),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        if let Some(size) = def.image_sizes.get(id) {
                            region.validate(*size)?;
                        }
                    }
                    output.insert(node.id.clone(), region);
                }
                Ok(())
            })();
            result.map_err(|e| anyhow::anyhow!("{}：节点 {}：{e:#}", node.location, node.id))?;
            walk(def, state, &node.children, output)?;
        }
        Ok(())
    }
    let mut output = BTreeMap::new();
    for screen in def.screens.values() {
        walk(def, state, &screen.nodes, &mut output)?;
    }
    Ok(output)
}
