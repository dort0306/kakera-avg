//! Owned interface data and strict authoring contract. No interpreter or GPU objects.
use crate::{
    loader::{check, elements, error, required, single},
    model::{AssetKind, Story, VariableDecl, VariableValue, Variables},
};
use anyhow::{Context, Result, ensure};
use roxmltree::{Document, Node as XmlNode};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub mod data;
pub mod host;
pub mod palette;
pub mod presentation;
pub mod render;
pub mod source;

#[derive(Clone, Debug)]
pub struct PaletteSlot {
    pub initial: crate::color::Rgba,
    pub location: String,
}

#[derive(Clone, Debug)]
pub struct Definition {
    pub localization: crate::localization::SharedCatalog,
    pub localized_title: Option<crate::localization::TextBinding>,
    /// Lifecycle dispatch keys keyed by `<category>:<resolved id>`.
    pub lifecycle: BTreeMap<String, UiEventKey>,
    pub media: presentation::Catalog,
    pub window_title: WindowTitle,
    pub size: [f32; 2],
    pub initial: String,
    pub variables: BTreeMap<String, VariableDecl>,
    pub gallery: BTreeMap<String, Cg>,
    pub palette: BTreeMap<String, PaletteSlot>,
    pub screens: BTreeMap<String, Screen>,
    pub images: BTreeSet<String>,
    pub nine_patches: BTreeSet<String>,
    pub image_sizes: BTreeMap<String, [u32; 2]>,
    /// Ordinary `ui('$host').bind(result, callback)` capture result events.
    pub host_handlers: BTreeSet<String>,
    pub enter: bool,
}
/// Namespace of one JavaScript registration. The virtual `$game`/`$host` targets
/// and the two frame lifecycle phases are explicit categories, never inferred from
/// an event name string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EventCategory {
    Ui,
    Game,
    Host,
    FrameEnter,
    FrameExit,
}
impl EventCategory {
    pub fn id(self) -> &'static str {
        match self {
            Self::Ui => "ui",
            Self::Game => "game",
            Self::Host => "host",
            Self::FrameEnter => "frame.enter",
            Self::FrameExit => "frame.exit",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ui" => Some(Self::Ui),
            "game" => Some(Self::Game),
            "host" => Some(Self::Host),
            "frame.enter" => Some(Self::FrameEnter),
            "frame.exit" => Some(Self::FrameExit),
            _ => None,
        }
    }
    pub fn lifecycle(self) -> bool {
        matches!(self, Self::FrameEnter | Self::FrameExit)
    }
}
/// Structured dispatch key `(category, target, event)`; never concatenated text.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UiEventKey {
    pub category: EventCategory,
    pub target: String,
    pub event: String,
}
impl UiEventKey {
    pub fn ui(target: impl Into<String>, event: impl Into<String>) -> Self {
        Self {
            category: EventCategory::Ui,
            target: target.into(),
            event: event.into(),
        }
    }
    pub fn game() -> Self {
        Self {
            category: EventCategory::Game,
            target: "$game".into(),
            event: "game.enter".into(),
        }
    }
    pub fn host(event: impl Into<String>) -> Self {
        Self {
            category: EventCategory::Host,
            target: "$host".into(),
            event: event.into(),
        }
    }
    pub fn lifecycle(category: EventCategory, target: impl Into<String>) -> Self {
        Self {
            category,
            target: target.into(),
            event: String::new(),
        }
    }
    pub fn describe(&self) -> String {
        if self.category.lifecycle() {
            format!("{}{}", self.category.id(), self.target)
        } else {
            format!("{}/{}", self.target, self.event)
        }
    }
}
/// One JavaScript registration with its author call site.
#[derive(Clone, Debug)]
pub struct Registration {
    pub key: UiEventKey,
    pub location: String,
}
impl Registration {
    pub fn new(
        category: EventCategory,
        target: impl Into<String>,
        event: impl Into<String>,
    ) -> Self {
        Self {
            key: UiEventKey {
                category,
                target: target.into(),
                event: event.into(),
            },
            location: "<test>".into(),
        }
    }
    pub fn ui(target: impl Into<String>, event: impl Into<String>) -> Self {
        Self::new(EventCategory::Ui, target, event)
    }
}
#[derive(Clone, Debug)]
pub struct WindowTitle {
    value: String,
    bound: bool,
    location: String,
}
impl WindowTitle {
    pub(crate) fn resolve<'a>(&'a self, variables: &'a Variables) -> Result<&'a str> {
        let title = if self.bound {
            match variables.get(&self.value) {
                Some(VariableValue::String(title)) => title.as_str(),
                _ => anyhow::bail!(
                    "{}",
                    crate::i18n::format(
                        "host-interface-0001",
                        &[("arg0", self.location.to_string())]
                    )
                ),
            }
        } else {
            &self.value
        };
        ensure!(
            !title.trim().is_empty()
                && title.chars().count() <= 256
                && !title.chars().any(char::is_control),
            "{}",
            crate::i18n::format(
                "host-interface-0002",
                &[("arg0", self.location.to_string())]
            )
        );
        Ok(title)
    }
}
#[derive(Clone, Debug)]
pub struct Cg {
    pub image: String,
    pub title: String,
    pub unlocked: bool,
}
#[derive(Clone, Debug)]
pub struct Screen {
    pub location: String,
    pub sequence: Option<String>,
    pub modal: bool,
    pub pause: bool,
    pub background_click: Option<String>,
    pub background_right_click: Option<String>,
    pub nodes: Vec<Node>,
}
#[derive(Clone, Debug)]
pub enum Kind {
    Scene,
    Panel,
    Image,
    NinePatch,
    Label,
    Button,
    Slider {
        min: f32,
        max: f32,
    },
    Toggle,
    Repeat {
        source: String,
        columns: usize,
        cell: [f32; 2],
        gap: f32,
    },
    ColorPicker {
        alpha: bool,
    },
}
#[derive(Clone, Debug)]
pub struct Node {
    pub localized: Option<crate::localization::TextBinding>,
    pub sound: Option<String>,
    pub route: Option<UiRoute>,
    pub location: String,
    pub region: Option<crate::image_layout::Region>,
    pub region_bind: Option<String>,
    pub color_bind: Option<String>,
    pub density: f32,
    pub effect: crate::model::effects::ImageEffect,
    pub id: String,
    pub kind: Kind,
    pub rect: [f32; 4],
    pub color: [u8; 4],
    pub font: f32,
    pub radius: f32,
    pub text: String,
    pub bind: Option<String>,
    pub image: Option<String>,
    pub image_bind: Option<String>,
    pub hover_image: Option<String>,
    pub disabled_image: Option<String>,
    pub fit: String,
    pub visible: String,
    pub enabled: String,
    pub event: Option<String>,
    pub children: Vec<Node>,
}
/// One host-created ordinary event. The structured registration key travels with the
/// event so the adapter resolves `(category, target, event)` without trusting prefixes.
#[derive(Clone, Debug)]
pub struct Event {
    pub sound: Option<String>,
    pub route: Option<UiRoute>,
    pub key: UiEventKey,
    pub item: Value,
    pub value: Value,
}
impl Event {
    pub fn ui(target: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            sound: None,
            route: None,
            key: UiEventKey::ui(target, name),
            item: Value::Null,
            value: Value::Null,
        }
    }
    pub fn lifecycle(category: EventCategory, target: impl Into<String>) -> Self {
        Self {
            sound: None,
            route: None,
            key: UiEventKey::lifecycle(category, target),
            item: Value::Null,
            value: Value::Null,
        }
    }
    pub fn game() -> Self {
        Self {
            sound: None,
            route: None,
            key: UiEventKey::game(),
            item: Value::Null,
            value: Value::Null,
        }
    }
    pub fn host(name: impl Into<String>) -> Self {
        Self {
            sound: None,
            route: None,
            key: UiEventKey::host(name),
            item: Value::Null,
            value: Value::Null,
        }
    }
    /// Author event name; empty for route-only and sound-only controls.
    pub fn name(&self) -> &str {
        &self.key.event
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiRoute {
    Open(String),
    Close,
}
impl UiRoute {
    pub fn command(&self) -> host::Command {
        match self {
            Self::Open(screen) => host::Command::Open {
                screen: screen.clone(),
            },
            Self::Close => host::Command::Close,
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct Data {
    pub values: Value,
    pub lists: BTreeMap<String, Vec<Value>>,
}
impl Data {
    pub fn get<'a>(&'a self, path: &str, item: &'a Value) -> Option<&'a Value> {
        for prefix in ["audio.actor.", "audio.excluded."] {
            if let Some(id) = path.strip_prefix(prefix) {
                return self
                    .values
                    .get("audio")?
                    .get(if prefix == "audio.actor." {
                        "actor"
                    } else {
                        "excluded"
                    })?
                    .get(id);
            }
        }
        let (mut value, tail) = if let Some(tail) = path.strip_prefix("item.") {
            (item, tail)
        } else {
            (&self.values, path)
        };
        for part in tail.split('.') {
            value = value.get(part)?;
        }
        Some(value)
    }
    pub fn flag(&self, path: &str, item: &Value) -> bool {
        match path {
            "true" => true,
            "false" => false,
            _ => {
                if let Some(path) = path.strip_prefix("not:") {
                    !self.flag(path, item)
                } else {
                    self.get(path, item)
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                }
            }
        }
    }
}

impl Definition {
    pub fn initial_variables(&self) -> Variables {
        self.variables
            .iter()
            .map(|(id, decl)| (id.clone(), decl.initial.clone()))
            .collect()
    }
    pub fn text(&self) -> String {
        fn append(nodes: &[Node], text: &mut String) {
            for node in nodes {
                text.push_str(&node.text);
                append(&node.children, text);
            }
        }
        let mut text = String::new();
        for screen in self.screens.values() {
            append(&screen.nodes, &mut text);
        }
        for cg in self.gallery.values() {
            text.push_str(&cg.title);
        }
        for decl in self.variables.values() {
            if let VariableValue::String(value) = &decl.initial {
                text.push_str(value);
            }
        }
        text
    }
}

const LIFECYCLE_PREFIXES: [&str; 4] = [
    "frame.enter:",
    "frame.exit:",
    "screen.open:",
    "screen.close:",
];
/// Fixed virtual targets; reserved everywhere in a real XML UI.
fn reserved_target(id: &str) -> bool {
    matches!(id, "$game" | "$host")
}
/// `game.enter`, the empty name and the historical lifecycle encodings are never
/// ordinary XML events or ordinary registration names.
pub fn reserved_event(event: &str) -> bool {
    event.is_empty()
        || event == "game.enter"
        || LIFECYCLE_PREFIXES
            .iter()
            .any(|prefix| event.starts_with(prefix))
}
fn number(file: &str, node: XmlNode<'_, '_>, key: &str, default: Option<f32>) -> Result<f32> {
    let value = match node.attribute(key) {
        Some(text) => text.parse().map_err(|_| {
            error(
                file,
                node,
                crate::i18n::format("host-interface-0003", &[("arg0", key.to_string())]),
            )
        })?,
        None => default.ok_or_else(|| {
            error(
                file,
                node,
                crate::i18n::format("host-interface-0004", &[("arg0", key.to_string())]),
            )
        })?,
    };
    if !f32::is_finite(value) || value.abs() > 8192.0 {
        return Err(error(
            file,
            node,
            crate::i18n::format("host-interface-0005", &[("arg0", key.to_string())]),
        ));
    }
    Ok(value)
}
fn boolean(file: &str, node: XmlNode<'_, '_>, key: &str, default: bool) -> Result<bool> {
    match node.attribute(key) {
        None => Ok(default),
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        _ => Err(error(
            file,
            node,
            crate::i18n::format("host-interface-0006", &[("arg0", key.to_string())]),
        )),
    }
}
fn image(file: &str, node: XmlNode<'_, '_>, id: &str, story: &Story) -> Result<()> {
    if !story
        .assets
        .get(id)
        .is_some_and(|asset| matches!(asset.kind, AssetKind::Image | AssetKind::Cg))
    {
        return Err(error(
            file,
            node,
            crate::i18n::format("host-interface-0007", &[("arg0", id.to_string())]),
        ));
    }
    Ok(())
}
fn color(file: &str, node: XmlNode<'_, '_>, default: [u8; 4]) -> Result<[u8; 4]> {
    let Some(text) = node.attribute("color") else {
        return Ok(default);
    };
    let Some(hex) = text.strip_prefix('#') else {
        return Err(error(file, node, crate::i18n::text("host-interface-0028")));
    };
    if ![6, 8].contains(&hex.len()) || !hex.is_ascii() {
        return Err(error(file, node, crate::i18n::text("host-interface-0029")));
    }
    let mut color = [255; 4];
    for (index, value) in color.iter_mut().enumerate().take(hex.len() / 2) {
        *value = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| error(file, node, crate::i18n::text("host-interface-0030")))?;
    }
    Ok(color)
}

#[derive(PartialEq)]
enum ValueKind {
    Text,
    Bool,
    Number,
}
fn binding(
    path: &str,
    vars: &BTreeMap<String, VariableDecl>,
    source: Option<&str>,
) -> Option<ValueKind> {
    if path.len() > 128 {
        return None;
    }
    if let Some(path) = path.strip_prefix("not:") {
        if path.starts_with("not:") {
            return None;
        }
        return (binding(path, vars, source) == Some(ValueKind::Bool)).then_some(ValueKind::Bool);
    }
    if ["true", "false"].contains(&path) {
        return Some(ValueKind::Bool);
    }
    if let Some(id) = path.strip_prefix("ui.") {
        return vars.get(id).map(|decl| match decl.initial {
            VariableValue::Int(_) => ValueKind::Number,
            VariableValue::Bool(_) => ValueKind::Bool,
            VariableValue::String(_) => ValueKind::Text,
        });
    }
    if matches!(path, "cg.total" | "cg.unlocked") {
        return Some(ValueKind::Number);
    }
    if path == "cg.progress" {
        return Some(ValueKind::Text);
    }
    if let Some(id) = path.strip_prefix("game.") {
        return match id {
            "frameId" | "cgId" | "speaker" | "text" | "continueText" | "autoText" => {
                Some(ValueKind::Text)
            }
            "complete" | "canAdvance" | "ended" | "noChoices" | "transitioning" | "autoPlaying"
            | "live2dEnabled" => Some(ValueKind::Bool),
            _ => None,
        };
    }
    if path == "audio.enabled"
        || path
            .strip_prefix("audio.excluded.")
            .is_some_and(|id| !id.is_empty())
        || path
            .strip_prefix("audio.actor.")
            .is_some_and(|id| !id.is_empty())
    {
        return Some(ValueKind::Bool);
    }
    if let Some(id) = path.strip_prefix("locale.") {
        return match id {
            "enabled" => Some(ValueKind::Bool),
            "textRequested" | "voiceRequested" | "text" | "voice" | "textSource"
            | "voiceSource" => Some(ValueKind::Text),
            _ => None,
        };
    }
    if let Some(id) = path.strip_prefix("settings.") {
        return match id {
            "muted" | "autosave" | "skip_all" | "fullscreen" => Some(ValueKind::Bool),
            "text_speed" | "bgm_volume" | "voice_volume" | "se_volume" | "auto_delay"
            | "video_volume" => Some(ValueKind::Number),
            _ => None,
        };
    }
    if let Some(id) = path.strip_prefix("saves.") {
        return match id {
            "page" | "pages" => Some(ValueKind::Number),
            "pageText" => Some(ValueKind::Text),
            "canPrev" | "canNext" => Some(ValueKind::Bool),
            _ => None,
        };
    }
    if let Some(id) = path.strip_prefix("shortcuts.") {
        return match id {
            "advanceLabel"
            | "fastForwardLabel"
            | "skipSequenceLabel"
            | "captureText"
            | "bindings.advance"
            | "bindings.fast_forward"
            | "bindings.skip_sequence" => Some(ValueKind::Text),
            "captureActive" => Some(ValueKind::Bool),
            _ => None,
        };
    }
    let source = source.map(|s| if s == "autosaves" { "slots" } else { s });
    let field = path.strip_prefix("item.")?;
    match (source?, field) {
        ("textLocales" | "voiceLocales", "selected" | "default") => Some(ValueKind::Bool),
        ("textLocales" | "voiceLocales", "id" | "label") => Some(ValueKind::Text),
        ("slots", "slot") => Some(ValueKind::Number),
        ("slots", "exists" | "loadable" | "hasScreenshot") | ("gallery", "unlocked") => {
            Some(ValueKind::Bool)
        }
        ("slots", "title" | "preview" | "excerpt" | "savedAt" | "image")
        | ("choices", "id" | "text")
        | ("history", "id" | "speaker" | "text" | "choice")
        | ("gallery", "id" | "title" | "image")
        | ("variables", "id" | "label" | "value") => Some(ValueKind::Text),
        _ => None,
    }
}

struct Parser<'a> {
    catalog: &'a crate::localization::Catalog,
    file: &'a str,
    story: &'a Story,
    vars: &'a BTreeMap<String, VariableDecl>,
    palette: &'a BTreeMap<String, PaletteSlot>,
    ids: BTreeSet<String>,
    count: usize,
    pickers: usize,
}
impl Parser<'_> {
    fn nodes(
        &mut self,
        parent: XmlNode<'_, '_>,
        depth: usize,
        source: Option<&str>,
    ) -> Result<Vec<Node>> {
        let mut nodes = Vec::new();
        for xml in elements(parent) {
            self.count += 1;
            if depth > 8 || self.count > 1024 {
                return Err(error(
                    self.file,
                    xml,
                    crate::i18n::text("host-interface-0031"),
                ));
            }
            let tag = xml.tag_name().name();
            let container = ["panel", "nine-patch", "repeat"].contains(&tag);
            let mut attrs = vec![
                "id", "x", "y", "width", "height", "color", "visible", "enabled",
            ];
            match tag {
                "scene" => {}
                "panel" => attrs.extend([
                    "image",
                    "image-bind",
                    "fit",
                    "radius",
                    "effect",
                    "region",
                    "region-bind",
                    "color-bind",
                ]),
                "image" => attrs.extend([
                    "image",
                    "image-bind",
                    "fit",
                    "effect",
                    "region",
                    "region-bind",
                    "color-bind",
                ]),
                "nine-patch" => attrs.extend(["image", "density", "color-bind"]),
                "label" => attrs.extend(["text", "bind", "font-size", "color-bind"]),
                "button" => attrs.extend([
                    "route",
                    "sound",
                    "text",
                    "bind",
                    "font-size",
                    "event",
                    "image",
                    "hover-image",
                    "disabled-image",
                    "region",
                    "region-bind",
                    "radius",
                    "color-bind",
                ]),
                "slider" => attrs.extend(["value", "min", "max", "event"]),
                "toggle" => attrs.extend(["text", "value", "event", "font-size", "color-bind"]),
                "repeat" => attrs.extend(["source", "columns", "item-width", "item-height", "gap"]),
                "color-picker" => attrs.extend(["value", "event", "alpha"]),
                _ => {
                    return Err(error(
                        self.file,
                        xml,
                        crate::i18n::format("host-interface-0008", &[("arg0", tag.to_string())]),
                    ));
                }
            }
            if matches!(tag, "label" | "button" | "toggle") {
                attrs.extend(["text-key", "text-key-bind"]);
            }
            check(
                self.file,
                xml,
                tag,
                &attrs,
                if container {
                    &[
                        "scene",
                        "panel",
                        "image",
                        "nine-patch",
                        "label",
                        "button",
                        "slider",
                        "toggle",
                        "repeat",
                        "color-picker",
                    ]
                } else if matches!(tag, "label" | "button" | "toggle") {
                    &["text-arg"]
                } else {
                    &[]
                },
                false,
            )?;
            let id = required(self.file, xml, "id")?.to_owned();
            if reserved_target(&id) {
                return Err(error(
                    self.file,
                    xml,
                    crate::i18n::format("host-interface-0076", &[("arg0", id.to_string())]),
                ));
            }
            if !self.ids.insert(id.clone()) {
                return Err(error(
                    self.file,
                    xml,
                    crate::i18n::format("host-interface-0009", &[("arg0", id.to_string())]),
                ));
            }
            let rect = [
                number(self.file, xml, "x", Some(0.0))?,
                number(self.file, xml, "y", Some(0.0))?,
                number(self.file, xml, "width", None)?,
                number(self.file, xml, "height", None)?,
            ];
            if rect[2] <= 0.0 || rect[3] <= 0.0 {
                return Err(error(
                    self.file,
                    xml,
                    crate::i18n::text("host-interface-0032"),
                ));
            }
            let mut child_source = source;
            let kind = match tag {
                "scene" => Kind::Scene,
                "panel" => Kind::Panel,
                "image" => Kind::Image,
                "nine-patch" => Kind::NinePatch,
                "label" => Kind::Label,
                "button" => Kind::Button,
                "toggle" => Kind::Toggle,
                "color-picker" => {
                    self.pickers += 1;
                    if self.pickers > 8 {
                        return Err(error(
                            self.file,
                            xml,
                            crate::i18n::text("host-interface-0033"),
                        ));
                    }
                    if source.is_some() {
                        return Err(error(
                            self.file,
                            xml,
                            crate::i18n::text("host-interface-0034"),
                        ));
                    }
                    if xml.attribute("color").is_some() {
                        return Err(error(
                            self.file,
                            xml,
                            crate::i18n::text("host-interface-0035"),
                        ));
                    }
                    let alpha = boolean(self.file, xml, "alpha", true)?;
                    let width = rect[2];
                    let height = rect[3];
                    // The colour disc is min(width, height - reserved row) and needs 128;
                    // the same definition is used by PickerGeometry when drawing.
                    let reserved = if alpha { 26.0 } else { 0.0 };
                    if width < 128.0 || height - reserved < 128.0 {
                        return Err(error(
                            self.file,
                            xml,
                            crate::i18n::text("host-interface-0036"),
                        ));
                    }
                    Kind::ColorPicker { alpha }
                }
                "slider" => {
                    let min = number(self.file, xml, "min", None)?;
                    let max = number(self.file, xml, "max", None)?;
                    if min >= max {
                        return Err(error(
                            self.file,
                            xml,
                            crate::i18n::text("host-interface-0037"),
                        ));
                    }
                    Kind::Slider { min, max }
                }
                "repeat" => {
                    if source.is_some() {
                        return Err(error(
                            self.file,
                            xml,
                            crate::i18n::text("host-interface-0038"),
                        ));
                    }
                    let list = required(self.file, xml, "source")?;
                    if ![
                        "choices",
                        "slots",
                        "autosaves",
                        "history",
                        "gallery",
                        "variables",
                        "textLocales",
                        "voiceLocales",
                    ]
                    .contains(&list)
                    {
                        return Err(error(
                            self.file,
                            xml,
                            crate::i18n::text("host-interface-0039"),
                        ));
                    }
                    child_source = Some(list);
                    let columns = number(self.file, xml, "columns", Some(1.0))?;
                    let cell = [
                        number(self.file, xml, "item-width", None)?,
                        number(self.file, xml, "item-height", None)?,
                    ];
                    let gap = number(self.file, xml, "gap", Some(8.0))?;
                    if !(1.0..=8.0).contains(&columns)
                        || columns.fract() != 0.0
                        || cell.iter().any(|size| *size <= 0.0)
                        || gap < 0.0
                    {
                        return Err(error(
                            self.file,
                            xml,
                            crate::i18n::text("host-interface-0040"),
                        ));
                    }
                    Kind::Repeat {
                        source: list.into(),
                        columns: columns as usize,
                        cell,
                        gap,
                    }
                }
                _ => unreachable!(),
            };
            let font = number(self.file, xml, "font-size", Some(20.0))?;
            let radius = number(self.file, xml, "radius", Some(0.0))?;
            if !(8.0..=96.0).contains(&font) || !(0.0..=128.0).contains(&radius) {
                return Err(error(
                    self.file,
                    xml,
                    crate::i18n::text("host-interface-0041"),
                ));
            }
            let fit = xml.attribute("fit").unwrap_or("stretch").to_owned();
            if !["stretch", "contain", "cover"].contains(&fit.as_str()) {
                return Err(error(
                    self.file,
                    xml,
                    crate::i18n::text("host-interface-0042"),
                ));
            }
            let image_id = xml.attribute("image").map(str::to_owned);
            let image_bind = xml.attribute("image-bind").map(str::to_owned);
            if image_id.is_some() && image_bind.is_some() {
                return Err(error(
                    self.file,
                    xml,
                    crate::i18n::text("host-interface-0043"),
                ));
            }
            let hover_image = xml.attribute("hover-image").map(str::to_owned);
            let disabled_image = xml.attribute("disabled-image").map(str::to_owned);
            for id in [&image_id, &hover_image, &disabled_image]
                .into_iter()
                .flatten()
            {
                if tag == "nine-patch" {
                    if !self
                        .story
                        .assets
                        .get(id)
                        .is_some_and(|a| a.kind == AssetKind::NinePatch)
                    {
                        return Err(error(
                            self.file,
                            xml,
                            crate::i18n::text("host-interface-0044"),
                        ));
                    }
                } else {
                    image(self.file, xml, id, self.story)?;
                }
            }
            if matches!(tag, "image" | "nine-patch") && image_id.is_none() && image_bind.is_none() {
                return Err(error(
                    self.file,
                    xml,
                    crate::i18n::text("host-interface-0045"),
                ));
            }
            let region = xml
                .attribute("region")
                .map(crate::image_layout::Region::parse)
                .transpose()
                .map_err(|e| error(self.file, xml, e))?;
            let region_bind = xml.attribute("region-bind").map(str::to_owned);
            if region.is_some() && region_bind.is_some() {
                return Err(error(
                    self.file,
                    xml,
                    crate::i18n::text("host-interface-0046"),
                ));
            }
            if let Some(path) = &region_bind
                && (!path.starts_with("ui.")
                    || binding(path, self.vars, source) != Some(ValueKind::Text))
            {
                return Err(error(
                    self.file,
                    xml,
                    crate::i18n::text("host-interface-0047"),
                ));
            }
            if (region.is_some() || region_bind.is_some())
                && (image_id.is_none() && image_bind.is_none()
                    || image_bind.as_deref() == Some("item.image"))
            {
                return Err(error(
                    self.file,
                    xml,
                    crate::i18n::text("host-interface-0048"),
                ));
            }
            let color_bind = xml.attribute("color-bind").map(str::to_owned);
            if color_bind.is_some() && xml.attribute("color").is_some() {
                return Err(error(
                    self.file,
                    xml,
                    crate::i18n::text("host-interface-0049"),
                ));
            }
            if let Some(path) = &color_bind {
                let valid = if let Some(id) = path.strip_prefix("ui.") {
                    matches!(
                        self.vars.get(id).map(|decl| &decl.initial),
                        Some(VariableValue::String(_))
                    )
                } else if let Some(id) = path.strip_prefix("colors.") {
                    self.palette.contains_key(id)
                } else {
                    false
                };
                if !valid {
                    return Err(error(
                        self.file,
                        xml,
                        crate::i18n::text("host-interface-0050"),
                    ));
                }
            }
            let density = number(self.file, xml, "density", Some(1.0))?;
            if !(0.125..=8.0).contains(&density) {
                return Err(error(
                    self.file,
                    xml,
                    crate::i18n::text("host-interface-0051"),
                ));
            }
            let text = xml.attribute("text").unwrap_or("").to_owned();
            let bind = xml
                .attribute("bind")
                .or_else(|| xml.attribute("value"))
                .map(str::to_owned);
            if xml.attribute("bind").is_some() && xml.attribute("text").is_some() {
                return Err(error(
                    self.file,
                    xml,
                    crate::i18n::text("host-interface-0052"),
                ));
            }
            let visible = xml.attribute("visible").unwrap_or("true").to_owned();
            let enabled = xml.attribute("enabled").unwrap_or("true").to_owned();
            for path in [&visible, &enabled] {
                if binding(path, self.vars, source) != Some(ValueKind::Bool) {
                    return Err(error(
                        self.file,
                        xml,
                        crate::i18n::format("host-interface-0010", &[("arg0", path.to_string())]),
                    ));
                }
            }
            if let Some(path) = &bind {
                let actual = binding(path, self.vars, source);
                let valid = match kind {
                    Kind::Slider { .. } => actual == Some(ValueKind::Number),
                    Kind::Toggle => actual == Some(ValueKind::Bool),
                    Kind::ColorPicker { .. } => {
                        actual == Some(ValueKind::Text) && path.starts_with("ui.")
                    }
                    _ => actual.is_some(),
                };
                if !valid {
                    return Err(error(
                        self.file,
                        xml,
                        crate::i18n::format("host-interface-0011", &[("arg0", path.to_string())]),
                    ));
                }
            }
            if matches!(kind, Kind::Slider { .. } | Kind::Toggle) && bind.is_none() {
                return Err(error(
                    self.file,
                    xml,
                    crate::i18n::text("host-interface-0053"),
                ));
            }
            if let Kind::ColorPicker { .. } = kind {
                let path = bind.as_deref().ok_or_else(|| {
                    error(self.file, xml, crate::i18n::text("host-interface-0054"))
                })?;
                let name = path.strip_prefix("ui.").unwrap_or_default();
                let initial = match self.vars.get(name).map(|decl| &decl.initial) {
                    Some(VariableValue::String(value)) => value,
                    _ => {
                        return Err(error(
                            self.file,
                            xml,
                            crate::i18n::text("host-interface-0055"),
                        ));
                    }
                };
                crate::color::Rgba::parse(initial).map_err(|reason| {
                    error(
                        self.file,
                        xml,
                        crate::i18n::format("host-interface-0012", &[("arg0", reason.to_string())]),
                    )
                })?;
            }
            if let Some(path) = &image_bind
                && (binding(path, self.vars, source) != Some(ValueKind::Text)
                    || !(path.starts_with("ui.")
                        || (path == "item.image"
                            && matches!(source, Some("gallery" | "slots" | "autosaves")))))
            {
                return Err(error(
                    self.file,
                    xml,
                    crate::i18n::text("host-interface-0056"),
                ));
            }
            let sound = xml.attribute("sound").map(str::to_owned);
            let event = xml.attribute("event").map(str::to_owned);
            let route = xml
                .attribute("route")
                .map(|value| {
                    if event.is_some() {
                        return Err(error(
                            self.file,
                            xml,
                            crate::i18n::text("host-interface-0057"),
                        ));
                    }
                    match value {
                        "close" => Ok(UiRoute::Close),
                        _ => value
                            .strip_prefix("open:")
                            .filter(|id| !id.is_empty() && !id.chars().any(char::is_whitespace))
                            .map(|id| UiRoute::Open(id.into()))
                            .ok_or_else(|| {
                                error(self.file, xml, crate::i18n::text("host-interface-0058"))
                            }),
                    }
                })
                .transpose()?;
            if let Some(event) = &event
                && reserved_event(event)
            {
                return Err(error(
                    self.file,
                    xml,
                    crate::i18n::format("host-interface-0077", &[("arg0", event.to_string())]),
                ));
            }
            if matches!(kind, Kind::Button | Kind::Slider { .. } | Kind::Toggle)
                && event.is_none()
                && route.is_none()
                && sound.is_none()
            {
                return Err(error(
                    self.file,
                    xml,
                    crate::i18n::text("host-interface-0059"),
                ));
            }
            if matches!(kind, Kind::ColorPicker { .. }) && event.is_none() {
                return Err(error(
                    self.file,
                    xml,
                    crate::i18n::text("host-interface-0060"),
                ));
            }
            let children = if container {
                self.nodes(xml, depth + 1, child_source)?
            } else {
                Vec::new()
            };
            let default = if matches!(kind, Kind::Button) && image_id.is_none() {
                [43, 64, 90, 255]
            } else {
                [255, 255, 255, 255]
            };
            let localized = crate::localization::binding(self.file, xml, "text", self.catalog)?;
            nodes.push(Node {
                localized,
                sound,
                route,
                location: {
                    let p = xml.document().text_pos_at(xml.range().start);
                    format!("{}:{}:{}", self.file, p.row, p.col)
                },
                region,
                region_bind,
                color_bind,
                density,
                effect: crate::loader::effect_xml::reference(
                    self.file,
                    xml,
                    "effect",
                    &self.story.effects,
                )?,
                id,
                kind,
                rect,
                color: color(self.file, xml, default)?,
                font,
                radius,
                text,
                bind,
                image: image_id,
                image_bind,
                hover_image,
                disabled_image,
                fit,
                visible,
                enabled,
                event,
                children,
            });
        }
        Ok(nodes)
    }
}

fn ui_node<'a>(screens: &'a BTreeMap<String, Screen>, id: &str) -> Option<&'a Node> {
    fn find<'a>(nodes: &'a [Node], id: &str) -> Option<&'a Node> {
        for node in nodes {
            if node.id == id {
                return Some(node);
            }
            if let Some(found) = find(&node.children, id) {
                return Some(found);
            }
        }
        None
    }
    screens.values().find_map(|screen| find(&screen.nodes, id))
}
fn ui_target_location(screens: &BTreeMap<String, Screen>, id: &str) -> Option<String> {
    screens
        .get(id)
        .map(|screen| screen.location.clone())
        .or_else(|| ui_node(screens, id).map(|node| node.location.clone()))
}
pub fn parse(
    text: &str,
    file: &str,
    story: &Story,
    registrations: &[Registration],
) -> Result<Definition> {
    parse_localized(
        text,
        file,
        story,
        registrations,
        std::sync::Arc::new(crate::localization::Catalog::default()),
    )
}
pub fn parse_localized(
    text: &str,
    file: &str,
    story: &Story,
    registrations: &[Registration],
    mut catalog: crate::localization::SharedCatalog,
) -> Result<Definition> {
    ensure!(
        text.len() <= 1024 * 1024,
        "{}",
        crate::i18n::format("host-interface-0014", &[("arg0", file.to_string())])
    );
    let doc = Document::parse(text).with_context(|| {
        crate::i18n::format("host-interface-0015", &[("arg0", file.to_string())])
    })?;
    let root = doc.root_element();
    check(
        file,
        root,
        "ui",
        &[
            "schema",
            "width",
            "height",
            "initial",
            "title",
            "title-bind",
            "title-key",
            "title-key-bind",
        ],
        &["variables", "gallery", "palette", "screen", "ui-media"],
        false,
    )?;
    if required(file, root, "schema")? != "3" {
        return Err(error(file, root, crate::i18n::text("host-interface-0061")));
    }
    let size = [
        number(file, root, "width", Some(1280.0))?,
        number(file, root, "height", Some(720.0))?,
    ];
    if size[0] < 640.0 || size[1] < 360.0 || (size[0] * 9.0 - size[1] * 16.0).abs() > 0.1 {
        return Err(error(file, root, crate::i18n::text("host-interface-0062")));
    }
    let variables = crate::loader::variables::variables_in(root, file)?;
    let mut palette = BTreeMap::new();
    if elements(root).any(|node| node.has_tag_name("palette")) {
        let group = single(file, root, "palette")?;
        check(file, group, "palette", &[], &["color"], false)?;
        for item in elements(group) {
            check(file, item, "color", &["id", "initial"], &[], false)?;
            let id = required(file, item, "id")?.to_owned();
            let mut chars = id.chars();
            let valid = id.len() <= 64
                && chars.next().is_some_and(|ch| ch.is_ascii_lowercase())
                && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_');
            if !valid {
                return Err(error(file, item, crate::i18n::text("host-interface-0063")));
            }
            let initial_text = required(file, item, "initial")?;
            let initial = crate::color::Rgba::parse(initial_text).map_err(|reason| {
                error(
                    file,
                    item,
                    crate::i18n::format("host-interface-0016", &[("arg0", reason.to_string())]),
                )
            })?;
            let position = doc.text_pos_at(item.range().start);
            if palette
                .insert(
                    id.clone(),
                    PaletteSlot {
                        initial,
                        location: format!("{}:{}:{}", file, position.row, position.col),
                    },
                )
                .is_some()
            {
                return Err(error(
                    file,
                    item,
                    crate::i18n::format("host-interface-0017", &[("arg0", id.to_string())]),
                ));
            }
        }
        if palette.len() > palette::MAX_SLOTS {
            return Err(error(
                file,
                group,
                crate::i18n::format(
                    "host-interface-0018",
                    &[("arg0", format!("{}", palette::MAX_SLOTS))],
                ),
            ));
        }
    }
    if root.has_attribute("title") && root.has_attribute("title-bind") {
        return Err(error(file, root, crate::i18n::text("host-interface-0064")));
    }
    let (value, bound) = if let Some(path) = root.attribute("title-bind") {
        let name = path
            .strip_prefix("ui.")
            .ok_or_else(|| error(file, root, crate::i18n::text("host-interface-0065")))?;
        (name.to_owned(), true)
    } else {
        (
            root.attribute("title").unwrap_or("Kakera AVG").to_owned(),
            false,
        )
    };
    let position = doc.text_pos_at(root.range().start);
    let window_title = WindowTitle {
        value,
        bound,
        location: format!("{file}:{}:{}", position.row, position.col),
    };
    let localized_title = crate::localization::binding(file, root, "title", &catalog)?;
    let mut gallery = BTreeMap::new();
    if elements(root).any(|n| n.has_tag_name("gallery")) {
        let group = single(file, root, "gallery")?;
        check(file, group, "gallery", &[], &["cg"], false)?;
        for cg in elements(group) {
            check(
                file,
                cg,
                "cg",
                &["id", "image", "title", "title-key", "unlocked"],
                &[],
                false,
            )?;
            let id = required(file, cg, "id")?.to_owned();
            let img = required(file, cg, "image")?;
            image(file, cg, img, story)?;
            if story.assets[img].kind == AssetKind::Cg && id != img {
                return Err(error(file, cg, crate::i18n::text("host-interface-0066")));
            }
            let title = if let Some(key) = cg.attribute("title-key") {
                ensure!(
                    cg.attribute("title").is_none(),
                    "{}",
                    crate::i18n::text("host-i18n-detail-0065")
                );
                let title = catalog
                    .static_text(key)
                    .map_err(|cause| error(file, cg, cause))?;
                std::sync::Arc::make_mut(&mut catalog)
                    .targets
                    .insert(crate::localization::Target::Gallery(id.clone()), key.into());
                title
            } else {
                required(file, cg, "title")?.into()
            };
            if gallery
                .insert(
                    id.clone(),
                    Cg {
                        image: img.into(),
                        title,
                        unlocked: boolean(file, cg, "unlocked", false)?,
                    },
                )
                .is_some()
            {
                return Err(error(
                    file,
                    cg,
                    crate::i18n::format("host-interface-0019", &[("arg0", id.to_string())]),
                ));
            }
        }
        if gallery.len() > 256 {
            return Err(error(file, group, crate::i18n::text("host-interface-0067")));
        }
    }
    let mut parser = Parser {
        catalog: &catalog,
        file,
        story,
        vars: &variables,
        palette: &palette,
        ids: BTreeSet::new(),
        count: 0,
        pickers: 0,
    };
    for (id, asset) in &story.assets {
        if asset.kind == AssetKind::Cg {
            if let Some(cg) = gallery.get(id) {
                if cg.image != *id {
                    return Err(error(file, root, crate::i18n::text("host-interface-0068")));
                }
            } else {
                gallery.insert(
                    id.clone(),
                    Cg {
                        image: id.clone(),
                        title: id.clone(),
                        unlocked: false,
                    },
                );
            }
        }
    }
    if gallery.len() > 256 {
        return Err(error(file, root, crate::i18n::text("host-interface-0069")));
    }
    let mut screens = BTreeMap::new();
    for screen in elements(root).filter(|node| node.has_tag_name("screen")) {
        check(
            file,
            screen,
            "screen",
            &[
                "id",
                "modal",
                "pause",
                "background-click",
                "background-right-click",
                "sequence",
            ],
            &[
                "scene",
                "panel",
                "image",
                "nine-patch",
                "label",
                "button",
                "slider",
                "toggle",
                "repeat",
                "color-picker",
            ],
            false,
        )?;
        let id = required(file, screen, "id")?.to_owned();
        if reserved_target(&id) {
            return Err(error(
                file,
                screen,
                crate::i18n::format("host-interface-0076", &[("arg0", id.to_string())]),
            ));
        }
        let event = |name| -> Result<Option<String>> {
            screen
                .attribute(name)
                .map(|value| {
                    if reserved_event(value) {
                        return Err(error(
                            file,
                            screen,
                            crate::i18n::format(
                                "host-interface-0077",
                                &[("arg0", value.to_string())],
                            ),
                        ));
                    }
                    Ok(value.to_owned())
                })
                .transpose()
        };
        let position = doc.text_pos_at(screen.range().start);
        let value = Screen {
            location: format!("{file}:{}:{}", position.row, position.col),
            sequence: screen.attribute("sequence").map(str::to_owned),
            modal: boolean(file, screen, "modal", true)?,
            pause: boolean(file, screen, "pause", true)?,
            background_click: event("background-click")?,
            background_right_click: event("background-right-click")?,
            nodes: parser.nodes(screen, 1, None)?,
        };
        if screens.insert(id.clone(), value).is_some() {
            return Err(error(
                file,
                screen,
                crate::i18n::format("host-interface-0021", &[("arg0", id.to_string())]),
            ));
        }
    }
    let initial = required(file, root, "initial")?.to_owned();
    fn validate_routes(
        file: &str,
        parent: XmlNode<'_, '_>,
        screens: &BTreeMap<String, Screen>,
    ) -> Result<()> {
        for node in elements(parent) {
            if let Some(value) = node.attribute("route")
                && let Some(target) = value.strip_prefix("open:")
                && !screens.contains_key(target)
            {
                return Err(error(
                    file,
                    node,
                    crate::i18n::format("host-interface-0022", &[("arg0", target.to_string())]),
                ));
            }
            validate_routes(file, node, screens)?;
        }
        Ok(())
    }
    validate_routes(file, root, &screens)?;
    if screens.is_empty() || screens.len() > 64 {
        return Err(error(file, root, crate::i18n::text("host-interface-0070")));
    }
    if !screens.get(&initial).is_some_and(|s| !s.modal) {
        return Err(error(file, root, crate::i18n::text("host-interface-0071")));
    }
    // `ui(id)` has one namespace: a screen and a control may never share an ID.
    for (id, screen) in &screens {
        if let Some(node) = ui_node(&screens, id) {
            return Err(anyhow::anyhow!(
                "{}",
                crate::i18n::format(
                    "host-interface-0085",
                    &[
                        ("arg0", id.clone()),
                        ("arg1", screen.location.clone()),
                        ("arg2", node.location.clone())
                    ]
                )
            ));
        }
    }
    let images = story
        .assets
        .iter()
        .filter(|(_, asset)| matches!(asset.kind, AssetKind::Image | AssetKind::Cg))
        .map(|(id, _)| id.clone())
        .collect();
    let media = presentation::Catalog::parse(root, file, story, &screens)?;
    fn validate_media(
        file: &str,
        node: XmlNode<'_, '_>,
        media: &presentation::Catalog,
        story: &Story,
    ) -> Result<()> {
        if let Some(id) = node.attribute("sound")
            && !media.sound_exists(id)
        {
            return Err(error(file, node, crate::i18n::text("host-interface-0072")));
        }
        if node.has_tag_name("screen")
            && let Some(id) = node.attribute("sequence")
            && !media.sequences.contains_key(id)
        {
            return Err(error(file, node, crate::i18n::text("host-interface-0073")));
        }
        for name in ["visible", "enabled", "value", "bind"] {
            if let Some(path) = node.attribute(name)
                && let Some(asset) = path
                    .strip_prefix("audio.excluded.")
                    .or_else(|| path.strip_prefix("not:audio.excluded."))
                && !media.audio.contains(asset)
            {
                return Err(error(file, node, crate::i18n::text("host-interface-0074")));
            }
            if let Some(path) = node.attribute(name)
                && let Some(actor) = path
                    .strip_prefix("audio.actor.")
                    .or_else(|| path.strip_prefix("not:audio.actor."))
                && !story.characters.contains_key(actor)
            {
                return Err(error(file, node, crate::i18n::text("host-interface-0075")));
            }
        }
        for child in elements(node) {
            validate_media(file, child, media, story)?;
        }
        Ok(())
    }
    validate_media(file, root, &media, story)?;
    let mut lifecycle = BTreeMap::new();
    let mut host_handlers = BTreeSet::new();
    let mut enter = false;
    let mut seen: BTreeMap<UiEventKey, &Registration> = BTreeMap::new();
    for registration in registrations {
        let key = &registration.key;
        if let Some(first) = seen.insert(key.clone(), registration) {
            return Err(anyhow::anyhow!(
                "{}",
                crate::i18n::format(
                    "host-interface-0078",
                    &[
                        ("arg0", key.describe()),
                        ("arg1", registration.location.to_string()),
                        ("arg2", first.location.to_string())
                    ]
                )
            ));
        }
        match key.category {
            EventCategory::Game => {
                ensure!(
                    key.target == "$game" && key.event == "game.enter",
                    "{}",
                    crate::i18n::format("host-interface-0079", &[("arg0", key.describe())])
                );
                enter = true;
            }
            EventCategory::Host => {
                ensure!(
                    key.target == "$host" && !reserved_event(&key.event),
                    "{}",
                    crate::i18n::format("host-interface-0079", &[("arg0", key.describe())])
                );
                host_handlers.insert(key.event.clone());
            }
            EventCategory::FrameEnter | EventCategory::FrameExit => {
                let actual = story.resolve_frame(&key.target).ok_or_else(|| {
                    anyhow::anyhow!(
                        "{}",
                        crate::i18n::format(
                            "host-interface-0080",
                            &[
                                ("arg0", key.describe()),
                                ("arg1", registration.location.to_string())
                            ]
                        )
                    )
                })?;
                let slot = format!("{}:{}", key.category.id(), actual);
                ensure!(
                    lifecycle.insert(slot.clone(), key.clone()).is_none(),
                    "{}",
                    crate::i18n::format("host-interface-0081", &[("arg0", slot.to_string())])
                );
            }
            EventCategory::Ui => {
                ensure!(
                    !key.event.is_empty(),
                    "{}",
                    crate::i18n::format("host-interface-0079", &[("arg0", key.describe())])
                );
                let screen = screens.get(&key.target);
                match key.event.as_str() {
                    "screen.open" | "screen.close" => {
                        ensure!(
                            screen.is_some(),
                            "{}",
                            crate::i18n::format(
                                "host-interface-0082",
                                &[
                                    ("arg0", key.describe()),
                                    ("arg1", registration.location.to_string())
                                ]
                            )
                        );
                        let slot = format!("{}:{}", key.event, key.target);
                        ensure!(
                            lifecycle.insert(slot.clone(), key.clone()).is_none(),
                            "{}",
                            crate::i18n::format(
                                "host-interface-0081",
                                &[("arg0", slot.to_string())]
                            )
                        );
                    }
                    event if reserved_event(event) => {
                        return Err(anyhow::anyhow!(
                            "{}",
                            crate::i18n::format("host-interface-0079", &[("arg0", key.describe())])
                        ));
                    }
                    event => {
                        let node = ui_node(&screens, &key.target);
                        ensure!(
                            screen.is_some() || node.is_some(),
                            "{}",
                            crate::i18n::format(
                                "host-interface-0086",
                                &[
                                    ("arg0", key.describe()),
                                    ("arg1", registration.location.to_string())
                                ]
                            )
                        );
                        let matched = match screen {
                            Some(screen) => {
                                screen.background_click.as_deref() == Some(event)
                                    || screen.background_right_click.as_deref() == Some(event)
                            }
                            None => node.is_some_and(|node| node.event.as_deref() == Some(event)),
                        };
                        ensure!(
                            matched,
                            "{}",
                            crate::i18n::format(
                                "host-interface-0083",
                                &[
                                    ("arg0", key.describe()),
                                    ("arg1", registration.location.to_string()),
                                    (
                                        "arg2",
                                        ui_target_location(&screens, &key.target)
                                            .unwrap_or_default()
                                    )
                                ]
                            )
                        );
                    }
                }
            }
        }
    }
    // Every XML event reference needs exactly one registered callback, and every
    // registration of a real target points back at a real XML event.
    let mut xml_keys: BTreeMap<UiEventKey, String> = BTreeMap::new();
    for (id, screen) in &screens {
        for name in [&screen.background_click, &screen.background_right_click]
            .into_iter()
            .flatten()
        {
            xml_keys.insert(
                UiEventKey::ui(id.clone(), name.clone()),
                screen.location.clone(),
            );
        }
    }
    fn collect_events(nodes: &[Node], keys: &mut BTreeMap<UiEventKey, String>) {
        for node in nodes {
            if let Some(event) = &node.event {
                keys.insert(
                    UiEventKey::ui(node.id.clone(), event.clone()),
                    node.location.clone(),
                );
            }
            collect_events(&node.children, keys);
        }
    }
    for (id, screen) in &screens {
        collect_events(&screen.nodes, &mut xml_keys);
        let _ = id;
    }
    for (key, location) in &xml_keys {
        ensure!(
            seen.contains_key(key),
            "{}",
            crate::i18n::format(
                "host-interface-0084",
                &[("arg0", key.describe()), ("arg1", location.to_string())]
            )
        );
    }
    let definition = Definition {
        localization: catalog,
        localized_title,
        lifecycle,
        media,
        window_title,
        size,
        initial,
        variables,
        gallery,
        palette,
        screens,
        images,
        nine_patches: story
            .assets
            .iter()
            .filter(|(_, a)| a.kind == AssetKind::NinePatch)
            .map(|(id, _)| id.clone())
            .collect(),
        image_sizes: BTreeMap::new(),
        host_handlers,
        enter,
    };
    definition
        .window_title
        .resolve(&definition.initial_variables())?;
    host::State::try_new(&definition)?;
    Ok(definition)
}
