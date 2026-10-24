use kakera_avg::{
    controller::{Intent, reduce},
    loader::parse_story,
    model::{HistoryEntry, MAX_HISTORY, Session},
    save::{SaveStore, SlotId, SlotState, Snapshot},
    settings::Settings,
};
use std::path::Path;

fn story() -> kakera_avg::model::Story {
    parse_story(
        include_str!("../doc/rust-mvc-example/story.xml"),
        include_str!("../doc/rust-mvc-example/flow.xml"),
        Path::new("demo"),
    )
    .unwrap()
    .story
}
fn slot(number: u16) -> SlotId {
    SlotId::new(number).unwrap()
}

#[test]
fn reopening_store_restores_question_with_history_and_working_choices() {
    let story = story();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("story");
    let question = reduce(&story, &story.start(), Intent::Advance).unwrap();
    SaveStore::at(path.clone())
        .save(slot(1), &story, &question)
        .unwrap();
    let restored = SaveStore::at(path.clone()).load(slot(1), &story).unwrap();
    assert_eq!(restored, question);
    assert_eq!(restored.history.len(), 2);
    assert_eq!(story.frame(&restored), story.frame(&question));
    let end = reduce(&story, &restored, Intent::Choose("refuse".into())).unwrap();
    SaveStore::at(path.clone())
        .save(slot(1), &story, &end)
        .unwrap();
    assert_eq!(SaveStore::at(path).load(slot(1), &story).unwrap(), end);
    assert_eq!(end.history[1].choice_id.as_deref(), Some("refuse"));
}

#[test]
fn two_hundred_slots_are_isolated_and_invalid_slot_numbers_are_rejected() {
    let story = story();
    let directory = tempfile::tempdir().unwrap();
    let store = SaveStore::at(directory.path().into());
    assert!(SlotId::new(0).is_err());
    assert!(SlotId::new(201).is_err());
    assert!(
        store
            .list(&story)
            .iter()
            .all(|info| matches!(info.state, SlotState::Empty))
    );
    let question = reduce(&story, &story.start(), Intent::Advance).unwrap();
    let accept = reduce(&story, &question, Intent::Choose("accept".into())).unwrap();
    let refuse = reduce(&story, &question, Intent::Choose("refuse".into())).unwrap();
    for id in SlotId::all() {
        store
            .save(
                id,
                &story,
                if id.number() % 2 == 0 {
                    &accept
                } else {
                    &refuse
                },
            )
            .unwrap();
    }
    store.save(slot(2), &story, &question).unwrap();
    assert_eq!(store.load(slot(1), &story).unwrap(), refuse);
    assert_eq!(store.load(slot(2), &story).unwrap(), question);
    assert_eq!(store.load(slot(6), &story).unwrap(), accept);
    assert_eq!(store.list(&story).len(), 200);
    assert!(
        matches!(&store.list(&story)[1].state, SlotState::Ready { frame_id, preview, saved_at_unix_ms } if frame_id=="question" && preview=="要一起去看看吗？" && *saved_at_unix_ms>0)
    );
}

