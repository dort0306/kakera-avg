use crate::model::*;
use anyhow::{Context, Result, anyhow, bail};
use roxmltree::{Document, Node};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

pub(crate) mod effect_xml;
mod media_xml;
pub(crate) mod variables;

pub struct LoadedStory {
    pub story: Story,
    pub scripts: Vec<std::path::PathBuf>,
    pub interface: crate::interface::Definition,
}

struct ParsedStory {
    story: Story,
    scripts: Vec<std::path::PathBuf>,
    interface_path: std::path::PathBuf,
}

pub(crate) fn error(
    file: &str,
    node: Node<'_, '_>,
    message: impl std::fmt::Display,
) -> anyhow::Error {
    let pos = node.document().text_pos_at(node.range().start);
    anyhow!("{file}:{}:{}: {message}", pos.row, pos.col)
}

pub(crate) fn check(
    file: &str,
    node: Node<'_, '_>,
    tag: &str,
    attrs: &[&str],
    children: &[&str],
    text: bool,
) -> Result<()> {
    if node.tag_name().name() != tag
        || node.tag_name().namespace().is_some()
        || node.namespaces().next().is_some()
    {
        return Err(error(file, node, format!("预期无命名空间的 <{tag}>")));
    }
    for attr in node.attributes() {
        if attr.namespace().is_some() || !attrs.contains(&attr.name()) {
            return Err(error(file, node, format!("未知属性 {}", attr.name())));
        }
    }
    for child in node.children() {
        if child.is_element() && !children.contains(&child.tag_name().name()) {
            return Err(error(
                file,
                child,
                format!("未知元素 <{}>", child.tag_name().name()),
            ));
        }
        if !text && child.is_text() && !child.text().unwrap_or("").trim().is_empty() {
            return Err(error(file, child, "这里不允许普通文本"));
        }
    }
    Ok(())
}

pub(crate) fn required<'a>(file: &str, node: Node<'a, '_>, name: &str) -> Result<&'a str> {
    node.attribute(name)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| error(file, node, format!("缺少非空属性 {name}")))
}

fn unique<T>(
    file: &str,
    node: Node<'_, '_>,
    map: &mut BTreeMap<String, T>,
    id: &str,
    value: T,
) -> Result<()> {
    if id.is_empty() || id.chars().any(char::is_whitespace) {
        return Err(error(
            file,
            node,
            format!("ID 必须非空且不能含空白：{id:?}"),
        ));
    }
    if map.insert(id.to_owned(), value).is_some() {
        return Err(error(file, node, format!("重复 ID：{id}")));
    }
    Ok(())
}

pub(crate) fn single<'a, 'input>(
    file: &str,
    parent: Node<'a, 'input>,
    name: &str,
) -> Result<Node<'a, 'input>> {
    let mut nodes = parent.children().filter(|n| n.has_tag_name(name));
    let node = nodes
        .next()
        .ok_or_else(|| error(file, parent, format!("缺少 <{name}>")))?;
    if nodes.next().is_some() {
        return Err(error(file, parent, format!("重复 <{name}>")));
    }
    Ok(node)
}

pub(crate) fn elements<'a, 'input>(
    node: Node<'a, 'input>,
) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(Node::is_element)
}

fn plain_text(file: &str, node: Node<'_, '_>, tag: &str, attrs: &[&str]) -> Result<String> {
    check(file, node, tag, attrs, &[], true)?;
    let text: String = node
        .children()
        .filter(Node::is_text)
        .filter_map(|n| n.text())
        .collect();
    if text.trim().is_empty() {
        return Err(error(file, node, "文本不能为空"));
    }
    Ok(text)
}

fn image_ref(
    file: &str,
    node: Node<'_, '_>,
    assets: &BTreeMap<String, Asset>,
    id: &str,
) -> Result<()> {
    if !matches!(
        assets.get(id),
        Some(Asset {
            kind: AssetKind::Image,
            ..
        })
    ) {
        return Err(error(file, node, format!("图片引用不存在或类型错误：{id}")));
    }
    Ok(())
}

