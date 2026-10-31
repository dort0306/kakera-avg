use crate::model::*;
use anyhow::{Context, Result, anyhow};
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
        return Err(error(
            file,
            node,
            crate::i18n::format("host-loader-0001", &[("arg0", tag.to_string())]),
        ));
    }
    for attr in node.attributes() {
        if attr.namespace().is_some() || !attrs.contains(&attr.name()) {
            return Err(error(
                file,
                node,
                crate::i18n::format("host-loader-0002", &[("arg0", attr.name().to_string())]),
            ));
        }
    }
    for child in node.children() {
        if child.is_element() && !children.contains(&child.tag_name().name()) {
            return Err(error(
                file,
                child,
                crate::i18n::format(
                    "host-loader-0003",
                    &[("arg0", child.tag_name().name().to_string())],
                ),
            ));
        }
        if !text && child.is_text() && !child.text().unwrap_or("").trim().is_empty() {
            return Err(error(file, child, crate::i18n::text("host-loader-0019")));
        }
    }
    Ok(())
}

pub(crate) fn required<'a>(file: &str, node: Node<'a, '_>, name: &str) -> Result<&'a str> {
    node.attribute(name)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            error(
                file,
                node,
                crate::i18n::format("host-loader-0004", &[("arg0", name.to_string())]),
            )
        })
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
            crate::i18n::format("host-loader-0005", &[("arg0", format!("{:?}", id))]),
        ));
    }
    if map.insert(id.to_owned(), value).is_some() {
        return Err(error(
            file,
            node,
            crate::i18n::format("host-loader-0006", &[("arg0", id.to_string())]),
        ));
    }
    Ok(())
}

pub(crate) fn single<'a, 'input>(
    file: &str,
    parent: Node<'a, 'input>,
    name: &str,
) -> Result<Node<'a, 'input>> {
    let mut nodes = parent.children().filter(|n| n.has_tag_name(name));
    let node = nodes.next().ok_or_else(|| {
        error(
            file,
            parent,
            crate::i18n::format("host-loader-0007", &[("arg0", name.to_string())]),
        )
    })?;
    if nodes.next().is_some() {
        return Err(error(
            file,
            parent,
            crate::i18n::format("host-loader-0008", &[("arg0", name.to_string())]),
        ));
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
        return Err(error(file, node, crate::i18n::text("host-loader-0020")));
    }
    Ok(text)
}

fn localized_plain(
    file: &str,
    node: Node<'_, '_>,
    tag: &str,
    attrs: &[&str],
    key_attr: &str,
    catalog: &crate::localization::Catalog,
) -> Result<String> {
    if let Some(key) = node.attribute(key_attr) {
        let mut allowed = attrs.to_vec();
        allowed.push(key_attr);
        check(file, node, tag, &allowed, &[], false)?;
        return catalog.static_text(key).map_err(|e| error(file, node, e));
    }
    plain_text(file, node, tag, attrs)
}
fn localized_attribute(
    file: &str,
    node: Node<'_, '_>,
    literal: &str,
    key: &str,
    catalog: &crate::localization::Catalog,
) -> Result<String> {
    match (node.attribute(literal), node.attribute(key)) {
        (Some(value), None) => Ok(value.into()),
        (None, Some(key)) => catalog.static_text(key).map_err(|e| error(file, node, e)),
        _ => Err(error(
            file,
            node,
            crate::i18n::format(
                "host-i18n-detail-0066",
                &[("arg0", literal.to_string()), ("arg1", key.to_string())],
            ),
        )),
    }
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
        return Err(error(
            file,
            node,
            crate::i18n::format("host-loader-0009", &[("arg0", id.to_string())]),
        ));
    }
    Ok(())
}

