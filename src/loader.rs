use crate::model::*;
use anyhow::{Context, Result, anyhow, bail, ensure};
use roxmltree::{Document, Node};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

pub(crate) mod effect_xml;
mod media_xml;
pub(crate) mod rule_xml;
use rule_xml::Guard;

pub struct LoadedStory {
    pub story: Story,
    pub warnings: Vec<String>,
    pub scripts: Vec<std::path::PathBuf>,
    pub interface: Option<crate::interface::Definition>,
    pub interface_path: Option<std::path::PathBuf>,
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

/// Parse and validate data without touching the filesystem. Resource paths are relative to `root`.
pub fn parse_story(story_xml: &str, flow_xml: &str, root: &Path) -> Result<LoadedStory> {
    let content = Document::parse(story_xml).context("story.xml 解析失败")?;
    let s = "story.xml";
    let f = "flow.xml";
    let story_node = content.root_element();
    let schema = required(s, story_node, "schema")?;
    check(
        s,
        story_node,
        "story",
        if ["4", "5", "6"].contains(&schema) {
            &["schema", "id", "content-version", "entry"]
        } else {
            &["schema", "id", "content-version"]
        },
        if ["5", "6"].contains(&schema) {
            &[
                "assets",
                "characters",
                "visuals",
                "frames",
                "variables",
                "scripts",
                "interface",
                "effects",
            ]
        } else if schema == "4" {
            &[
                "assets",
                "characters",
                "visuals",
                "frames",
                "variables",
                "scripts",
            ]
        } else if schema == "3" {
            &["assets", "characters", "visuals", "frames", "variables"]
        } else {
            &["assets", "characters", "visuals", "frames"]
        },
        false,
    )?;
    if !["1", "2", "3", "4", "5", "6"].contains(&schema) {
        return Err(error(s, story_node, "不支持的 story schema"));
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
    let variables = rule_xml::variables(story_node)?;
    let effects = effect_xml::presets(story_node)?;

    let mut assets = BTreeMap::new();
    let asset_node = single(s, story_node, "assets")?;
    let asset_tags: &[&str] = if schema == "6" {
        &["image", "font", "audio", "video", "live2d"]
    } else if schema != "1" {
        &["image", "font", "audio"]
    } else {
        &["image", "font"]
    };
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
                path: root.join(relative),
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
            if ["5", "6"].contains(&schema) {
                &["id", "background", "background-effect"]
            } else {
                &["id", "background"]
            },
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
                if ["5", "6"].contains(&schema) {
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
                    ]
                } else {
                    &["character", "image", "slot"]
                },
                if schema == "6" { &["live2d"] } else { &[] },
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
    let mut frame_nodes = BTreeMap::new();
    let frames_node = single(s, story_node, "frames")?;
    check(s, frames_node, "frames", &[], &["frame"], false)?;
    for node in elements(frames_node) {
        check(
            s,
            node,
            "frame",
            &["id", "visual", "speaker"],
            if schema == "6" {
                &["text", "options", "audio", "transition", "video"]
            } else if schema == "5" {
                &["text", "options", "audio", "transition"]
            } else if schema != "1" {
                &["text", "options", "audio"]
            } else {
                &["text", "options"]
            },
            false,
        )?;
        let id = required(s, node, "id")?;
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
        frame_nodes.insert(id.to_owned(), node);
    }

    if ["4", "5", "6"].contains(&schema) {
        let interface_path = if ["5", "6"].contains(&schema) {
            let node = single(s, story_node, "interface")?;
            check(s, node, "interface", &["src"], &[], false)?;
            let path = Path::new(required(s, node, "src")?);
            crate::content::validate_path(path).map_err(|e| error(s, node, e))?;
            Some(path.to_owned())
        } else {
            None
        };
        let entry = required(s, story_node, "entry")?.to_owned();
        if !frames.contains_key(&entry) {
            return Err(error(s, story_node, "入口场景不存在"));
        }
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
        return Ok(LoadedStory {
            story: Story {
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
            warnings: Vec::new(),
            scripts,
            interface: None,
            interface_path,
        });
    }
    let flow_doc = Document::parse(flow_xml).context("flow.xml 解析失败")?;
    let flow = flow_doc.root_element();
    let flow_schema = required(f, flow, "schema")?;
    check(
        f,
        flow,
        "flow",
        &["schema", "story", "entry"],
        if flow_schema == "2" {
            &["next", "choice", "end", "branch"]
        } else {
            &["next", "choice", "end"]
        },
        false,
    )?;
    if !["1", "2"].contains(&flow_schema) {
        return Err(error(f, flow, "不支持的 flow schema"));
    }
    if flow_schema == "2" && schema != "3" {
        return Err(error(f, flow, "flow schema 2 需要 story schema 3"));
    }
    if required(f, flow, "story")? != id {
        return Err(error(f, flow, "flow 的 story 与故事 ID 不匹配"));
    }
    let entry = required(f, flow, "entry")?.to_owned();
    if !frames.contains_key(&entry) {
        return Err(error(f, flow, format!("入口场景不存在：{entry}")));
    }
    let mut routes = BTreeMap::new();
    for node in elements(flow) {
        let (from, route) = match node.tag_name().name() {
            "next" => {
                check(
                    f,
                    node,
                    "next",
                    &["from", "to"],
                    if flow_schema == "2" {
                        &["effects"]
                    } else {
                        &[]
                    },
                    false,
                )?;
                (
                    required(f, node, "from")?,
                    Route::Next(rule_xml::transition(f, node, &variables, Guard::Forbidden)?),
                )
            }
            "choice" => {
                check(f, node, "choice", &["from"], &["on"], false)?;
                let mut choices = BTreeMap::new();
                for on in elements(node) {
                    check(
                        f,
                        on,
                        "on",
                        &["option", "to"],
                        if flow_schema == "2" {
                            &["when", "effects"]
                        } else {
                            &[]
                        },
                        false,
                    )?;
                    unique(
                        f,
                        on,
                        &mut choices,
                        required(f, on, "option")?,
                        rule_xml::transition(f, on, &variables, Guard::Optional)?,
                    )?;
                }
                (required(f, node, "from")?, Route::Choice(choices))
            }
            "branch" => {
                check(f, node, "branch", &["from"], &["case", "otherwise"], false)?;
                let children: Vec<_> = elements(node).collect();
                let fallback = single(f, node, "otherwise")?;
                if children.last().copied() != Some(fallback) || children.len() < 2 {
                    return Err(error(
                        f,
                        node,
                        "branch 至少一个 case，otherwise 必须恰好一个且在最后",
                    ));
                }
                let mut cases = Vec::new();
                for case in children
                    .into_iter()
                    .filter(|child| child.has_tag_name("case"))
                {
                    check(f, case, "case", &["to"], &["when", "effects"], false)?;
                    cases.push(rule_xml::transition(f, case, &variables, Guard::Required)?);
                }
                check(f, fallback, "otherwise", &["to"], &["effects"], false)?;
                let otherwise = rule_xml::transition(f, fallback, &variables, Guard::Forbidden)?;
                (
                    required(f, node, "from")?,
                    Route::Branch { cases, otherwise },
                )
            }
            "end" => {
                check(f, node, "end", &["frame"], &[], false)?;
                (required(f, node, "frame")?, Route::End)
            }
            _ => unreachable!(),
        };
        let frame = frames
            .get(from)
            .ok_or_else(|| error(f, node, format!("流转源场景不存在：{from}")))?;
        for target in route.targets() {
            if !frames.contains_key(target) {
                return Err(error(f, node, format!("目标场景不存在：{target}")));
            }
        }
        match &route {
            Route::Choice(choices) => {
                let expected: BTreeSet<_> = frame.options.iter().map(|o| &o.id).collect();
                let actual: BTreeSet<_> = choices.keys().collect();
                if expected.is_empty() || expected != actual {
                    return Err(error(
                        f,
                        node,
                        format!("场景 {from} 的选项与路由必须一一匹配"),
                    ));
                }
            }
            _ if !frame.options.is_empty() => {
                return Err(error(
                    f,
                    node,
                    format!("场景 {from} 有选项，必须使用 choice"),
                ));
            }
            _ => {}
        }
        unique(f, node, &mut routes, from, route)?;
    }
    for id in frames.keys() {
        if !routes.contains_key(id) {
            return Err(error(s, frame_nodes[id], format!("场景 {id} 缺少显式流转")));
        }
    }
    let mut reached = BTreeSet::new();
    let mut pending = vec![entry.clone()];
    while let Some(id) = pending.pop() {
        if !reached.insert(id.clone()) {
            continue;
        }
        pending.extend(routes[&id].targets().into_iter().map(str::to_owned));
    }
    let warnings = frames
        .keys()
        .filter(|id| !reached.contains(*id))
        .map(|id| format!("不可达场景：{id}"))
        .collect();
    Ok(LoadedStory {
        story: Story {
            effects,
            id,
            content_version,
            entry,
            assets,
            characters,
            frames,
            routes,
            variables,
        },
        warnings,
        scripts: Vec::new(),
        interface: None,
        interface_path: None,
    })
}

pub fn load_story(root: &Path) -> Result<LoadedStory> {
    let root = root
        .canonicalize()
        .with_context(|| format!("无法打开游戏目录 {}", root.display()))?;
    let story_path = root.join("story.xml");
    let flow_path = root.join("flow.xml");
    let story = std::fs::read_to_string(&story_path)
        .with_context(|| format!("读取 {}", story_path.display()))?;
    let scripted = matches!(
        Document::parse(&story)?.root_element().attribute("schema"),
        Some("4" | "5" | "6")
    );
    let flow = if scripted {
        String::new()
    } else {
        std::fs::read_to_string(&flow_path)
            .with_context(|| format!("读取 {}", flow_path.display()))?
    };
    let mut loaded = parse_story(&story, &flow, &root)
        .with_context(|| format!("游戏目录 {}", root.display()))?;
    if scripted {
        let source = crate::content::DirectorySource::new(&root)?;
        let script = crate::script::ScriptLogic::load(&mut loaded.story, &source, &loaded.scripts)?;
        attach_interface(
            &mut loaded,
            &source,
            &crate::script::LogicEngine::Script(script),
        )?;
    }
    for (id, asset) in &loaded.story.assets {
        let resolved = asset
            .path
            .canonicalize()
            .with_context(|| format!("资源 {id} 缺失：{}", asset.path.display()))?;
        ensure!(
            resolved.starts_with(&root),
            "资源 {id} 位于游戏目录之外：{}",
            asset.path.display()
        );
        if !resolved.is_file() {
            bail!("资源 {id} 不是普通文件：{}", asset.path.display());
        }
    }
    Ok(loaded)
}

/// Runtime entry point for directories, memory or a future archive. Paths stay logical.
pub fn load_game(
    source: &dyn crate::content::ContentSource,
) -> Result<(LoadedStory, crate::script::LogicEngine)> {
    use crate::{
        content,
        script::{LogicEngine, ScriptLogic},
    };
    let story = content::text(source, Path::new("story.xml"))?;
    let scripted = matches!(
        Document::parse(&story)
            .context("story.xml 解析失败")?
            .root_element()
            .attribute("schema"),
        Some("4" | "5" | "6")
    );
    let flow = if scripted {
        String::new()
    } else {
        content::text(source, Path::new("flow.xml"))?
    };
    let mut loaded = parse_story(&story, &flow, Path::new(""))?;
    let logic = if scripted {
        LogicEngine::Script(ScriptLogic::load(
            &mut loaded.story,
            source,
            &loaded.scripts,
        )?)
    } else {
        LogicEngine::Declarative
    };
    logic
        .options(&loaded.story, &loaded.story.start())
        .map_err(anyhow::Error::msg)?;
    for frame in loaded.story.frames.values() {
        if frame.video.as_ref().is_some_and(|v| v.advance)
            && !matches!(
                loaded.story.routes[&frame.id],
                Route::Next(_) | Route::Branch { .. } | Route::Script(ScriptRoute::Advance)
            )
        {
            bail!("视频场景 {} 的 advance=true 需要 Advance 路由", frame.id);
        }
    }
    attach_interface(&mut loaded, source, &logic)?;
    Ok((loaded, logic))
}

fn attach_interface(
    loaded: &mut LoadedStory,
    source: &dyn crate::content::ContentSource,
    logic: &crate::script::LogicEngine,
) -> Result<()> {
    if let Some(path) = &loaded.interface_path {
        let text = crate::content::text(source, path)?;
        loaded.interface = Some(crate::interface::parse(
            &text,
            &path.to_string_lossy(),
            &loaded.story,
            &logic.ui_handlers()?,
        )?);
    }
    initial_ui_plan(loaded, logic)?;
    Ok(())
}

/// Evaluate the entry lifecycle against a fresh, IO-free snapshot for startup validation.
pub fn initial_ui_plan(
    loaded: &LoadedStory,
    logic: &crate::script::LogicEngine,
) -> Result<Option<crate::interface::host::Plan>> {
    if let Some(def) = &loaded.interface
        && def.enter
    {
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