/// Parse the assembled current grammar into owned data; resource paths stay logical.
fn parse_story(story_xml: &str) -> Result<ParsedStory> {
    let content = Document::parse(story_xml).context("story.xml 解析失败")?;
    let s = "story.xml";
    let story_node = content.root_element();
    let schema = required(s, story_node, "schema")?;
    check(
        s,
        story_node,
        "story",
        &["schema", "id", "content-version", "entry"],
        &[
            "assets",
            "characters",
            "visuals",
            "frames",
            "variables",
            "scripts",
            "interface",
            "effects",
            "next",
        ],
        false,
    )?;
    if schema != "8" {
        return Err(error(s, story_node, "仅支持 story schema 8"));
    }
    if crate::story_chain::next(s, story_node)?.is_some() {
        return Err(error(
            s,
            story_node,
            "多文件剧本必须通过 ContentSource 先合并",
        ));
    }
    let id = required(s, story_node, "id")?.to_owned();
    if id == "."
        || id == ".."
        || id
            .chars()
            .any(|c| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '.')
    {
        return Err(error(
            s,
            story_node,
            "故事 ID 仅允许 ASCII 字母、数字、点、连字符和下划线",
        ));
    }
    let content_version: u32 = required(s, story_node, "content-version")?
        .parse()
        .map_err(|_| error(s, story_node, "content-version 必须是正整数"))?;
    if content_version == 0 {
        return Err(error(s, story_node, "content-version 必须大于 0"));
    }
    let variables = variables::variables(story_node)?;
    let effects = effect_xml::presets(story_node)?;

    let mut assets = BTreeMap::new();
    let asset_node = single(s, story_node, "assets")?;
    let asset_tags = &["image", "nine-patch", "font", "audio", "video", "live2d"];
    check(s, asset_node, "assets", &[], asset_tags, false)?;
    for node in elements(asset_node) {
        check(s, node, node.tag_name().name(), &["id", "src"], &[], false)?;
        let id = required(s, node, "id")?;
        let relative = Path::new(required(s, node, "src")?);
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            return Err(error(
                s,
                node,
                format!("资源 {id} 必须使用游戏目录内的相对路径"),
            ));
        }
        let kind = match node.tag_name().name() {
            "video" => AssetKind::Video,
            "live2d" => AssetKind::Live2D,
            "image" => AssetKind::Image,
            "nine-patch" => {
                if !relative.to_string_lossy().ends_with(".9.png") {
                    return Err(error(s, node, "nine-patch 资源必须为源 .9.png"));
                }
                AssetKind::NinePatch
            }
            "font" => AssetKind::Font,
            "audio" => {
                let extension = relative
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !["wav", "ogg"].contains(&extension.as_str()) {
                    return Err(error(s, node, "音频仅支持 .wav 或 .ogg"));
                }
                AssetKind::Audio
            }
            _ => unreachable!(),
        };
        unique(
            s,
            node,
            &mut assets,
            id,
            Asset {
                kind,
                path: relative.to_owned(),
            },
        )?;
    }
    if !assets.values().any(|a| a.kind == AssetKind::Font) {
        return Err(error(s, asset_node, "至少需要配置一份中文字体"));
    }
    let mut characters = BTreeMap::new();
    let chars_node = single(s, story_node, "characters")?;
    check(s, chars_node, "characters", &[], &["character"], false)?;
    for node in elements(chars_node) {
        check(s, node, "character", &["id", "name"], &[], false)?;
        unique(
            s,
            node,
            &mut characters,
            required(s, node, "id")?,
            required(s, node, "name")?.to_owned(),
        )?;
    }
    let mut visuals = BTreeMap::new();
    let visuals_node = single(s, story_node, "visuals")?;
    check(s, visuals_node, "visuals", &[], &["visual"], false)?;
    for node in elements(visuals_node) {
        check(
            s,
            node,
            "visual",
            &["id", "background", "background-effect"],
            &["portrait"],
            false,
        )?;
        let background = node.attribute("background").map(str::to_owned);
        if let Some(bg) = &background {
            image_ref(s, node, &assets, bg)?;
        }
        let mut portraits = Vec::new();
        let mut slots = BTreeSet::new();
        for p in elements(node) {
            check(
                s,
                p,
                "portrait",
                &[
                    "character",
                    "image",
                    "slot",
                    "effect",
                    "pose",
                    "enter",
                    "exit",
                    "duration",
                    "strength",
                    "easing",
                ],
                &["live2d"],
                false,
            )?;
            let character = required(s, p, "character")?;
            if !characters.contains_key(character) {
                return Err(error(s, p, format!("未知人物：{character}")));
            }
            let image = required(s, p, "image")?;
            image_ref(s, p, &assets, image)?;
            let slot = match required(s, p, "slot")? {
                "left" => Slot::Left,
                "center" => Slot::Center,
                "right" => Slot::Right,
                value => return Err(error(s, p, format!("无效立绘位置：{value}"))),
            };
            if !slots.insert(slot) {
                return Err(error(s, p, "立绘位置重复"));
            }
            let (pose, enter, exit) = effect_xml::portrait(s, p)?;
            let live2d = media_xml::live2d(p, &assets)?;
            let effect = effect_xml::reference(s, p, "effect", &effects)?;
            if live2d.is_some() && effect.filter != crate::model::effects::Filter::None {
                return Err(error(
                    s,
                    p,
                    "Live2D 不支持静态栅格滤镜；可使用透明度/染色/缩放/旋转",
                ));
            }
            portraits.push(Portrait {
                live2d,
                pose,
                enter,
                exit,
                effect,
                character: character.to_owned(),
                image: image.to_owned(),
                slot,
            });
        }
        unique(
            s,
            node,
            &mut visuals,
            required(s, node, "id")?,
            Visual {
                background_effect: effect_xml::reference(s, node, "background-effect", &effects)?,
                background,
                portraits,
            },
        )?;
    }
    let mut frames = BTreeMap::new();
    let mut frame_names = BTreeMap::new();
    let mut frame_references = BTreeMap::new();
    let frames_node = single(s, story_node, "frames")?;
    check(s, frames_node, "frames", &[], &["frame"], false)?;
    for node in elements(frames_node) {
        check(
            s,
            node,
            "frame",
            &["id", "name", "visual", "speaker"],
            &["text", "options", "audio", "transition", "video"],
            false,
        )?;
        let name = node
            .attribute("name")
            .map(|_| required(s, node, "name"))
            .transpose()?;
        let id = if node.attribute("id").is_some() {
            required(s, node, "id")?
        } else {
            name.ok_or_else(|| error(s, node, "省略 frame.id 时需要稳定 name 供入口和 JS 引用"))?
        };
        for reference in std::iter::once(id).chain(name) {
            if reference.chars().any(char::is_whitespace) {
                return Err(error(s, node, "frame 的 id/name 不能包含空白"));
            }
            if frame_references
                .get(reference)
                .is_some_and(|old| *old != id)
            {
                return Err(error(
                    s,
                    node,
                    format!("frame 的 id/name 冲突：{reference}"),
                ));
            }
            frame_references.insert(reference.to_owned(), id.to_owned());
        }
        if let Some(name) = name
            && frame_names.insert(name.to_owned(), id.to_owned()).is_some()
        {
            return Err(error(s, node, format!("重复 frame name：{name}")));
        }
        let visual = match node.attribute("visual") {
            Some(id) => visuals
                .get(id)
                .cloned()
                .ok_or_else(|| error(s, node, format!("未知视觉预设：{id}")))?,
            None => Visual::default(),
        };
        let speaker = node.attribute("speaker").map(str::to_owned);
        if let Some(name) = &speaker
            && !characters.contains_key(name)
        {
            return Err(error(s, node, format!("未知说话人：{name}")));
        }
        let text = plain_text(s, single(s, node, "text")?, "text", &[])?;
        let mut options = Vec::new();
        if node.children().any(|n| n.has_tag_name("options")) {
            let opt_node = single(s, node, "options")?;
            check(s, opt_node, "options", &[], &["option"], false)?;
            let mut option_ids = BTreeMap::new();
            for opt in elements(opt_node) {
                let text = plain_text(s, opt, "option", &["id"])?;
                let opt_id = required(s, opt, "id")?;
                unique(s, opt, &mut option_ids, opt_id, ())?;
                options.push(Choice {
                    id: opt_id.to_owned(),
                    text,
                });
            }
            if options.is_empty() {
                return Err(error(s, opt_node, "options 不能为空"));
            }
        }
        let mut audio = AudioCue::default();
        if node.children().any(|n| n.has_tag_name("audio")) {
            let audio_node = single(s, node, "audio")?;
            check(s, audio_node, "audio", &["bgm", "voice", "se"], &[], false)?;
            for (attribute, target) in [
                ("bgm", &mut audio.bgm),
                ("voice", &mut audio.voice),
                ("se", &mut audio.se),
            ] {
                if let Some(id) = audio_node.attribute(attribute) {
                    if !matches!(
                        assets.get(id),
                        Some(Asset {
                            kind: AssetKind::Audio,
                            ..
                        })
                    ) {
                        return Err(error(
                            s,
                            audio_node,
                            format!("音频引用不存在或类型错误：{id}"),
                        ));
                    }
                    *target = Some(id.to_owned());
                }
            }
        }
        unique(
            s,
            node,
            &mut frames,
            id,
            Frame {
                name: name.map(str::to_owned),
                video: media_xml::video(node, &assets)?,
                transition: if elements(node).any(|n| n.has_tag_name("transition")) {
                    effect_xml::transition(s, single(s, node, "transition")?)?
                } else {
                    Default::default()
                },
                id: id.to_owned(),
                visual,
                speaker,
                text,
                options,
                audio,
            },
        )?;
    }

    let node = single(s, story_node, "interface")?;
    check(s, node, "interface", &["src"], &[], false)?;
    let path = Path::new(required(s, node, "src")?);
    crate::content::validate_path(path).map_err(|e| error(s, node, e))?;
    let interface_path = path.to_owned();
    let entry = frame_references
        .get(required(s, story_node, "entry")?)
        .cloned()
        .ok_or_else(|| error(s, story_node, "入口场景不存在"))?;
    let scripts_node = single(s, story_node, "scripts")?;
    check(s, scripts_node, "scripts", &[], &["script"], false)?;
    let mut scripts = Vec::new();
    for script in elements(scripts_node) {
        check(s, script, "script", &["src"], &[], false)?;
        let path = Path::new(required(s, script, "src")?);
        crate::content::validate_path(path).map_err(|message| error(s, script, message))?;
        if path.extension().and_then(|item| item.to_str()) != Some("js")
            || scripts.contains(&path.to_path_buf())
        {
            return Err(error(s, script, "脚本必须为不重复的 .js 相对路径"));
        }
        scripts.push(path.to_path_buf());
    }
    if scripts.is_empty() || scripts.len() > 32 {
        return Err(error(s, scripts_node, "脚本数量必须为 1–32"));
    }
    Ok(ParsedStory {
        story: Story {
            frame_names,
            effects,
            id,
            content_version,
            entry,
            assets,
            characters,
            frames,
            routes: BTreeMap::new(),
            variables,
        },
        scripts,
        interface_path,
    })
}

