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
pub const SCRIPT: &str = "on.advance('opening',()=> 'question');on.choice('question','accept',()=> 'accept');on.choice('question','refuse',()=> 'refuse');on.end('accept');on.end('refuse');";
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