/// Parse the assembled current grammar into owned data; resource paths stay logical.
fn parse_story(story_xml: &str, catalog: &crate::localization::Catalog) -> Result<ParsedStory> {
    let content = Document::parse(story_xml).context(crate::i18n::text("host-loader-0021"))?;
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
            "localization",
        ],
        false,
    )?;
    if schema != "8" {
        return Err(error(s, story_node, crate::i18n::text("host-loader-0022")));
    }
    if crate::story_chain::next(s, story_node)?.is_some() {
        return Err(error(s, story_node, crate::i18n::text("host-loader-0023")));
    }
    let id = required(s, story_node, "id")?.to_owned();
    if id == "."
        || id == ".."
        || id
            .chars()
            .any(|c| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '.')
    {
        return Err(error(s, story_node, crate::i18n::text("host-loader-0024")));
    }
    let content_version: u32 = required(s, story_node, "content-version")?
        .parse()
        .map_err(|_| error(s, story_node, crate::i18n::text("host-loader-0025")))?;
    if content_version == 0 {
        return Err(error(s, story_node, crate::i18n::text("host-loader-0026")));
    }
    let variables = variables::variables(story_node, catalog)?;
    let effects = effect_xml::presets(story_node)?;

    let mut assets = BTreeMap::new();
    let asset_node = single(s, story_node, "assets")?;
    let asset_tags = &[
        "cg",
        "image",
        "nine-patch",
        "font",
        "audio",
        "video",
        "live2d",
    ];
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
                crate::i18n::format("host-loader-0010", &[("arg0", id.to_string())]),
            ));
        }
        let kind = match node.tag_name().name() {
            "video" => AssetKind::Video,
            "live2d" => AssetKind::Live2D,
            "image" => AssetKind::Image,
            "cg" => AssetKind::Cg,
            "nine-patch" => {
                if !relative.to_string_lossy().ends_with(".9.png") {
                    return Err(error(s, node, crate::i18n::text("host-loader-0027")));
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
                    return Err(error(s, node, crate::i18n::text("host-loader-0028")));
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
        return Err(error(s, asset_node, crate::i18n::text("host-loader-0029")));
    }
    let mut characters = BTreeMap::new();
    let chars_node = single(s, story_node, "characters")?;
    check(s, chars_node, "characters", &[], &["character"], false)?;
    for node in elements(chars_node) {
        check(
            s,
            node,
            "character",
            &["id", "name", "name-key"],
            &[],
            false,
        )?;
        unique(
            s,
            node,
            &mut characters,
            required(s, node, "id")?,
            localized_attribute(s, node, "name", "name-key", catalog)?,
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
            &["id", "background", "cg", "background-effect"],
            &["portrait"],
            false,
        )?;
        let cg = node.attribute("cg");
        if cg.is_some()
            && (node.attribute("background").is_some() || elements(node).next().is_some())
        {
            return Err(error(s, node, crate::i18n::text("host-loader-0030")));
        }
        let background = if let Some(id) = cg {
            if !assets.get(id).is_some_and(|a| a.kind == AssetKind::Cg) {
                return Err(error(
                    s,
                    node,
                    crate::i18n::format("host-loader-0011", &[("arg0", id.to_string())]),
                ));
            }
            Some(id.to_owned())
        } else {
            let background = node.attribute("background").map(str::to_owned);
            if let Some(bg) = &background {
                image_ref(s, node, &assets, bg)?;
            }
            background
        };
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
                return Err(error(
                    s,
                    p,
                    crate::i18n::format("host-loader-0012", &[("arg0", character.to_string())]),
                ));
            }
            let image = required(s, p, "image")?;
            image_ref(s, p, &assets, image)?;
            let slot = match required(s, p, "slot")? {
                "left" => Slot::Left,
                "center" => Slot::Center,
                "right" => Slot::Right,
                value => {
                    return Err(error(
                        s,
                        p,
                        crate::i18n::format("host-loader-0013", &[("arg0", value.to_string())]),
                    ));
                }
            };
            if !slots.insert(slot) {
                return Err(error(s, p, crate::i18n::text("host-loader-0031")));
            }
            let (pose, enter, exit) = effect_xml::portrait(s, p)?;
            let live2d = media_xml::live2d(p, &assets)?;
            let effect = effect_xml::reference(s, p, "effect", &effects)?;
            if live2d.is_some() && effect.filter != crate::model::effects::Filter::None {
                return Err(error(s, p, crate::i18n::text("host-loader-0032")));
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
            &["text", "options", "audio", "transition"],
            false,
        )?;
        let name = node
            .attribute("name")
            .map(|_| required(s, node, "name"))
            .transpose()?;
        let id = if node.attribute("id").is_some() {
            required(s, node, "id")?
        } else {
            name.ok_or_else(|| error(s, node, crate::i18n::text("host-loader-0033")))?
        };
        for reference in std::iter::once(id).chain(name) {
            if reference.chars().any(char::is_whitespace) {
                return Err(error(s, node, crate::i18n::text("host-loader-0034")));
            }
            if frame_references
                .get(reference)
                .is_some_and(|old| *old != id)
            {
                return Err(error(
                    s,
                    node,
                    crate::i18n::format("host-loader-0014", &[("arg0", reference.to_string())]),
                ));
            }
            frame_references.insert(reference.to_owned(), id.to_owned());
        }
        if let Some(name) = name
            && frame_names.insert(name.to_owned(), id.to_owned()).is_some()
        {
            return Err(error(
                s,
                node,
                crate::i18n::format("host-loader-0015", &[("arg0", name.to_string())]),
            ));
        }
        let visual = match node.attribute("visual") {
            Some(id) => visuals.get(id).cloned().ok_or_else(|| {
                error(
                    s,
                    node,
                    crate::i18n::format("host-loader-0016", &[("arg0", id.to_string())]),
                )
            })?,
            None => Visual::default(),
        };
        let speaker = node.attribute("speaker").map(str::to_owned);
        if let Some(name) = &speaker
            && !characters.contains_key(name)
        {
            return Err(error(
                s,
                node,
                crate::i18n::format("host-loader-0017", &[("arg0", name.to_string())]),
            ));
        }
        let text = localized_plain(s, single(s, node, "text")?, "text", &[], "key", catalog)?;
        let mut options = Vec::new();
        if node.children().any(|n| n.has_tag_name("options")) {
            let opt_node = single(s, node, "options")?;
            check(s, opt_node, "options", &[], &["option"], false)?;
            let mut option_ids = BTreeMap::new();
            for opt in elements(opt_node) {
                let text = localized_plain(s, opt, "option", &["id"], "text-key", catalog)?;
                let opt_id = required(s, opt, "id")?;
                unique(s, opt, &mut option_ids, opt_id, ())?;
                options.push(Choice {
                    id: opt_id.to_owned(),
                    text,
                });
            }
            if options.is_empty() {
                return Err(error(s, opt_node, crate::i18n::text("host-loader-0035")));
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
                            crate::i18n::format("host-loader-0018", &[("arg0", id.to_string())]),
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
    let mut entries = Vec::new();
    for reference in required(s, story_node, "entry")?.split_whitespace() {
        let id = frame_references
            .get(reference)
            .ok_or_else(|| error(s, story_node, crate::i18n::text("host-loader-0036")))?;
        if entries.contains(id) {
            return Err(error(s, story_node, crate::i18n::text("host-loader-0037")));
        }
        entries.push(id.clone());
    }
    let entry = entries
        .first()
        .cloned()
        .ok_or_else(|| error(s, story_node, crate::i18n::text("host-loader-0038")))?;
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
            return Err(error(s, script, crate::i18n::text("host-loader-0039")));
        }
        scripts.push(path.to_path_buf());
    }
    if scripts.is_empty() || scripts.len() > 32 {
        return Err(error(
            s,
            scripts_node,
            crate::i18n::text("host-loader-0040"),
        ));
    }
    Ok(ParsedStory {
        story: Story {
            frame_names,
            effects,
            id,
            content_version,
            entry,
            entries,
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
    let catalog = std::sync::Arc::new(
        crate::localization::Catalog::from_story(&compiled.xml, source)
            .map_err(|e| compiled.diagnostic(e))?,
    );
    let mut parsed = parse_story(&compiled.xml, &catalog).map_err(|e| compiled.diagnostic(e))?;
    catalog.validate(&parsed.story)?;
    let mut logic = crate::script::ScriptLogic::load(&mut parsed.story, source, &parsed.scripts)?;
    for entry in &parsed.story.entries {
        logic
            .options(
                &parsed.story,
                &parsed.story.start_at(entry).map_err(anyhow::Error::msg)?,
            )
            .map_err(anyhow::Error::msg)?;
    }
    let ui = crate::interface::source::compile(source, &parsed.interface_path)?;
    let registrations = logic.ui_registrations()?;
    let components = logic.component_library()?;
    let interface = ui.parse_localized(
        &parsed.story,
        &registrations,
        &components,
        Some(&logic),
        catalog,
    )?;
    validate_initial_pages(&interface, &parsed.story)?;
    logic.bind_interface(&interface);
    let loaded = LoadedStory {
        story: parsed.story,
        scripts: parsed.scripts,
        interface,
    };
    initial_ui_plans(&loaded, &logic)?;
    Ok((loaded, logic))
}
/// Initial pages must fit the startup candidate; only natural later shrink clamps.
fn validate_initial_pages(
    def: &crate::interface::Definition,
    story: &crate::model::Story,
) -> Result<()> {
    use crate::interface::pagination;
    let has_pages = def.screens.values().any(|screen| {
        fn walk(nodes: &[crate::interface::Node]) -> bool {
            nodes.iter().any(|node| {
                matches!(
                    node.kind,
                    crate::interface::Kind::Repeat {
                        pagination: Some(_),
                        ..
                    }
                ) || walk(&node.children)
            })
        }
        walk(&screen.nodes)
    });
    if !has_pages {
        return Ok(());
    }
    let totals = pagination::Totals {
        choices: 0,
        history: 0,
        gallery: def.gallery.len(),
        variables: story.variables.len(),
        text_locales: def.localization.texts.len(),
        voice_locales: def.localization.voices.len(),
        autosaves: 0,
        slots: crate::save::SLOT_COUNT as usize,
    };
    let variables = def.initial_variables();
    let (plans, normalized) = pagination::plans(def, &variables, &totals);
    for plan in plans {
        if normalized.contains_key(&plan.variable) {
            anyhow::bail!(
                "{}",
                crate::i18n::format(
                    "host-interface-pagination-0010",
                    &[("arg0", plan.variable.clone())]
                )
            );
        }
    }
    Ok(())
}

/// Evaluate the entry lifecycle against a fresh, IO-free snapshot for startup validation.
pub fn initial_ui_plans(
    loaded: &LoadedStory,
    logic: &crate::script::ScriptLogic,
) -> Result<Vec<(String, crate::interface::host::Plan)>> {
    let def = &loaded.interface;
    let mut plans = Vec::new();
    if def.enter {
        use crate::interface::{
            Data, data,
            host::{self, Profile, State},
        };
        for entry in &loaded.story.entries {
            let session = loaded.story.start_at(entry).map_err(anyhow::Error::msg)?;
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
                slot_pages: &[],
                autosaves: &[],
            });
            let commands = logic.ui_event(
                &crate::interface::Event::game(),
                &session,
                &state.variables,
                &data.values,
            )?;
            let plan = host::prepare_with_pages(
                def,
                &state,
                &settings,
                &profile,
                commands,
                true,
                Some(&data.page_limits),
            )?;
            crate::effects::validate_portraits(
                &loaded.story.frames[&session.frame_id],
                &plan.portraits,
            )
            .map_err(anyhow::Error::msg)?;
            plans.push((session.frame_id.clone(), plan));
        }
    }
    Ok(plans)
}
