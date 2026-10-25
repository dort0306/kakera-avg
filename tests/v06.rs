use kakera_avg::{
    content::DirectorySource,
    controller::{Intent, transition_with},
    interface::{
        Event,
        host::{self, Command, Profile, State},
    },
    live2d::motion::Motion,
    loader::load_game,
    media::{PreparedMedia, VideoPlayer},
    presentation::AutoPlay,
    save::{SaveStore, SlotId, Snapshot},
    settings::Settings,
};
use serde_json::{Value, json};
use std::path::Path;
fn game() -> (
    kakera_avg::loader::LoadedStory,
    kakera_avg::script::ScriptLogic,
) {
    load_game(
        &DirectorySource::new(&Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo")).unwrap(),
    )
    .unwrap()
}
#[test]
fn pages_cover_exactly_two_hundred_slots_and_page_commands_are_atomic() {
    let (loaded, _) = game();
    let story = &loaded.story;
    let dir = tempfile::tempdir().unwrap();
    let saves = SaveStore::at(dir.path().into());
    let numbers: Vec<_> = (1..=20)
        .flat_map(|page| {
            saves
                .list_page(story, page)
                .unwrap()
                .into_iter()
                .map(|i| i.slot.number())
        })
        .collect();
    assert_eq!(numbers, (1..=200).collect::<Vec<_>>());
    assert!(saves.list_page(story, 0).is_err());
    assert!(saves.list_page(story, 21).is_err());
    assert!(SlotId::new(0).is_err());
    assert!(SlotId::new(201).is_err());
    let def = loaded.interface;
    let state = State::new(&def);
    let profile = Profile::new(&story.id, &def);
    let result = host::prepare(
        &def,
        &state,
        &Settings::default(),
        &profile,
        vec![Command::SlotsPage { page: 20 }],
        false,
    )
    .unwrap();
    assert_eq!(result.state.slot_page, 20);
    assert!(
        host::prepare(
            &def,
            &state,
            &Settings::default(),
            &profile,
            vec![
                Command::SlotsPage { page: 20 },
                Command::SlotsPage { page: 21 }
            ],
            false
        )
        .is_err()
    );
    assert_eq!(state.slot_page, 1);
}
#[test]
fn autosaves_rotate_independently_and_corruption_or_failed_writes_preserve_old_data() {
    let (loaded, logic) = game();
    let story = &loaded.story;
    let session = story.start();
    let dir = tempfile::tempdir().unwrap();
    let saves = SaveStore::at(dir.path().into());
    let manual = SlotId::new(200).unwrap();
    saves.save_with(manual, story, &session, &logic).unwrap();
    let old = std::fs::read(saves.slot_path(manual)).unwrap();
    for _ in 0..13 {
        saves.autosave_with(story, &session, &logic).unwrap();
    }
    let list = saves.auto_list(story);
    assert_eq!(list.len(), 10);
    assert_eq!(list[0]["generation"], 13);
    assert_eq!(list.last().unwrap()["generation"], 4);
    assert_eq!(saves.load_auto(3, story, &logic).unwrap(), session);
    assert_eq!(std::fs::read(saves.slot_path(manual)).unwrap(), old);
    std::fs::write(saves.auto_path(1).unwrap(), "broken").unwrap();
    assert!(saves.load_auto(1, story, &logic).is_err());
    assert!(saves.load_auto(2, story, &logic).is_ok());
    assert!(saves.load_auto(0, story, &logic).is_err());
    assert!(saves.load_auto(11, story, &logic).is_err());
    let mut bad = session.clone();
    bad.frame_id = "missing".into();
    let previous = std::fs::read(saves.auto_path(2).unwrap()).unwrap();
    assert!(saves.autosave_with(story, &bad, &logic).is_err());
    assert_eq!(
        std::fs::read(saves.auto_path(2).unwrap()).unwrap(),
        previous
    );
    let target = saves.auto_path(4).unwrap();
    std::fs::remove_file(&target).unwrap();
    std::fs::create_dir(&target).unwrap();
    assert!(saves.autosave_with(story, &session, &logic).is_err());
    assert!(target.is_dir());
    assert_eq!(
        std::fs::read(saves.auto_path(2).unwrap()).unwrap(),
        previous
    );
}
#[test]
fn current_snapshot_rejects_old_schema_and_keeps_variables_history_and_manual_slot_200() {
    let (documented, documented_logic) = game();
    let snapshot: Snapshot =
        serde_json::from_str(include_str!("../doc/v0.8-save.example.json")).unwrap();
    assert_eq!(
        snapshot
            .restore_with(&documented.story, &documented_logic)
            .unwrap(),
        documented.story.start()
    );

    let (loaded, logic) = game();
    let story = &loaded.story;
    let q = transition_with(story, &story.start(), Intent::Advance, &logic)
        .unwrap()
        .session;
    let w = transition_with(story, &q, Intent::Choose("accept".into()), &logic)
        .unwrap()
        .session;
    let dir = tempfile::tempdir().unwrap();
    let saves = SaveStore::at(dir.path().into());
    let slot = SlotId::new(200).unwrap();
    saves.save_with(slot, story, &w, &logic).unwrap();
    assert_eq!(saves.load_with(slot, story, &logic).unwrap(), w);
    let mut snapshot = Snapshot::capture_with(story, &w, &logic).unwrap();
    assert_eq!(snapshot.schema_version, 8);
    snapshot.schema_version = 3;
    assert!(snapshot.restore_with(story, &logic).is_err());
}
#[test]
fn autoplay_waits_for_readiness_and_delay_pauses_without_accumulating_and_never_chooses() {
    let mut auto = AutoPlay::default();
    auto.set(true);
    assert!(!auto.tick(9.0, false, true, 1.0));
    assert!(!auto.tick(0.4, true, true, 1.0));
    assert!(!auto.tick(0.0, true, true, 1.0));
    assert!(!auto.tick(0.5, true, true, 1.0));
    assert!(auto.tick(0.2, true, true, 1.0));
    auto.enter();
    assert!(!auto.tick(0.9, true, true, 1.0));
    auto.set(false);
    assert!(!auto.tick(10.0, true, true, 1.0));
    auto.set(true);
    assert!(!auto.tick(10.0, true, false, 1.0));
    assert!(!auto.enabled);
}
#[test]
fn new_js_commands_are_bounded_and_game_enter_cannot_navigate_or_start_autoplay() {
    let (loaded, logic) = game();
    let def = loaded.interface;
    let session = loaded.story.start();
    let state = State::new(&def);
    let settings = Settings::default();
    let profile = Profile::new(&loaded.story.id, &def);
    let commands = logic
        .ui_event(
            &Event {
                name: "slots-last".into(),
                item: Value::Null,
                value: Value::Null,
            },
            &session,
            &state.variables,
            &json!({"game":{},"saves":{"page":1}}),
        )
        .unwrap();
    assert_eq!(
        host::prepare(&def, &state, &settings, &profile, commands, false)
            .unwrap()
            .state
            .slot_page,
        20
    );
    for cmd in [
        Command::LoadAuto { slot: 0 },
        Command::SlotsPage { page: 21 },
        Command::Save {
            slot: 201,
            overwrite: false,
        },
    ] {
        assert!(host::prepare(&def, &state, &settings, &profile, vec![cmd], false).is_err());
    }
    assert!(
        host::prepare(
            &def,
            &state,
            &settings,
            &profile,
            vec![Command::AutoPlay { enabled: true }],
            true
        )
        .is_err()
    );
}
#[test]
fn motion_linear_bezier_and_steps_have_correct_endpoints_and_reject_invalid_data() {
    for segments in [
        json!([0, 0, 0, 1, 10]),
        json!([0, 0, 1, 0.3, 0, 0.7, 10, 1, 10]),
        json!([0, 0, 2, 1, 10]),
        json!([0, 0, 3, 1, 10]),
    ] {
        let v = json!({"Version":3,"Meta":{"Duration":1,"Loop":false},"Curves":[{"Target":"Parameter","Id":"angle","Segments":segments}]});
        let m = Motion::parse(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert!((m.curves[0].value(0.0)).abs() < 0.001);
        assert!((m.curves[0].value(1.0) - 10.0).abs() < 0.001);
    }
    let v = json!({"Version":3,"Meta":{"Duration":1},"Curves":[{"Target":"Parameter","Id":"angle","Segments":[0,0,5,1,10]}]});
    assert!(Motion::parse(&serde_json::to_vec(&v).unwrap()).is_err());
}

#[test]
fn media_xml_rejects_unknown_attributes_wrong_types_and_unavailable_routes_with_positions() {
    let source =
        DirectorySource::new(&Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo")).unwrap();
    let story = kakera_avg::story_chain::compile(&source).unwrap().xml;
    for (from, to) in [
        ("asset=\"video.intro\"", "asset=\"bg.room\""),
        ("skippable=\"true\"", "skippable=\"yes\""),
        ("advance=\"true\"", "advance=\"true\" loop=\"true\""),
        ("asset=\"video.intro\"", "asset=\"missing\""),
    ] {
        struct Xml(String);
        impl kakera_avg::content::ContentSource for Xml {
            fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
                if path == Path::new("story.xml") {
                    Ok(self.0.as_bytes().to_vec())
                } else {
                    anyhow::bail!("unexpected resource request")
                }
            }
        }
        let error = kakera_avg::loader::load_game(&Xml(story.replace(from, to)))
            .err()
            .unwrap();
        assert!(format!("{error:#}").contains("story.xml:"));
    }
}
#[cfg(windows)]
#[test]
fn real_mp4_decodes_video_and_embedded_audio_without_output_device_and_preserves_end_event() {
    let (loaded, _) = game();
    let source =
        DirectorySource::new(&Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo")).unwrap();
    let media = PreparedMedia::load(&loaded.story, &source).unwrap();
    let clip = &media.videos["video.intro"];
    assert_eq!(clip.size, [1280, 720]);
    assert!((11.9..12.2).contains(&clip.duration));
    assert!(clip.audio.is_some());
    assert!(clip.first.pixels.iter().any(|p| p.r() != p.b()));
    let mut video =
        VideoPlayer::prepare(loaded.story.frames["intro"].video.as_ref(), &media).unwrap();
    let ctx = eframe::egui::Context::default();
    let mut assets = kakera_avg::assets::PreparedAssets::load_from(&loaded.story, &source)
        .unwrap()
        .install(&ctx);
    assert!(!video.tick(0.0, &ctx, &mut assets).unwrap());
    assert_eq!(video.position, 0.0);
    assert!(video.tick(13.0, &ctx, &mut assets).unwrap());
    assert!(video.tick(0.0, &ctx, &mut assets).unwrap());
    assert!(video.pending_advance);
    video.pending_advance = false;
    assert!(!video.tick(0.0, &ctx, &mut assets).unwrap());
    let mut video =
        VideoPlayer::prepare(loaded.story.frames["credits"].video.as_ref(), &media).unwrap();
    assert!(video.skip());
    assert!(!video.playing());
}

#[test]
fn bundled_live2d_routes_and_tap_callback_preserve_variables_and_replayable_history() {
    let (loaded, logic) = game();
    let story = &loaded.story;
    let question = transition_with(story, &story.start(), Intent::Advance, &logic)
        .unwrap()
        .session;
    let stage = transition_with(
        story,
        &question,
        Intent::Choose("live2d-demo".into()),
        &logic,
    )
    .unwrap()
    .session;
    assert_eq!(stage.frame_id, "live2d.stage");
    for from in ["stage", "expression", "angle"] {
        let initial = if from == "stage" {
            stage.clone()
        } else {
            transition_with(story, &stage, Intent::Choose(from.into()), &logic)
                .unwrap()
                .session
        };
        for (option, target) in [
            ("idle", "live2d.stage"),
            ("tap", "live2d.stage"),
            ("expression", "live2d.expression"),
            ("angle", "live2d.angle"),
            ("return", "question"),
        ] {
            let candidate = transition_with(story, &initial, Intent::Choose(option.into()), &logic)
                .unwrap()
                .session;
            assert_eq!(candidate.frame_id, target);
            assert_eq!(candidate.variables, question.variables);
            assert_eq!(
                Snapshot::capture_with(story, &candidate, &logic)
                    .unwrap()
                    .restore_with(story, &logic)
                    .unwrap(),
                candidate
            );
        }
    }
    let definition = &loaded.interface;
    let state = State::new(definition);
    let commands = logic
        .ui_event(
            &Event {
                name: "choose".into(),
                item: json!({"id":"tap"}),
                value: Value::Null,
            },
            &stage,
            &state.variables,
            &json!({"game":{"frameId":"live2d.stage"}}),
        )
        .unwrap();
    let plan = host::prepare(
        definition,
        &state,
        &Settings::default(),
        &Profile::new(&story.id, definition),
        commands,
        false,
    )
    .unwrap();
    assert_eq!(
        plan.live2d,
        vec![(kakera_avg::model::Slot::Center, "TapBody:0".into())]
    );
    assert_eq!(plan.operation, host::Operation::Choose("tap".into()));
}
