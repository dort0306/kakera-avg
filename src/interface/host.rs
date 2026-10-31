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
    pub languages: crate::localization::Selection,
    pub localized_text: BTreeMap<String, String>,
    pub overflow_messages: BTreeMap<String, String>,
    pub localization_numbers: BTreeMap<String, i64>,
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
            languages: def.localization.selection(Default::default(), Vec::new()),
            localized_text: BTreeMap::new(),
            overflow_messages: BTreeMap::new(),
            localization_numbers: BTreeMap::new(),
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
        self.localization_numbers
            .insert("saves.page".into(), i64::from(self.slot_page));
        self.localization_numbers.insert("saves.pages".into(), 20);
        self.localization_numbers
            .insert("cg.total".into(), def.gallery.len() as i64);
        self.localization_numbers
            .entry("cg.unlocked".into())
            .or_insert_with(|| def.gallery.values().filter(|cg| cg.unlocked).count() as i64);
        self.localized_text.clear();
        self.overflow_messages.clear();
        fn localized(
            def: &Definition,
            state: &mut State,
            nodes: &[super::Node],
            bytes: &mut usize,
        ) -> Result<()> {
            for node in nodes {
                state.overflow_messages.insert(
                    node.id.clone(),
                    crate::i18n::format("host-interface-render-0001", &[("arg0", node.id.clone())]),
                );
                if let Some(binding) = &node.localized {
                    let text = binding
                        .resolve(
                            &def.localization,
                            &state.languages,
                            &state.variables,
                            &state.localization_numbers,
                        )
                        .map_err(|cause| anyhow::anyhow!("{}: {cause:#}", binding.location))?;
                    *bytes = bytes.saturating_add(text.len());
                    ensure!(
                        *bytes <= 64 * 1024 * 1024,
                        "{}",
                        crate::i18n::text("host-i18n-detail-0059")
                    );
                    state.localized_text.insert(node.id.clone(), text);
                }
                localized(def, state, &node.children, bytes)?;
            }
            Ok(())
        }
        let mut bytes = 0usize;
        for screen in def.screens.values() {
            localized(def, self, &screen.nodes, &mut bytes)?;
        }
        self.window_title = if let Some(binding) = &def.localized_title {
            let text = binding
                .resolve(
                    &def.localization,
                    &self.languages,
                    &self.variables,
                    &self.localization_numbers,
                )
                .map_err(|cause| anyhow::anyhow!("{}: {cause:#}", binding.location))?;
            ensure!(
                !text.trim().is_empty()
                    && text.chars().count() <= 256
                    && !text.chars().any(char::is_control),
                "{}",
                crate::i18n::text("host-i18n-detail-0060")
            );
            text
        } else {
            def.window_title.resolve(&self.variables)?.into()
        };
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
            "{}",
            crate::i18n::format("host-interface-host-0001", &[])
        );
        ensure!(
            profile
                .unlocked
                .iter()
                .all(|id| def.gallery.contains_key(id)),
            "{}",
            crate::i18n::format("host-interface-host-0002", &[])
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
    Locale {
        channel: String,
        #[serde(deserialize_with = "Option::deserialize")]
        locale: Option<String>,
    },
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
    Locales,
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
        (Operation::Locales, Operation::Locales)
            | (Operation::Settings, Operation::Settings)
            | (Operation::Profile, Operation::Profile)
            | (Operation::UiAudio, Operation::UiAudio)
            | (Operation::Keybindings, Operation::Keybindings)
            | (Operation::Colors, Operation::Colors)
    );
    ensure!(
        matches!(current, Operation::None) || aggregate,
        "{}",
        crate::i18n::format("host-interface-host-0003", &[])
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
    prepare_with_pages(def, state, settings, profile, commands, entering, None)
}
/// Same transaction with the current page counts of every paginated repeat. An
/// explicit page write beyond the current candidate page count rejects the batch.
pub fn prepare_with_pages(
    def: &Definition,
    state: &State,
    settings: &Settings,
    profile: &Profile,
    commands: Vec<Command>,
    entering: bool,
    page_limits: Option<&BTreeMap<String, i64>>,
) -> Result<Plan> {
    ensure!(
        commands.len() <= 32,
        "{}",
        crate::i18n::format("host-interface-host-0004", &[])
    );
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
    let mut locale_modified = BTreeSet::new();
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
                "{}",
                crate::i18n::format("host-interface-host-0005", &[])
            );
        }
        match command {
            Command::Locale { channel, locale } => {
                ensure!(
                    def.localization.enabled(),
                    "{}",
                    crate::i18n::text("host-i18n-detail-0061")
                );
                ensure!(
                    locale_modified.insert(channel.clone()),
                    "{}",
                    crate::i18n::text("host-i18n-detail-0062")
                );
                if let Some(id) = &locale {
                    crate::localization::canonical(id)?;
                    ensure!(
                        match channel.as_str() {
                            "text" => def.localization.texts.contains_key(id),
                            "voice" => def.localization.voices.contains_key(id),
                            _ => false,
                        },
                        "{}",
                        crate::i18n::text("host-i18n-detail-0063")
                    );
                }
                match channel.as_str() {
                    "text" => plan.state.languages.requests.text = locale,
                    "voice" => plan.state.languages.requests.voice = locale,
                    _ => anyhow::bail!("{}", crate::i18n::text("host-i18n-detail-0064")),
                };
                plan.state.languages = def.localization.selection(
                    plan.state.languages.requests.clone(),
                    plan.state.languages.system.clone(),
                );
                operation(&mut plan.operation, Operation::Locales)?;
            }
            Command::PlaySound { id } => {
                ensure!(
                    !entering && def.media.sound_exists(&id),
                    "{}",
                    crate::i18n::format("host-interface-host-0006", &[])
                );
                plan.ui_actions.push(super::presentation::Action::Sound(id));
            }
            Command::Sequence { id } => {
                ensure!(
                    !entering && def.media.sequences.contains_key(&id),
                    "{}",
                    crate::i18n::format("host-interface-host-0007", &[])
                );
                ensure!(
                    !plan
                        .ui_actions
                        .iter()
                        .any(|a| matches!(a, super::presentation::Action::Sequence(_))),
                    "{}",
                    crate::i18n::format("host-interface-host-0008", &[])
                );
                plan.ui_actions
                    .push(super::presentation::Action::Sequence(id));
            }
            Command::SoundEnabled { enabled } => {
                ensure!(
                    !entering,
                    "{}",
                    crate::i18n::format("host-interface-host-0009", &[])
                );
                operation(&mut plan.operation, Operation::UiAudio)?;
                plan.ui_actions
                    .push(super::presentation::Action::Enabled(enabled));
            }
            Command::SoundActor { actor, enabled } => {
                ensure!(
                    !entering && def.media.actors.contains(&actor),
                    "{}",
                    crate::i18n::format("host-interface-host-0010", &[])
                );
                operation(&mut plan.operation, Operation::UiAudio)?;
                plan.ui_actions
                    .push(super::presentation::Action::Actor(actor, enabled));
            }
            Command::SoundExclude { asset, excluded } => {
                ensure!(
                    !entering && def.media.audio.contains(&asset),
                    "{}",
                    crate::i18n::format("host-interface-host-0011", &[])
                );
                operation(&mut plan.operation, Operation::UiAudio)?;
                plan.ui_actions
                    .push(super::presentation::Action::Exclude(asset, excluded));
            }
            Command::Present { screen } => {
                ensure!(
                    !entering,
                    "{}",
                    crate::i18n::format("host-interface-host-0012", &[])
                );
                let target = def.screens.get(&screen).ok_or_else(|| {
                    anyhow::anyhow!("{}", crate::i18n::format("host-interface-host-0013", &[]))
                })?;
                if target.modal {
                    ensure!(
                        plan.state.stack.len() < 8 && !plan.state.stack.contains(&screen),
                        "{}",
                        crate::i18n::format("host-interface-host-0014", &[])
                    );
                    plan.state.stack.push(screen);
                } else {
                    plan.state.stack = vec![screen];
                }
                operation(&mut plan.operation, Operation::Present)?;
            }
            Command::Resume { frame } => {
                ensure!(
                    !entering,
                    "{}",
                    crate::i18n::format("host-interface-host-0015", &[])
                );
                operation(&mut plan.operation, Operation::Resume(frame))?;
            }

            Command::Live2d { slot, motion } => {
                let slot = match slot.as_str() {
                    "left" => crate::model::Slot::Left,
                    "center" => crate::model::Slot::Center,
                    "right" => crate::model::Slot::Right,
                    _ => anyhow::bail!("{}", crate::i18n::format("host-interface-host-0016", &[])),
                };
                ensure!(
                    motion.len() <= 256,
                    "{}",
                    crate::i18n::format("host-interface-host-0017", &[])
                );
                plan.live2d.push((slot, motion));
            }
            Command::SlotsPage { page } => {
                ensure!(
                    (1..=crate::save::PAGE_COUNT).contains(&page),
                    "{}",
                    crate::i18n::format("host-interface-host-0018", &[])
                );
                plan.state.slot_page = page;
            }
            Command::LoadAuto { slot } => {
                ensure!(
                    (1..=crate::save::AUTO_COUNT).contains(&slot),
                    "{}",
                    crate::i18n::format("host-interface-host-0019", &[])
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
                    _ => anyhow::bail!(
                        "{}",
                        crate::i18n::format(
                            "host-interface-host-0020",
                            &[("arg0", slot.to_string())]
                        )
                    ),
                };
                crate::effects::validate_portrait(animation).map_err(anyhow::Error::msg)?;
                plan.portraits.push((slot, animation));
            }
            Command::Transition { transition } => {
                ensure!(
                    entering,
                    "{}",
                    crate::i18n::format("host-interface-host-0021", &[])
                );
                crate::effects::validate_transition(transition).map_err(anyhow::Error::msg)?;
                ensure!(
                    plan.transition.replace(transition).is_none(),
                    "{}",
                    crate::i18n::format("host-interface-host-0022", &[])
                );
            }
            Command::Animate { target, animation } => {
                fn valid(nodes: &[super::Node], target: &str, repeated: bool) -> bool {
                    nodes.iter().any(|n| {
                        (!repeated
                            && n.id == target
                            && (matches!(n.kind, super::Kind::Scene)
                                || matches!(n.kind, super::Kind::Panel)
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
                    "{}",
                    crate::i18n::format(
                        "host-interface-host-0023",
                        &[("arg0", target.to_string())]
                    )
                );
                crate::effects::validate_animation(animation).map_err(anyhow::Error::msg)?;
                plan.animations.push((target, animation));
            }
            Command::Open { screen } => {
                let target = def.screens.get(&screen).ok_or_else(|| {
                    anyhow::anyhow!(
                        "{}",
                        crate::i18n::format(
                            "host-interface-host-0024",
                            &[("arg0", screen.to_string())]
                        )
                    )
                })?;
                if target.modal {
                    ensure!(
                        plan.state.stack.len() < 8 && !plan.state.stack.contains(&screen),
                        "{}",
                        crate::i18n::format("host-interface-host-0025", &[])
                    );
                    plan.state.stack.push(screen);
                } else {
                    plan.state.stack = vec![screen];
                }
            }
            Command::Close => {
                ensure!(
                    plan.state.stack.len() > 1,
                    "{}",
                    crate::i18n::format("host-interface-host-0026", &[])
                );
                plan.state.stack.pop();
            }
            Command::Quit {} => operation(&mut plan.operation, Operation::Quit)?,
            Command::SetUi { name, value } => {
                let decl = def.variables.get(&name).ok_or_else(|| {
                    anyhow::anyhow!(
                        "{}",
                        crate::i18n::format(
                            "host-interface-host-0027",
                            &[("arg0", name.to_string())]
                        )
                    )
                })?;
                let value = value.value()?;
                rules::validate_value(decl, &value).map_err(anyhow::Error::msg)?;
                if let (Some(limits), crate::model::VariableValue::Int(page)) =
                    (page_limits, &value)
                    && let Some(pages) = limits.get(&name)
                {
                    ensure!(
                        *page <= *pages,
                        "{}",
                        crate::i18n::format(
                            "host-interface-pagination-0011",
                            &[("arg0", page.to_string()), ("arg1", pages.to_string())]
                        )
                    );
                }
                plan.state.variables.insert(name, value);
            }
            Command::CaptureShortcut {
                action,
                result_event,
            } => {
                ensure!(
                    !entering,
                    "{}",
                    crate::i18n::format("host-interface-host-0028", &[])
                );
                let action = crate::shortcuts::Action::from_id(&action).ok_or_else(|| {
                    anyhow::anyhow!(
                        "{}",
                        crate::i18n::format(
                            "host-interface-host-0029",
                            &[("arg0", action.to_string())]
                        )
                    )
                })?;
                ensure!(
                    def.host_handlers.contains(&result_event),
                    "{}",
                    crate::i18n::format(
                        "host-interface-host-0030",
                        &[("arg0", result_event.to_string())]
                    )
                );
                ensure!(
                    plan.capture.is_none() && !plan.cancel_capture,
                    "{}",
                    crate::i18n::format("host-interface-host-0031", &[])
                );
                plan.capture = Some(CaptureRequest {
                    action,
                    result_event,
                });
            }
            Command::CancelShortcutCapture {} => {
                ensure!(
                    !entering,
                    "{}",
                    crate::i18n::format("host-interface-host-0032", &[])
                );
                ensure!(
                    !plan.cancel_capture,
                    "{}",
                    crate::i18n::format("host-interface-host-0033", &[])
                );
                plan.cancel_capture = true;
            }
            Command::Keybinding { action, binding } => {
                ensure!(
                    !entering,
                    "{}",
                    crate::i18n::format("host-interface-host-0034", &[])
                );
                ensure!(
                    !keybinding_reset,
                    "{}",
                    crate::i18n::format("host-interface-host-0035", &[])
                );
                let action = crate::shortcuts::Action::from_id(&action).ok_or_else(|| {
                    anyhow::anyhow!(
                        "{}",
                        crate::i18n::format(
                            "host-interface-host-0036",
                            &[("arg0", action.to_string())]
                        )
                    )
                })?;
                ensure!(
                    keybinding_modified.insert(action),
                    "{}",
                    crate::i18n::format(
                        "host-interface-host-0037",
                        &[("arg0", action.id().to_string())]
                    )
                );
                let binding = binding
                    .map(|text| crate::shortcuts::Binding::parse(&text))
                    .transpose()
                    .map_err(anyhow::Error::msg)?;
                plan.state.keybindings.set(action, binding);
                operation(&mut plan.operation, Operation::Keybindings)?;
            }
            Command::ResetKeybindings {} => {
                ensure!(
                    !entering,
                    "{}",
                    crate::i18n::format("host-interface-host-0038", &[])
                );
                ensure!(
                    keybinding_modified.is_empty(),
                    "{}",
                    crate::i18n::format("host-interface-host-0039", &[])
                );
                ensure!(
                    !keybinding_reset,
                    "{}",
                    crate::i18n::format("host-interface-host-0040", &[])
                );
                keybinding_reset = true;
                plan.state.keybindings = crate::shortcuts::Keybindings::defaults();
                operation(&mut plan.operation, Operation::Keybindings)?;
            }
            Command::ColorPreference { slot, value } => {
                ensure!(
                    !entering,
                    "{}",
                    crate::i18n::format("host-interface-host-0041", &[])
                );
                ensure!(
                    def.palette.contains_key(&slot),
                    "{}",
                    crate::i18n::format("host-interface-host-0042", &[("arg0", slot.to_string())])
                );
                ensure!(
                    !color_reset,
                    "{}",
                    crate::i18n::format("host-interface-host-0043", &[])
                );
                ensure!(
                    color_modified.insert(slot.clone()),
                    "{}",
                    crate::i18n::format("host-interface-host-0044", &[("arg0", slot.to_string())])
                );
                let value = value
                    .map(|text| crate::color::Rgba::parse(&text))
                    .transpose()
                    .map_err(|reason| {
                        anyhow::anyhow!(
                            "{}",
                            crate::i18n::format(
                                "host-interface-host-0045",
                                &[("arg0", slot.to_string()), ("arg1", reason.to_string())]
                            )
                        )
                    })?;
                plan.state.colors.set(&slot, value);
                operation(&mut plan.operation, Operation::Colors)?;
            }
            Command::ResetColors {} => {
                ensure!(
                    !entering,
                    "{}",
                    crate::i18n::format("host-interface-host-0046", &[])
                );
                ensure!(
                    color_modified.is_empty(),
                    "{}",
                    crate::i18n::format("host-interface-host-0047", &[])
                );
                ensure!(
                    !color_reset,
                    "{}",
                    crate::i18n::format("host-interface-host-0048", &[])
                );
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
                ensure!(
                    def.gallery.contains_key(&id),
                    "{}",
                    crate::i18n::format("host-interface-host-0049", &[("arg0", id.to_string())])
                );
                if plan.profile.unlocked.insert(id) {
                    operation(&mut plan.operation, Operation::Profile)?;
                }
            }
            Command::Setting { name, value } => {
                if matches!(
                    name.as_str(),
                    "muted" | "autosave" | "skip_all" | "fullscreen"
                ) {
                    let enabled = value.as_bool().ok_or_else(|| {
                        anyhow::anyhow!("{}", crate::i18n::format("host-interface-host-0050", &[]))
                    })?;
                    match name.as_str() {
                        "muted" => plan.settings.muted = enabled,
                        "autosave" => plan.settings.autosave = enabled,
                        "fullscreen" => plan.settings.fullscreen = enabled,
                        _ => plan.settings.skip_all = enabled,
                    }
                } else {
                    let value = value.as_f64().ok_or_else(|| {
                        anyhow::anyhow!("{}", crate::i18n::format("host-interface-host-0051", &[]))
                    })? as f32;
                    match name.as_str() {
                        "auto_delay" => plan.settings.auto_delay = value,
                        "video_volume" => plan.settings.video_volume = value,
                        "text_speed" => plan.settings.text_speed = value,
                        "bgm_volume" => plan.settings.bgm_volume = value,
                        "voice_volume" => plan.settings.voice_volume = value,
                        "se_volume" => plan.settings.se_volume = value,
                        _ => anyhow::bail!(
                            "{}",
                            crate::i18n::format(
                                "host-interface-host-0052",
                                &[("arg0", name.to_string())]
                            )
                        ),
                    }
                }
                plan.settings.validate()?;
                operation(&mut plan.operation, Operation::Settings)?;
            }
            Command::Message { text } => {
                ensure!(
                    text.len() <= 4096,
                    "{}",
                    crate::i18n::format("host-interface-host-0053", &[])
                );
                plan.message = Some(text);
            }
        }
    }
    if matches!(plan.operation, Operation::Keybindings) {
        ensure!(
            !keybinding_modified.is_empty() || keybinding_reset,
            "{}",
            crate::i18n::format("host-interface-host-0054", &[])
        );
        plan.state
            .keybindings
            .validate()
            .map_err(anyhow::Error::msg)?;
    } else {
        ensure!(
            keybinding_modified.is_empty() && !keybinding_reset,
            "{}",
            crate::i18n::format("host-interface-host-0055", &[])
        );
    }
    if matches!(plan.operation, Operation::Colors) {
        ensure!(
            !color_modified.is_empty() || color_reset,
            "{}",
            crate::i18n::format("host-interface-host-0056", &[])
        );
    } else {
        ensure!(
            color_modified.is_empty() && !color_reset,
            "{}",
            crate::i18n::format("host-interface-host-0057", &[])
        );
    }
    ensure!(
        !(plan.capture.is_some() && plan.cancel_capture),
        "{}",
        crate::i18n::format("host-interface-host-0058", &[])
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
            "{}",
            crate::i18n::format("host-interface-host-0059", &[])
        );
    }
    plan.state.localization_numbers.insert(
        "cg.unlocked".into(),
        def.gallery
            .keys()
            .filter(|id| plan.profile.unlocked.contains(*id))
            .count() as i64,
    );
    plan.state.refresh(def)?;
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
                        anyhow::bail!("{}", crate::i18n::format("host-interface-host-0060", &[]))
                    };
                    ensure!(
                        def.images.contains(image),
                        "{}",
                        crate::i18n::format(
                            "host-interface-host-0061",
                            &[("arg0", image.to_string())]
                        )
                    );
                }
                Ok(())
            };
            check().map_err(|e| {
                anyhow::anyhow!(
                    "{}",
                    crate::i18n::format(
                        "host-interface-host-0062",
                        &[
                            ("arg0", node.location.to_string()),
                            ("arg1", node.id.to_string()),
                            ("arg2", format!("{:#}", e))
                        ]
                    )
                )
            })?;
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
                            _ => anyhow::bail!(
                                "{}",
                                crate::i18n::format("host-interface-host-0063", &[])
                            ),
                        }
                    } else if let Some(id) = path.strip_prefix("colors.") {
                        *state.resolved_colors.get(id).ok_or_else(|| {
                            anyhow::anyhow!(
                                "{}",
                                crate::i18n::format(
                                    "host-interface-host-0064",
                                    &[("arg0", id.to_string())]
                                )
                            )
                        })?
                    } else {
                        anyhow::bail!("{}", crate::i18n::format("host-interface-host-0065", &[]))
                    };
                    output.insert(node.id.clone(), color);
                }
                if matches!(node.kind, super::Kind::ColorPicker { .. }) {
                    let path = node.bind.as_deref().ok_or_else(|| {
                        anyhow::anyhow!("{}", crate::i18n::format("host-interface-host-0066", &[]))
                    })?;
                    let id = path.strip_prefix("ui.").ok_or_else(|| {
                        anyhow::anyhow!("{}", crate::i18n::format("host-interface-host-0067", &[]))
                    })?;
                    match state.variables.get(id) {
                        Some(VariableValue::String(text)) => {
                            crate::color::Rgba::parse(text).map_err(anyhow::Error::msg)?;
                        }
                        _ => anyhow::bail!(
                            "{}",
                            crate::i18n::format("host-interface-host-0068", &[])
                        ),
                    }
                }
                Ok(())
            };
            check(output).map_err(|e| {
                anyhow::anyhow!(
                    "{}",
                    crate::i18n::format(
                        "host-interface-host-0069",
                        &[
                            ("arg0", node.location.to_string()),
                            ("arg1", node.id.to_string()),
                            ("arg2", format!("{:#}", e))
                        ]
                    )
                )
            })?;
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
                        anyhow::bail!("{}", crate::i18n::format("host-interface-host-0070", &[]));
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
            result.map_err(|e| {
                anyhow::anyhow!(
                    "{}",
                    crate::i18n::format(
                        "host-interface-host-0071",
                        &[
                            ("arg0", node.location.to_string()),
                            ("arg1", node.id.to_string()),
                            ("arg2", format!("{:#}", e))
                        ]
                    )
                )
            })?;
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