/// Load the current contract from any validated content source. Paths remain logical.
pub fn load_game(
    source: &dyn crate::content::ContentSource,
) -> Result<(LoadedStory, crate::script::ScriptLogic)> {
    let compiled = crate::story_chain::compile(source)?;
    let mut parsed = parse_story(&compiled.xml).map_err(|e| compiled.diagnostic(e))?;
    let logic = crate::script::ScriptLogic::load(&mut parsed.story, source, &parsed.scripts)?;
    logic
        .options(&parsed.story, &parsed.story.start())
        .map_err(anyhow::Error::msg)?;
    for frame in parsed.story.frames.values() {
        if frame.video.as_ref().is_some_and(|v| v.advance)
            && parsed.story.routes[&frame.id] != Route::Advance
        {
            bail!("视频场景 {} 的 advance=true 需要 Advance 路由", frame.id);
        }
    }
    let text = crate::content::text(source, &parsed.interface_path)?;
    let interface = crate::interface::parse(
        &text,
        &parsed.interface_path.to_string_lossy(),
        &parsed.story,
        &logic.ui_handlers()?,
    )?;
    let loaded = LoadedStory {
        story: parsed.story,
        scripts: parsed.scripts,
        interface,
    };
    initial_ui_plan(&loaded, &logic)?;
    Ok((loaded, logic))
}
/// Evaluate the entry lifecycle against a fresh, IO-free snapshot for startup validation.
pub fn initial_ui_plan(
    loaded: &LoadedStory,
    logic: &crate::script::ScriptLogic,
) -> Result<Option<crate::interface::host::Plan>> {
    let def = &loaded.interface;
    if def.enter {
        use crate::interface::{
            Data, Event, data,
            host::{self, Profile, State},
        };
        let session = loaded.story.start();
        let state = State::new(def);
        let profile = Profile::new(&loaded.story.id, def);
        let settings = crate::settings::Settings::default();
        let available = logic
            .options(&loaded.story, &session)
            .map_err(anyhow::Error::msg)?;
        let data = Data::build(data::Input {
            definition: def,
            story: &loaded.story,
            session: &session,
            available: &available,
            state: &state,
            profile: &profile,
            slots: &[],
            settings: &settings,
            complete: false,
        });
        let commands = logic.ui_event(
            &Event {
                name: "game.enter".into(),
                item: serde_json::Value::Null,
                value: serde_json::Value::Null,
            },
            &session,
            &state.variables,
            &data.values,
        )?;
        let plan = host::prepare(def, &state, &settings, &profile, commands, true)?;
        crate::effects::validate_portraits(
            &loaded.story.frames[&session.frame_id],
            &plan.portraits,
        )
        .map_err(anyhow::Error::msg)?;
        return Ok(Some(plan));
    }
    Ok(None)
}
