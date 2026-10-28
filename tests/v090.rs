use anyhow::Result;
use kakera_avg::{
    content::{ContentSource, validate_path},
    controller::{self, Intent},
    loader,
    model::{Logic, MAX_HISTORY, Session, Story, TransitionRule, Variables},
    save::Snapshot,
};
use std::{collections::BTreeMap, path::Path};
struct Memory(BTreeMap<String, Vec<u8>>);
impl ContentSource for Memory {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        validate_path(path)?;
        self.0
            .get(path.to_str().unwrap())
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing"))
    }
}
fn source(extra: &str) -> Memory {
    Memory([("story.xml",r#"<story schema="8" id="callbacks" content-version="9" entry="a"><scripts><script src="story.js"/></scripts><interface src="ui.xml"/><assets><font id="font" src="font.otf"/><video id="intro" src="intro.mp4"/></assets><variables><int id="n" initial="0" min="0" max="10"/></variables><characters/><visuals/><frames><frame id="a" name="alias"><text>一</text></frame><frame id="b"><text>二</text></frame></frames><next end="true"/></story>"#.into()),("ui.xml",r#"<ui schema="3" initial="hud"><screen id="hud" modal="false" pause="false"><scene id="scene" width="1280" height="720"/></screen><screen id="movie"/></ui>"#.into()),("story.js",format!("on.advance('a',()=> 'b');on.end('b');{extra}"))].into_iter().map(|(p,s):( &str,String)|(p.into(),s.into_bytes())).collect())
}
#[test]
fn lifecycle_registration_resolves_names_and_rejects_bad_or_duplicate_targets() -> Result<()> {
    let (loaded, _) = loader::load_game(&source(
        "on.frameEnter('alias',ctx=>{});on.frameExit('a',ctx=>{});on.screenOpen('movie',ctx=>{});on.screenClose('movie',ctx=>{});",
    ))?;
    assert!(loaded.interface.lifecycle.contains_key("frame.enter:a"));
    assert_eq!(loaded.interface.lifecycle.len(), 4);
    for js in [
        "on.frameEnter('missing',ctx=>{});",
        "on.screenOpen('missing',ctx=>{});",
        "on.frameEnter('a',ctx=>{});on.frameEnter('alias',ctx=>{});",
        "on.screenClose('movie',ctx=>{});on.screenClose('movie',ctx=>{});",
    ] {
        assert!(loader::load_game(&source(js)).is_err(), "{js}");
    }
    Ok(())
}
#[test]
fn frame_video_and_old_video_bindings_are_strictly_rejected() -> Result<()> {
    for child in ["<video asset=\"intro\"/>", "<video/>"] {
        let mut m = source("");
        let story = String::from_utf8(m.0["story.xml"].clone())?
            .replace("<text>一</text>", &format!("{child}<text>一</text>"));
        m.0.insert("story.xml".into(), story.into_bytes());
        let error = loader::load_game(&m).err().unwrap();
        assert!(format!("{error:#}").contains("story.xml:"));
    }
    for bind in ["game.videoPlaying", "game.videoSkippable"] {
        let mut m = source("");
        m.0.insert("ui.xml".into(),format!(r#"<ui schema="3" initial="hud"><screen id="hud" modal="false"><label id="label" width="100" height="30" bind="{bind}"/></screen></ui>"#).into_bytes());
        assert!(loader::load_game(&m).is_err());
    }
    Ok(())
}
struct Pure;
impl Logic for Pure {
    fn resolve(
        &self,
        _: &Story,
        frame: &str,
        _: Option<&str>,
        _: &Variables,
    ) -> std::result::Result<Option<TransitionRule>, String> {
        Ok(Some(TransitionRule::plain(if frame == "a" {
            "b"
        } else {
            "a"
        })))
    }
    fn options(&self, _: &Story, _: &Session) -> std::result::Result<Vec<String>, String> {
        Ok(vec![])
    }
}
#[test]
fn ui_return_history_trims_replays_and_save9_rejects_old_or_forged_entries() -> Result<()> {
    let (loaded, _) = loader::load_game(&source(""))?;
    let story = &loaded.story;
    let mut session = story.start();
    for i in 0..MAX_HISTORY + 7 {
        let target = if i % 2 == 0 { "b" } else { "a" };
        session =
            controller::transition_with(story, &session, Intent::Resume(target.into()), &Pure)?
                .session;
    }
    assert_eq!(session.history.len(), MAX_HISTORY);
    story
        .validate_session_with(&session, &Pure)
        .map_err(anyhow::Error::msg)?;
    let snapshot = Snapshot::capture_with(story, &session, &Pure)?;
    assert_eq!(snapshot.schema_version, 9);
    assert_eq!(snapshot.restore_with(story, &Pure)?, session);
    let mut old = snapshot.clone();
    old.schema_version = 8;
    assert!(old.restore_with(story, &Pure).is_err());
    let mut bad = snapshot.clone();
    bad.history[0].choice_id = Some("forged".into());
    assert!(bad.restore_with(story, &Pure).is_err());
    let mut bad = snapshot.clone();
    bad.history[0].resume_to = Some("missing".into());
    assert!(bad.restore_with(story, &Pure).is_err());
    let mut json = serde_json::to_value(snapshot)?;
    json["history"][0]
        .as_object_mut()
        .unwrap()
        .remove("resume_to");
    assert!(serde_json::from_value::<Snapshot>(json).is_err());
    Ok(())
}
#[test]
fn hidden_file_is_not_collected_in_project_or_package_snapshots() -> Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("story.xml"), b"story")?;
    std::fs::write(
        dir.path().join("flow-checkpoint.json"),
        b"private player state",
    )?;
    let files = kakera_avg::package::files(dir.path())?;
    assert_eq!(files, vec![std::path::PathBuf::from("story.xml")]);
    let snapshot = kakera_avg::package::Snapshot::directory(dir.path())?;
    assert!(snapshot.read(Path::new("flow-checkpoint.json")).is_err());
    Ok(())
}
