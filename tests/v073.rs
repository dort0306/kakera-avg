//! v0.7.3: Ctrl fast-forward setting, read marks in user data, slot previews without frame ids.
use anyhow::Result;
use kakera_avg::{
    content::{ContentSource, DirectorySource},
    interface::{Data, data, host},
    loader,
    model::Logic,
    package::Snapshot,
    save::{SaveStore, SlotId},
    settings::Settings,
};
use std::path::Path;

fn demo() -> Result<(
    kakera_avg::loader::LoadedStory,
    kakera_avg::script::ScriptLogic,
)> {
    let source = DirectorySource::new(&Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo"))?;
    loader::load_game(&source)
}

#[test]
fn slot_metadata_previews_never_expose_frame_ids() -> Result<()> {
    let (loaded, logic) = demo()?;
    let story = &loaded.story;
    let definition = &loaded.interface;
    let directory = tempfile::tempdir()?;
    let saves = SaveStore::at(directory.path().to_owned());
    let session = story.start();
    saves.save_with(SlotId::new(1)?, story, &session, &logic)?;
    saves.autosave_with(story, &session, &logic)?;
    let slots = saves.list_page(story, 1)?;
    let (autosaves, _) = saves.auto_metadata(story);
    let state = host::State::new(definition);
    let profile = host::Profile::new(&story.id, definition);
    let available = logic.options(story, &session).map_err(anyhow::Error::msg)?;
    let data = Data::build(data::Input {
        definition,
        story,
        session: &session,
        available: &available,
        state: &state,
        profile: &profile,
        slots: &slots,
        settings: &Settings::default(),
        complete: true,
        slot_pages: &[],
        autosaves: &[],
    });
    let text = &story.frames["opening"].text;
    let slot = data.lists["slots"].first().expect("slot 1");
    assert_eq!(slot["preview"], serde_json::json!(text));
    assert_eq!(slot["excerpt"], serde_json::json!(text));
    let auto = autosaves.first().expect("autosave");
    assert_eq!(auto["preview"], serde_json::json!(text));
    for value in [&slot["preview"], &slot["excerpt"], &auto["preview"]] {
        let text = value.as_str().expect("text preview");
        assert!(
            !text.contains("opening") && !text.contains("frame"),
            "slot previews must not leak internal frame ids: {text}"
        );
    }
    // The save file itself keeps the frame id: only the displayed metadata changed.
    let raw: serde_json::Value =
        serde_json::from_slice(&std::fs::read(saves.slot_path(SlotId::new(1)?))?)?;
    assert_eq!(raw["frame_id"], serde_json::json!("opening"));
    Ok(())
}

#[test]
fn missing_old_settings_are_rejected_and_skip_setting_validates_through_plans() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("settings.json");
    // Only complete current settings are accepted; there is no missing-field upgrade path.
    std::fs::write(
        &path,
        r#"{"autosave":false,"auto_delay":2.0,"video_volume":0.5,"text_speed":12.0,"muted":true,"bgm_volume":0.2,"voice_volume":0.6,"se_volume":0.4}"#,
    )?;
    assert!(Settings::load(&path).is_err());
    let settings = Settings {
        autosave: false,
        muted: true,
        text_speed: 12.0,
        ..Settings::default()
    };
    settings.save(&path)?;
    assert_eq!(Settings::load(&path)?, settings);

    let (loaded, _) = demo()?;
    let definition = &loaded.interface;
    let state = host::State::new(definition);
    let profile = host::Profile::new(&loaded.story.id, definition);
    let prepare = |value: serde_json::Value| {
        host::prepare(
            definition,
            &state,
            &settings,
            &profile,
            vec![host::Command::Setting {
                name: "skip_all".into(),
                value,
            }],
            false,
        )
    };
    let plan = prepare(serde_json::json!(true))?;
    assert!(plan.settings.skip_all);
    assert!(matches!(plan.operation, host::Operation::Settings));
    assert!(
        prepare(serde_json::json!(1)).is_err(),
        "bool setting rejects numbers"
    );
    Ok(())
}

#[test]
fn read_progress_and_settings_are_never_packaged_as_game_content() -> Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join("story.xml"), "<story schema=\"8\"/>")?;
    std::fs::write(root.path().join("read.json"), "{}")?;
    std::fs::write(root.path().join("settings.json"), "{}")?;
    let snapshot = Snapshot::directory(root.path())?;
    assert!(snapshot.read(Path::new("story.xml")).is_ok());
    assert!(snapshot.read(Path::new("read.json")).is_err());
    assert!(snapshot.read(Path::new("settings.json")).is_err());
    Ok(())
}
