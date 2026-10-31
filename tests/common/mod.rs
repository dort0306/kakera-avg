#![allow(dead_code)]
use kakera_avg::{
    content::{ContentSource, DirectorySource, validate_path},
    loader::{self, LoadedStory},
    script::ScriptLogic,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
pub const STORY: &str = include_str!("../fixtures/basic.xml");
pub const SCRIPT: &str = "frame('opening').advance(()=> 'question');frame('question').choice('accept', ()=> 'accept');frame('question').choice('refuse', ()=> 'refuse');frame('accept').end();frame('refuse').end();";
pub const UI: &str = include_str!("../fixtures/game/ui.xml");
pub struct Memory(pub BTreeMap<PathBuf, Vec<u8>>);
impl ContentSource for Memory {
    fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
        validate_path(path)?;
        self.0
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing {}", path.display()))
    }
}
pub fn source(xml: &str, script: &str) -> Memory {
    Memory(
        [
            ("story.xml", xml),
            ("ui.xml", UI),
            ("scripts/story.js", script),
            (
                "scripts/endings.js",
                include_str!("../fixtures/game/endings.js"),
            ),
        ]
        .into_iter()
        .map(|(p, t)| (p.into(), t.as_bytes().to_vec()))
        .collect(),
    )
}
pub fn load(xml: &str, script: &str) -> anyhow::Result<(LoadedStory, ScriptLogic)> {
    loader::load_game(&source(xml, script))
}
pub fn game() -> (LoadedStory, ScriptLogic) {
    load(STORY, SCRIPT).unwrap()
}
pub fn demo() -> DirectorySource {
    DirectorySource::new(&Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo")).unwrap()
}
/// Resolves the XML target that declares `name`, mirroring what the renderer reports.
/// Tests that drive a complete UI do not need to hardcode every control ID.
pub fn event_key(
    definition: &kakera_avg::interface::Definition,
    name: &str,
) -> kakera_avg::interface::UiEventKey {
    fn find(nodes: &[kakera_avg::interface::Node], name: &str) -> Option<String> {
        for node in nodes {
            if node.event.as_deref() == Some(name) {
                return Some(node.id.clone());
            }
            if let Some(id) = find(&node.children, name) {
                return Some(id);
            }
        }
        None
    }
    for (id, screen) in &definition.screens {
        if screen.background_click.as_deref() == Some(name)
            || screen.background_right_click.as_deref() == Some(name)
        {
            return kakera_avg::interface::UiEventKey::ui(id.clone(), name);
        }
        if let Some(id) = find(&screen.nodes, name) {
            return kakera_avg::interface::UiEventKey::ui(id, name);
        }
    }
    panic!("no XML target declares event {name}");
}
/// Appends a probe screen with one button per event name, id equal to the event name.
/// A complete UI fixture must declare every registered target, so probe callbacks need
/// a real control instead of a bare registration.
pub fn probe(ui: &str, names: &[&str]) -> String {
    let mut buttons = String::new();
    for name in names {
        buttons.push_str(&format!(
            r#"<button id="{name}" width="40" height="20" text="探针" event="{name}"/>"#
        ));
    }
    let screen = format!(r#"<screen id="probe">{buttons}</screen>"#);
    ui.replacen("</ui>", &format!("{screen}</ui>"), 1)
}
fn attribute(tag: &str, name: &str) -> Option<String> {
    let marker = format!(r#"{name}=""#);
    let start = tag.find(&marker)? + marker.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_owned())
}
/// Author-declared `(target, event)` pairs of one UI source, for tests that call
/// `interface::parse` directly instead of loading complete game content.
pub fn declarations(ui: &str) -> Vec<kakera_avg::interface::Registration> {
    let mut out: Vec<kakera_avg::interface::Registration> = Vec::new();
    for tag in ["button", "toggle", "slider", "color-picker"] {
        for piece in ui.split(&format!("<{tag}")).skip(1) {
            let Some(rest) = piece.split('>').next() else {
                continue;
            };
            let (Some(id), Some(event)) = (attribute(rest, "id"), attribute(rest, "event")) else {
                continue;
            };
            if out
                .iter()
                .any(|item| item.key.target == id && item.key.event == event)
            {
                continue;
            }
            out.push(kakera_avg::interface::Registration::ui(id, event));
        }
    }
    for piece in ui.split("<screen").skip(1) {
        let Some(rest) = piece.split('>').next() else {
            continue;
        };
        let Some(id) = attribute(rest, "id") else {
            continue;
        };
        for name in ["background-click", "background-right-click"] {
            let Some(event) = attribute(rest, name) else {
                continue;
            };
            if out
                .iter()
                .any(|item| item.key.target == id && item.key.event == event)
            {
                continue;
            }
            out.push(kakera_avg::interface::Registration::ui(id.clone(), event));
        }
    }
    out.into_iter()
        .map(|item| item.key.event)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(kakera_avg::interface::Registration::event)
        .collect()
}
