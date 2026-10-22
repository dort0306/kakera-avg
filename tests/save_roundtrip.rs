use kakera_avg::{
    controller::{Intent, reduce},
    loader::parse_story,
    model::Session,
    save::{SaveStore, Snapshot},
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

#[test]
fn reopening_store_restores_question_with_working_choices() {
    let story = story();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("story/save.json");
    let question = reduce(&story, &story.start(), Intent::Advance).unwrap();
    SaveStore::at(path.clone()).save(&story, &question).unwrap();
    let restored = SaveStore::at(path.clone()).load(&story).unwrap();
    assert_eq!(restored, question);
    assert_eq!(story.frame(&restored), story.frame(&question));
    assert_eq!(
        reduce(&story, &restored, Intent::Choose("refuse".into()))
            .unwrap()
            .frame_id,
        "refuse"
    );
    // Existing-file replacement must work on Windows too.
    let end = Session {
        frame_id: "accept".into(),
    };
    SaveStore::at(path.clone()).save(&story, &end).unwrap();
    assert_eq!(SaveStore::at(path).load(&story).unwrap(), end);
}

#[test]
fn incompatible_and_corrupt_saves_do_not_replace_current_session() {
    let story = story();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("save.json");
    let store = SaveStore::at(path.clone());
    let mut session = story.start();
    let baseline = session.clone();
    for invalid in [
        "not json".to_owned(),
        r#"{"schema_version":1}"#.to_owned(),
        include_str!("../doc/rust-mvc-example/save.example.json")
            .replace("\"schema_version\": 1", "\"schema_version\": 9"),
        include_str!("../doc/rust-mvc-example/save.example.json").replace("kakera-demo", "other"),
        include_str!("../doc/rust-mvc-example/save.example.json")
            .replace("\"content_version\": 1", "\"content_version\": 2"),
        include_str!("../doc/rust-mvc-example/save.example.json").replace("question", "missing"),
        include_str!("../doc/rust-mvc-example/save.example.json")
            .replace("\"frame_id\"", "\"typo\""),
    ] {
        std::fs::write(&path, invalid).unwrap();
        let result = store.load(&story);
        assert!(result.is_err());
        if let Ok(loaded) = result {
            session = loaded;
        }
        assert_eq!(session, baseline);
    }
}

#[test]
fn invalid_session_cannot_overwrite_old_save() {
    let story = story();
    let directory = tempfile::tempdir().unwrap();
    let store = SaveStore::at(directory.path().join("save.json"));
    store.save(&story, &story.start()).unwrap();
    let old = std::fs::read(store.path()).unwrap();
    assert!(
        store
            .save(
                &story,
                &Session {
                    frame_id: "missing".into()
                }
            )
            .is_err()
    );
    assert_eq!(std::fs::read(store.path()).unwrap(), old);
    let snapshot: Snapshot =
        serde_json::from_str(include_str!("../doc/rust-mvc-example/save.example.json")).unwrap();
    assert_eq!(snapshot.restore(&story).unwrap().frame_id, "question");
}

#[test]
fn directory_failure_is_reported_and_preserves_existing_data() {
    let story = story();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("blocked");
    std::fs::write(&path, b"existing data").unwrap();
    assert!(
        SaveStore::at(path.join("save.json"))
            .save(&story, &story.start())
            .is_err()
    );
    assert_eq!(std::fs::read(path).unwrap(), b"existing data");
}

#[cfg(windows)]
#[test]
fn failed_windows_replacement_preserves_old_save_and_cleans_tempfile() {
    use std::os::windows::fs::OpenOptionsExt;
    let story = story();
    let directory = tempfile::tempdir().unwrap();
    let store = SaveStore::at(directory.path().join("save.json"));
    store.save(&story, &story.start()).unwrap();
    let old = std::fs::read(store.path()).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(store.path())
        .unwrap();
    assert!(
        store
            .save(
                &story,
                &Session {
                    frame_id: "question".into()
                }
            )
            .is_err()
    );
    drop(lock);
    assert_eq!(std::fs::read(store.path()).unwrap(), old);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
