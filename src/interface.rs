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
pub mod render;

#[derive(Clone, Debug)]
pub struct Definition {
    pub size: [f32; 2],
    pub initial: String,
    pub variables: BTreeMap<String, VariableDecl>,
    pub gallery: BTreeMap<String, Cg>,
    pub screens: BTreeMap<String, Screen>,
    pub images: BTreeSet<String>,
    pub nine_patches: BTreeSet<String>,
    pub image_sizes: BTreeMap<String, [u32; 2]>,
    pub enter: bool,
}
#[derive(Clone, Debug)]
pub struct Cg {
    pub image: String,
    pub title: String,
    pub unlocked: bool,
}
#[derive(Clone, Debug)]
pub struct Screen {
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
}
#[derive(Clone, Debug)]
pub struct Node {
    pub location: String,
    pub region: Option<crate::image_layout::Region>,
    pub region_bind: Option<String>,
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
#[derive(Clone, Debug)]
pub struct Event {
    pub name: String,
    pub item: Value,
    pub value: Value,
}
#[derive(Clone, Debug, Default)]
pub struct Data {
    pub values: Value,
    pub lists: BTreeMap<String, Vec<Value>>,
}
impl Data {
    pub fn get<'a>(&'a self, path: &str, item: &'a Value) -> Option<&'a Value> {
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
    pub fn events(&self) -> BTreeSet<String> {
        fn collect(nodes: &[Node], out: &mut BTreeSet<String>) {
            for node in nodes {
                if let Some(event) = &node.event {
                    out.insert(event.clone());
                }
                collect(&node.children, out);
            }
        }
        let mut out = BTreeSet::new();
        for screen in self.screens.values() {
            out.extend(screen.background_click.iter().cloned());
            out.extend(screen.background_right_click.iter().cloned());
            collect(&screen.nodes, &mut out);
        }
        out
    }
}

fn number(file: &str, node: XmlNode<'_, '_>, key: &str, default: Option<f32>) -> Result<f32> {
    let value = match node.attribute(key) {
        Some(text) => text
            .parse()
            .map_err(|_| error(file, node, format!("{key} 必须为数值")))?,
        None => default.ok_or_else(|| error(file, node, format!("缺少 {key}")))?,
    };
    if !f32::is_finite(value) || value.abs() > 8192.0 {
        return Err(error(file, node, format!("{key} 超出有限范围")));
    }
    Ok(value)
}
fn boolean(file: &str, node: XmlNode<'_, '_>, key: &str, default: bool) -> Result<bool> {
    match node.attribute(key) {
        None => Ok(default),
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        _ => Err(error(file, node, format!("{key} 必须为 true/false"))),
    }
}
fn image(file: &str, node: XmlNode<'_, '_>, id: &str, story: &Story) -> Result<()> {
    if !story
        .assets
        .get(id)
        .is_some_and(|asset| asset.kind == AssetKind::Image)
    {
        return Err(error(
            file,
            node,
            format!("UI 图片引用不存在或类型错误：{id}"),
        ));
    }
    Ok(())
}
fn color(file: &str, node: XmlNode<'_, '_>, default: [u8; 4]) -> Result<[u8; 4]> {
    let Some(text) = node.attribute("color") else {
        return Ok(default);
    };
    let Some(hex) = text.strip_prefix('#') else {
        return Err(error(file, node, "颜色需要 #RRGGBB 或 #RRGGBBAA"));
    };
    if ![6, 8].contains(&hex.len()) || !hex.is_ascii() {
        return Err(error(file, node, "颜色格式错误"));
    }
    let mut color = [255; 4];
    for (index, value) in color.iter_mut().enumerate().take(hex.len() / 2) {
        *value = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| error(file, node, "颜色格式错误"))?;
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
    if let Some(id) = path.strip_prefix("game.") {
        return match id {
            "frameId" | "speaker" | "text" | "continueText" | "autoText" => Some(ValueKind::Text),
            "complete" | "canAdvance" | "ended" | "noChoices" | "transitioning" | "autoPlaying"
            | "videoPlaying" | "videoSkippable" | "live2dEnabled" => Some(ValueKind::Bool),
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
    let source = source.map(|s| if s == "autosaves" { "slots" } else { s });
    let field = path.strip_prefix("item.")?;
    match (source?, field) {
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
    file: &'a str,
    story: &'a Story,
    vars: &'a BTreeMap<String, VariableDecl>,
    ids: BTreeSet<String>,
    count: usize,
    handlers: &'a BTreeSet<String>,
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
                return Err(error(self.file, xml, "UI 超过深度 8 或 1024 节点"));
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
                ]),
                "image" => attrs.extend([
                    "image",
                    "image-bind",
                    "fit",
                    "effect",
                    "region",
                    "region-bind",
                ]),
                "nine-patch" => attrs.extend(["image", "density"]),
                "label" => attrs.extend(["text", "bind", "font-size"]),
                "button" => attrs.extend([
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
                ]),
                "slider" => attrs.extend(["value", "min", "max", "event"]),
                "toggle" => attrs.extend(["text", "value", "event", "font-size"]),
                "repeat" => attrs.extend(["source", "columns", "item-width", "item-height", "gap"]),
                _ => return Err(error(self.file, xml, format!("未知 UI 节点：{tag}"))),
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
                    ]
                } else {
                    &[]
                },
                false,
            )?;
            let id = required(self.file, xml, "id")?.to_owned();
            if !self.ids.insert(id.clone()) {
                return Err(error(self.file, xml, format!("重复 UI ID：{id}")));
            }
            let rect = [
                number(self.file, xml, "x", Some(0.0))?,
                number(self.file, xml, "y", Some(0.0))?,
                number(self.file, xml, "width", None)?,
                number(self.file, xml, "height", None)?,
            ];
            if rect[2] <= 0.0 || rect[3] <= 0.0 {
                return Err(error(self.file, xml, "UI 宽高必须为正数"));
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
                "slider" => {
                    let min = number(self.file, xml, "min", None)?;
                    let max = number(self.file, xml, "max", None)?;
                    if min >= max {
                        return Err(error(self.file, xml, "slider min 必须小于 max"));
                    }
                    Kind::Slider { min, max }
                }
                "repeat" => {
                    if source.is_some() {
                        return Err(error(self.file, xml, "不支持嵌套 repeat"));
                    }
                    let list = required(self.file, xml, "source")?;
                    if ![
                        "choices",
                        "slots",
                        "autosaves",
                        "history",
                        "gallery",
                        "variables",
                    ]
                    .contains(&list)
                    {
                        return Err(error(self.file, xml, "未知 repeat 数据集"));
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
                        return Err(error(self.file, xml, "repeat 列数/单元格/间隔非法"));
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
                return Err(error(self.file, xml, "font-size/radius 超出范围"));
            }
            let fit = xml.attribute("fit").unwrap_or("stretch").to_owned();
            if !["stretch", "contain", "cover"].contains(&fit.as_str()) {
                return Err(error(self.file, xml, "未知图片 fit"));
            }
            let image_id = xml.attribute("image").map(str::to_owned);
            let image_bind = xml.attribute("image-bind").map(str::to_owned);
            if image_id.is_some() && image_bind.is_some() {
                return Err(error(self.file, xml, "image 与 image-bind 不能同时存在"));
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
                            "nine-patch 节点需要 nine-patch 类型资源",
                        ));
                    }
                } else {
                    image(self.file, xml, id, self.story)?;
                }
            }
            if matches!(tag, "image" | "nine-patch") && image_id.is_none() && image_bind.is_none() {
                return Err(error(self.file, xml, "image 节点需要图片"));
            }
            let region = xml
                .attribute("region")
                .map(crate::image_layout::Region::parse)
                .transpose()
                .map_err(|e| error(self.file, xml, e))?;
            let region_bind = xml.attribute("region-bind").map(str::to_owned);
            if region.is_some() && region_bind.is_some() {
                return Err(error(self.file, xml, "region 与 region-bind 不能同时设置"));
            }
            if let Some(path) = &region_bind
                && (!path.starts_with("ui.")
                    || binding(path, self.vars, source) != Some(ValueKind::Text))
            {
                return Err(error(self.file, xml, "region-bind 需要 ui 字符串变量"));
            }
            if (region.is_some() || region_bind.is_some())
                && (image_id.is_none() && image_bind.is_none()
                    || image_bind.as_deref() == Some("item.image"))
            {
                return Err(error(
                    self.file,
                    xml,
                    "region 需要静态资源或 UI 图片绑定，不用于 item.image 运行时预览",
                ));
            }
            let density = number(self.file, xml, "density", Some(1.0))?;
            if !(0.125..=8.0).contains(&density) {
                return Err(error(self.file, xml, "density 必须在 0.125–8"));
            }
            let text = xml.attribute("text").unwrap_or("").to_owned();
            let bind = xml
                .attribute("bind")
                .or_else(|| xml.attribute("value"))
                .map(str::to_owned);
            if xml.attribute("bind").is_some() && xml.attribute("text").is_some() {
                return Err(error(self.file, xml, "text 与 bind 不能同时存在"));
            }
            let visible = xml.attribute("visible").unwrap_or("true").to_owned();
            let enabled = xml.attribute("enabled").unwrap_or("true").to_owned();
            for path in [&visible, &enabled] {
                if binding(path, self.vars, source) != Some(ValueKind::Bool) {
                    return Err(error(self.file, xml, format!("未知或非 bool 绑定：{path}")));
                }
            }
            if let Some(path) = &bind {
                let actual = binding(path, self.vars, source);
                let valid = match kind {
                    Kind::Slider { .. } => actual == Some(ValueKind::Number),
                    Kind::Toggle => actual == Some(ValueKind::Bool),
                    _ => actual.is_some(),
                };
                if !valid {
                    return Err(error(self.file, xml, format!("未知或类型错误绑定：{path}")));
                }
            }
            if matches!(kind, Kind::Slider { .. } | Kind::Toggle) && bind.is_none() {
                return Err(error(self.file, xml, "slider/toggle 需要 value 绑定"));
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
                    "image-bind 需要 ui 字符串，或 gallery/slots/autosaves 模板的 item.image",
                ));
            }
            let event = xml.attribute("event").map(str::to_owned);
            if let Some(event) = &event
                && (!self.handlers.contains(event) || event == "game.enter")
            {
                return Err(error(
                    self.file,
                    xml,
                    format!("UI 事件未注册或为保留事件：{event}"),
                ));
            }
            if matches!(kind, Kind::Button | Kind::Slider { .. } | Kind::Toggle) && event.is_none()
            {
                return Err(error(self.file, xml, "交互节点缺少 event"));
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
            nodes.push(Node {
                location: {
                    let p = xml.document().text_pos_at(xml.range().start);
                    format!("{}:{}:{}", self.file, p.row, p.col)
                },
                region,
                region_bind,
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

pub fn parse(
    text: &str,
    file: &str,
    story: &Story,
    handlers: &BTreeSet<String>,
) -> Result<Definition> {
    ensure!(text.len() <= 1024 * 1024, "{file}：UI XML 超过 1MiB");
    let doc = Document::parse(text).with_context(|| format!("{file} 解析失败"))?;
    let root = doc.root_element();
    check(
        file,
        root,
        "ui",
        &["schema", "width", "height", "initial"],
        &["variables", "gallery", "screen"],
        false,
    )?;
    if required(file, root, "schema")? != "3" {
        return Err(error(file, root, "不支持的 UI schema"));
    }
    let size = [
        number(file, root, "width", Some(1280.0))?,
        number(file, root, "height", Some(720.0))?,
    ];
    if size[0] < 640.0 || size[1] < 360.0 || (size[0] * 9.0 - size[1] * 16.0).abs() > 0.1 {
        return Err(error(file, root, "UI 画布需要至少 640×360 的 16:9 尺寸"));
    }
    let variables = crate::loader::variables::variables_in(root, file)?;
    let mut gallery = BTreeMap::new();
    if elements(root).any(|n| n.has_tag_name("gallery")) {
        let group = single(file, root, "gallery")?;
        check(file, group, "gallery", &[], &["cg"], false)?;
        for cg in elements(group) {
            check(
                file,
                cg,
                "cg",
                &["id", "image", "title", "unlocked"],
                &[],
                false,
            )?;
            let id = required(file, cg, "id")?.to_owned();
            let img = required(file, cg, "image")?;
            image(file, cg, img, story)?;
            if gallery
                .insert(
                    id.clone(),
                    Cg {
                        image: img.into(),
                        title: required(file, cg, "title")?.into(),
                        unlocked: boolean(file, cg, "unlocked", false)?,
                    },
                )
                .is_some()
            {
                return Err(error(file, cg, format!("重复 CG：{id}")));
            }
        }
        if gallery.len() > 256 {
            return Err(error(file, group, "CG 超过 256 项"));
        }
    }
    let mut parser = Parser {
        file,
        story,
        vars: &variables,
        ids: BTreeSet::new(),
        count: 0,
        handlers,
    };
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
            ],
            false,
        )?;
        let id = required(file, screen, "id")?.to_owned();
        let event = |name| -> Result<Option<String>> {
            screen
                .attribute(name)
                .map(|value| {
                    if !handlers.contains(value) || value == "game.enter" {
                        return Err(error(
                            file,
                            screen,
                            format!("背景事件未注册或为保留事件：{value}"),
                        ));
                    }
                    Ok(value.to_owned())
                })
                .transpose()
        };
        let value = Screen {
            modal: boolean(file, screen, "modal", true)?,
            pause: boolean(file, screen, "pause", true)?,
            background_click: event("background-click")?,
            background_right_click: event("background-right-click")?,
            nodes: parser.nodes(screen, 1, None)?,
        };
        if screens.insert(id.clone(), value).is_some() {
            return Err(error(file, screen, format!("重复屏幕：{id}")));
        }
    }
    let initial = required(file, root, "initial")?.to_owned();
    ensure!(
        !screens.is_empty() && screens.len() <= 64,
        "{file}：屏幕数量必须为 1–64"
    );
    ensure!(
        screens.get(&initial).is_some_and(|s| !s.modal),
        "{file}：初始屏幕必须存在且非模态"
    );
    let images = story
        .assets
        .iter()
        .filter(|(_, asset)| asset.kind == AssetKind::Image)
        .map(|(id, _)| id.clone())
        .collect();
    let definition = Definition {
        size,
        initial,
        variables,
        gallery,
        screens,
        images,
        nine_patches: story
            .assets
            .iter()
            .filter(|(_, a)| a.kind == AssetKind::NinePatch)
            .map(|(id, _)| id.clone())
            .collect(),
        image_sizes: BTreeMap::new(),
        enter: handlers.contains("game.enter"),
    };
    for event in definition.events() {
        ensure!(
            handlers.contains(&event),
            "{file}：UI 事件未注册 on.ui：{event}"
        );
    }
    host::validate_images(&definition, &host::State::new(&definition))?;
    Ok(definition)
}