#[test]
fn malformed_identity_or_history_is_rejected_without_affecting_other_slots() {
    let story = story();
    let directory = tempfile::tempdir().unwrap();
    let store = SaveStore::at(directory.path().into());
    let question = reduce(&story, &story.start(), Intent::Advance).unwrap();
    store.save(slot(1), &story, &question).unwrap();
    let base = serde_json::to_value(Snapshot::capture(&story, &question).unwrap()).unwrap();
    let mut invalids = vec![serde_json::json!({"schema_version":2})];
    for (field, value) in [
        ("schema_version", serde_json::json!(1)),
        ("story_id", serde_json::json!("other")),
        ("content_version", serde_json::json!(9)),
        ("frame_id", serde_json::json!("missing")),
        ("history", serde_json::json!([])),
        (
            "history",
            serde_json::json!([{"frame_id":"opening","choice_id":null}]),
        ),
        (
            "history",
            serde_json::json!([{"frame_id":"missing","choice_id":null},{"frame_id":"question","choice_id":null}]),
        ),
        (
            "history",
            serde_json::json!([{"frame_id":"accept","choice_id":null},{"frame_id":"question","choice_id":null}]),
        ),
        (
            "history",
            serde_json::json!([{"frame_id":"question","choice_id":"accept"}]),
        ),
        (
            "history",
            serde_json::json!([{"frame_id":"opening","choice_id":"invented"},{"frame_id":"question","choice_id":null}]),
        ),
        ("unknown", serde_json::json!(true)),
    ] {
        let mut invalid = base.clone();
        invalid[field] = value;
        invalids.push(invalid);
    }
    for invalid in invalids {
        std::fs::write(
            store.slot_path(slot(2)),
            serde_json::to_vec(&invalid).unwrap(),
        )
        .unwrap();
        assert!(store.load(slot(2), &story).is_err(), "{invalid}");
        assert!(matches!(store.list(&story)[1].state, SlotState::Invalid(_)));
        assert_eq!(store.load(slot(1), &story).unwrap(), question);
    }
    std::fs::write(store.slot_path(slot(2)), b"broken JSON").unwrap();
    assert!(store.load(slot(2), &story).is_err());
    std::fs::write(store.slot_path(slot(2)), vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert!(store.load(slot(2), &story).is_err());
}

#[test]
fn invalid_session_cannot_overwrite_old_save() {
    let story = story();
    let directory = tempfile::tempdir().unwrap();
    let store = SaveStore::at(directory.path().into());
    store.save(slot(1), &story, &story.start()).unwrap();
    let old = std::fs::read(store.slot_path(slot(1))).unwrap();
    for invalid in [
        Session::at("missing"),
        Session {
            frame_id: "opening".into(),
            history: vec![],
            ..Session::at("opening")
        },
        Session {
            frame_id: "opening".into(),
            history: vec![
                HistoryEntry {
                    frame_id: "opening".into(),
                    choice_id: None
                };
                MAX_HISTORY + 1
            ],
            ..Session::at("opening")
        },
    ] {
        assert!(store.save(slot(1), &story, &invalid).is_err());
        assert_eq!(std::fs::read(store.slot_path(slot(1))).unwrap(), old);
    }
}

#[test]
fn legacy_import_is_explicit_and_does_not_rewrite_original_file() {
    let story = story();
    let directory = tempfile::tempdir().unwrap();
    let store = SaveStore::at(directory.path().into());
    let original = include_bytes!("../doc/rust-mvc-example/save.example.json");
    std::fs::write(store.legacy_path(), original).unwrap();
    let imported = store.import_legacy(&story).unwrap();
    assert_eq!(imported, Session::at("question"));
    assert!(
        store
            .list(&story)
            .iter()
            .all(|info| matches!(info.state, SlotState::Empty))
    );
    store.save(slot(3), &story, &imported).unwrap();
    assert_eq!(store.load(slot(3), &story).unwrap(), imported);
    assert_eq!(std::fs::read(store.legacy_path()).unwrap(), original);
    std::fs::write(
        store.legacy_path(),
        String::from_utf8_lossy(original).replace("question", "missing"),
    )
    .unwrap();
    assert!(store.import_legacy(&story).is_err());
}

#[test]
fn directory_failure_preserves_existing_data() {
    let story = story();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("blocked");
    std::fs::write(&path, b"existing data").unwrap();
    assert!(
        SaveStore::at(path.clone())
            .save(slot(1), &story, &story.start())
            .is_err()
    );
    assert_eq!(std::fs::read(path).unwrap(), b"existing data");
}

#[test]
fn settings_roundtrip_validates_ranges_and_keeps_previous_file_on_bad_values() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let settings = Settings {
        muted: true,
        text_speed: 0.0,
        ..Settings::default()
    };
    settings.save(&path).unwrap();
    assert_eq!(Settings::load(&path).unwrap(), settings);
    for bad in [
        Settings {
            text_speed: f32::NAN,
            ..settings.clone()
        },
        Settings {
            bgm_volume: 1.1,
            ..settings.clone()
        },
        Settings {
            text_speed: -1.0,
            ..settings.clone()
        },
    ] {
        assert!(bad.save(&path).is_err());
        assert_eq!(Settings::load(&path).unwrap(), settings);
    }
    std::fs::write(&path, "{}").unwrap();
    assert!(Settings::load(&path).is_err());
}

#[cfg(windows)]
#[test]
fn failed_windows_replacement_preserves_old_save_and_cleans_tempfile() {
    use std::os::windows::fs::OpenOptionsExt;
    let story = story();
    let directory = tempfile::tempdir().unwrap();
    let store = SaveStore::at(directory.path().into());
    store.save(slot(1), &story, &story.start()).unwrap();
    let old = std::fs::read(store.slot_path(slot(1))).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(store.slot_path(slot(1)))
        .unwrap();
    assert!(
        store
            .save(slot(1), &story, &Session::at("question"))
            .is_err()
    );
    drop(lock);
    assert_eq!(std::fs::read(store.slot_path(slot(1))).unwrap(), old);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
