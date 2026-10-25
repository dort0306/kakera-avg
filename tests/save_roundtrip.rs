mod common;
use kakera_avg::{
    controller::{Intent, transition_with},
    model::{HistoryEntry, MAX_HISTORY, Session},
    save::{SaveStore, SlotId, SlotState, Snapshot},
    settings::Settings,
};
fn slot(number: u16) -> SlotId {
    SlotId::new(number).unwrap()
}
#[test]
fn reopening_store_restores_question_with_history_and_working_choices() {
    let (loaded, logic) = common::game();
    let story = loaded.story;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("story");
    let question = transition_with(&story, &story.start(), Intent::Advance, &logic)
        .unwrap()
        .session;
    SaveStore::at(path.clone())
        .save_with(slot(1), &story, &question, &logic)
        .unwrap();
    let restored = SaveStore::at(path.clone())
        .load_with(slot(1), &story, &logic)
        .unwrap();
    assert_eq!(restored, question);
    assert_eq!(restored.history.len(), 2);
    assert_eq!(story.frame(&restored), story.frame(&question));
    let end = transition_with(&story, &restored, Intent::Choose("refuse".into()), &logic)
        .unwrap()
        .session;
    SaveStore::at(path.clone())
        .save_with(slot(1), &story, &end, &logic)
        .unwrap();
    assert_eq!(
        SaveStore::at(path)
            .load_with(slot(1), &story, &logic)
            .unwrap(),
        end
    );
    assert_eq!(end.history[1].choice_id.as_deref(), Some("refuse"));
}

#[test]
fn two_hundred_slots_are_isolated_and_invalid_slot_numbers_are_rejected() {
    let (loaded, logic) = common::game();
    let story = loaded.story;
    let directory = tempfile::tempdir().unwrap();
    let store = SaveStore::at(directory.path().into());
    assert!(SlotId::new(0).is_err());
    assert!(SlotId::new(201).is_err());
    assert!(
        store
            .list_page(&story, 1)
            .unwrap()
            .iter()
            .all(|info| matches!(info.state, SlotState::Empty))
    );
    let question = transition_with(&story, &story.start(), Intent::Advance, &logic)
        .unwrap()
        .session;
    let accept = transition_with(&story, &question, Intent::Choose("accept".into()), &logic)
        .unwrap()
        .session;
    let refuse = transition_with(&story, &question, Intent::Choose("refuse".into()), &logic)
        .unwrap()
        .session;
    for id in SlotId::all() {
        store
            .save_with(
                id,
                &story,
                if id.number() % 2 == 0 {
                    &accept
                } else {
                    &refuse
                },
                &logic,
            )
            .unwrap();
    }
    store.save_with(slot(2), &story, &question, &logic).unwrap();
    assert_eq!(store.load_with(slot(1), &story, &logic).unwrap(), refuse);
    assert_eq!(store.load_with(slot(2), &story, &logic).unwrap(), question);
    assert_eq!(store.load_with(slot(6), &story, &logic).unwrap(), accept);
    assert_eq!(
        (1..=20)
            .flat_map(|page| store.list_page(&story, page).unwrap())
            .count(),
        200
    );
    assert!(
        matches!(&store.list_page(&story, 1).unwrap()[1].state, SlotState::Ready { frame_id, preview, saved_at_unix_ms, .. } if frame_id=="question" && preview=="要一起去看看吗？" && *saved_at_unix_ms>0)
    );
}

#[test]
fn malformed_identity_or_history_is_rejected_without_affecting_other_slots() {
    let (loaded, logic) = common::game();
    let story = loaded.story;
    let directory = tempfile::tempdir().unwrap();
    let store = SaveStore::at(directory.path().into());
    let question = transition_with(&story, &story.start(), Intent::Advance, &logic)
        .unwrap()
        .session;
    store.save_with(slot(1), &story, &question, &logic).unwrap();
    let base =
        serde_json::to_value(Snapshot::capture_with(&story, &question, &logic).unwrap()).unwrap();
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
        assert!(
            store.load_with(slot(2), &story, &logic).is_err(),
            "{invalid}"
        );
        // Page scanning validates metadata only; history is replayed on actual load.
        let state = &store.list_page(&story, 1).unwrap()[1].state;
        if invalid
            .get("history")
            .is_some_and(|history| history != &base["history"])
        {
            assert!(matches!(state, SlotState::Ready { .. }), "{invalid}");
        } else {
            assert!(matches!(state, SlotState::Invalid(_)), "{invalid}");
        }
        assert_eq!(store.load_with(slot(1), &story, &logic).unwrap(), question);
    }
    std::fs::write(store.slot_path(slot(2)), b"broken JSON").unwrap();
    assert!(store.load_with(slot(2), &story, &logic).is_err());
    std::fs::write(store.slot_path(slot(2)), vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert!(store.load_with(slot(2), &story, &logic).is_err());
}

#[test]
fn invalid_session_cannot_overwrite_old_save() {
    let (loaded, logic) = common::game();
    let story = loaded.story;
    let directory = tempfile::tempdir().unwrap();
    let store = SaveStore::at(directory.path().into());
    store
        .save_with(slot(1), &story, &story.start(), &logic)
        .unwrap();
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
        assert!(store.save_with(slot(1), &story, &invalid, &logic).is_err());
        assert_eq!(std::fs::read(store.slot_path(slot(1))).unwrap(), old);
    }
}

#[test]
fn directory_failure_preserves_existing_data() {
    let (loaded, logic) = common::game();
    let story = loaded.story;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("blocked");
    std::fs::write(&path, b"existing data").unwrap();
    assert!(
        SaveStore::at(path.clone())
            .save_with(slot(1), &story, &story.start(), &logic)
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
    let (loaded, logic) = common::game();
    let story = loaded.story;
    let directory = tempfile::tempdir().unwrap();
    let store = SaveStore::at(directory.path().into());
    store
        .save_with(slot(1), &story, &story.start(), &logic)
        .unwrap();
    let old = std::fs::read(store.slot_path(slot(1))).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(store.slot_path(slot(1)))
        .unwrap();
    assert!(
        store
            .save_with(slot(1), &story, &Session::at("question"), &logic)
            .is_err()
    );
    drop(lock);
    assert_eq!(std::fs::read(store.slot_path(slot(1))).unwrap(), old);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
