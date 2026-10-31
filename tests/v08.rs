mod common;
use kakera_avg::{
    interface::{
        self, Data, Event, data,
        host::{self, Command, Operation, Profile, State},
    },
    loader,
    model::Logic,
    save::{SaveStore, SlotId, Snapshot},
    settings::Settings,
};
use serde_json::json;

#[test]
fn fullscreen_is_an_xml_bool_binding_with_js_and_atomic_setting_validation() -> anyhow::Result<()> {
    let (loaded, logic) = loader::load_game(&common::demo())?;
    let def = &loaded.interface;
    let state = State::new(def);
    let profile = Profile::new(&loaded.story.id, def);
    let settings = Settings::default();
    let session = loaded.story.start();
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
        complete: true,
        slot_pages: &[],
        autosaves: &[],
    });
    assert_eq!(data.values["settings"]["fullscreen"], false);
    let mut fullscreen = Event::ui("fullscreen", "fullscreen");
    fullscreen.value = json!(true);
    let commands = logic.ui_event(&fullscreen, &session, &state.variables, &data.values)?;
    let plan = host::prepare(def, &state, &settings, &profile, commands, false)?;
    assert!(plan.settings.fullscreen);
    assert_eq!(plan.operation, Operation::Settings);
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("settings.json");
    plan.settings.save(&path)?;
    assert!(Settings::load(&path)?.fullscreen);
    assert!(
        host::prepare(
            def,
            &state,
            &settings,
            &profile,
            vec![
                Command::Setting {
                    name: "fullscreen".into(),
                    value: json!(true)
                },
                Command::Setting {
                    name: "fullscreen".into(),
                    value: json!(1)
                }
            ],
            false
        )
        .is_err()
    );
    assert!(!settings.fullscreen);
    for schema in [1, 2] {
        let xml = format!(
            "<ui schema=\"{schema}\" initial=\"hud\"><screen id=\"hud\" modal=\"false\"/></ui>"
        );
        assert!(interface::parse(&xml, "ui.xml", &loaded.story, &[]).is_err());
    }
    Ok(())
}

#[test]
fn only_current_complete_save_and_settings_shapes_are_accepted() -> anyhow::Result<()> {
    let (loaded, logic) = common::game();
    let story = &loaded.story;
    let directory = tempfile::tempdir()?;
    let store = SaveStore::at(directory.path().into());
    let slot = SlotId::new(1)?;
    let snapshot = Snapshot::capture_with(story, &story.start(), &logic)?;
    let value = serde_json::to_value(&snapshot)?;
    assert!(value["screenshot"].is_null());
    assert!(value.as_object().unwrap().contains_key("screenshot"));
    for schema in 1..8 {
        let mut bad = value.clone();
        bad["schema_version"] = json!(schema);
        std::fs::write(store.slot_path(slot), serde_json::to_vec(&bad)?)?;
        assert!(store.load_with(slot, story, &logic).is_err());
    }
    let mut missing = value.clone();
    missing.as_object_mut().unwrap().remove("screenshot");
    assert!(serde_json::from_value::<Snapshot>(missing).is_err());
    store.save_with(slot, story, &story.start(), &logic)?;
    assert_eq!(store.load_with(slot, story, &logic)?, story.start());
    let mut settings = serde_json::to_value(Settings::default())?;
    for field in ["skip_all", "fullscreen"] {
        let old = settings.as_object_mut().unwrap().remove(field).unwrap();
        assert!(serde_json::from_value::<Settings>(settings.clone()).is_err());
        settings[field] = old;
    }
    // The old bridge command does not exist in the current host command grammar.
    assert!(serde_json::from_value::<Command>(json!({"type":"import_legacy"})).is_err());
    Ok(())
}
