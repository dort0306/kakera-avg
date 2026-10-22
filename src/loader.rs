use crate::model::*;
use anyhow::{Context, Result, anyhow, bail, ensure};
use roxmltree::{Document, Node};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

pub struct LoadedStory {
    pub story: Story,
    pub warnings: Vec<String>,
}

fn error(file: &str, node: Node<'_, '_>, message: impl std::fmt::Display) -> anyhow::Error {
    let pos = node.document().text_pos_at(node.range().start);
    anyhow!("{file}:{}:{}: {message}", pos.row, pos.col)
}

fn check(
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

fn required<'a>(file: &str, node: Node<'a, '_>, name: &str) -> Result<&'a str> {
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

fn single<'a, 'input>(
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

fn elements<'a, 'input>(node: Node<'a, 'input>) -> impl Iterator<Item = Node<'a, 'input>> {
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
    let flow_doc = Document::parse(flow_xml).context("flow.xml 解析失败")?;
    let s = "story.xml";
    let f = "flow.xml";
    let story_node = content.root_element();
    check(
        s,
        story_node,
        "story",
        &["schema", "id", "content-version"],
        &["assets", "characters", "visuals", "frames"],
        false,
    )?;
    if required(s, story_node, "schema")? != "1" {
        return Err(error(s, story_node, "不支持的 story schema"));
    }
    let id = required(s, story_node, "id")?.to_owned();
    if id
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

    let mut assets = BTreeMap::new();
    let asset_node = single(s, story_node, "assets")?;
    check(s, asset_node, "assets", &[], &["image", "font"], false)?;
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
        let kind = if node.has_tag_name("image") {
            AssetKind::Image
        } else {
            AssetKind::Font
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
            &["id", "background"],
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
                &["character", "image", "slot"],
                &[],
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
            portraits.push(Portrait {
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
            &["text", "options"],
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
        unique(
            s,
            node,
            &mut frames,
            id,
            Frame {
                id: id.to_owned(),
                visual,
                speaker,
                text,
                options,
            },
        )?;
        frame_nodes.insert(id.to_owned(), node);
    }

    let flow = flow_doc.root_element();
    check(
        f,
        flow,
        "flow",
        &["schema", "story", "entry"],
        &["next", "choice", "end"],
        false,
    )?;
    if required(f, flow, "schema")? != "1" {
        return Err(error(f, flow, "不支持的 flow schema"));
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
                check(f, node, "next", &["from", "to"], &[], false)?;
                (
                    required(f, node, "from")?,
                    Route::Next(required(f, node, "to")?.to_owned()),
                )
            }
            "choice" => {
                check(f, node, "choice", &["from"], &["on"], false)?;
                let mut choices = BTreeMap::new();
                for on in elements(node) {
                    check(f, on, "on", &["option", "to"], &[], false)?;
                    unique(
                        f,
                        on,
                        &mut choices,
                        required(f, on, "option")?,
                        required(f, on, "to")?.to_owned(),
                    )?;
                }
                (required(f, node, "from")?, Route::Choice(choices))
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
        let targets: Vec<&str> = match &route {
            Route::Next(to) => vec![to],
            Route::Choice(choices) => choices.values().map(String::as_str).collect(),
            Route::End => vec![],
        };
        for target in targets {
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
        match &routes[&id] {
            Route::Next(to) => pending.push(to.clone()),
            Route::Choice(choices) => pending.extend(choices.values().cloned()),
            Route::End => {}
        }
    }
    let warnings = frames
        .keys()
        .filter(|id| !reached.contains(*id))
        .map(|id| format!("不可达场景：{id}"))
        .collect();
    Ok(LoadedStory {
        story: Story {
            id,
            content_version,
            entry,
            assets,
            characters,
            frames,
            routes,
        },
        warnings,
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
    let flow = std::fs::read_to_string(&flow_path)
        .with_context(|| format!("读取 {}", flow_path.display()))?;
    let loaded = parse_story(&story, &flow, &root)
        .with_context(|| format!("游戏目录 {}", root.display()))?;
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
