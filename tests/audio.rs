use kakera_avg::{
    audio::{AudioCommand, AudioEngine, AudioPlan, Channel, PreparedAudio},
    loader::{load_story, parse_story},
    model::AudioCue,
};
use rodio::Source;
use std::path::Path;

#[test]
fn same_bgm_continues_while_one_shots_are_replaced_on_entry() {
    let mut plan = AudioPlan::default();
    let cue = AudioCue {
        bgm: Some("music".into()),
        voice: Some("voice".into()),
        se: Some("knock".into()),
    };
    let first = plan.enter(&cue, false);
    assert!(first.contains(&AudioCommand::Play {
        channel: Channel::Bgm,
        id: "music".into(),
        looping: true
    }));
    let next = plan.enter(&cue, false);
    assert!(!next.iter().any(|command| matches!(
        command,
        AudioCommand::Stop(Channel::Bgm)
            | AudioCommand::Play {
                channel: Channel::Bgm,
                ..
            }
    )));
    assert_eq!(
        next.iter()
            .filter(|command| matches!(command, AudioCommand::Play { .. }))
            .count(),
        2
    );
    let restored = plan.enter(&cue, true);
    assert_eq!(restored, first);
    let silence = plan.enter(&AudioCue::default(), false);
    assert_eq!(
        silence,
        vec![
            AudioCommand::Stop(Channel::Bgm),
            AudioCommand::Stop(Channel::Voice),
            AudioCommand::Stop(Channel::Se)
        ]
    );
    let changed = plan.enter(
        &AudioCue {
            bgm: Some("other".into()),
            ..cue
        },
        false,
    );
    assert!(changed.contains(&AudioCommand::Play {
        channel: Channel::Bgm,
        id: "other".into(),
        looping: true
    }));
}

#[test]
fn demo_audio_decodes_without_device_and_disabled_output_is_supported() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo");
    let story = load_story(&root).unwrap().story;
    let prepared = PreparedAudio::load(&story).unwrap();
    assert_eq!(prepared.len(), 3);
    let clip = prepared.clip("bgm.room").unwrap();
    assert_eq!(clip.channels(), 1);
    assert_eq!(clip.sample_rate(), 22_050);
    assert_eq!(clip.total_duration().unwrap().as_secs(), 4);
    assert!(clip.clone().any(|sample| sample.abs() > 0.01));
    let (mut engine, warning) = AudioEngine::new(prepared, true);
    assert!(warning.is_none());
    engine.enter(&story.frames["opening"].audio, true);
    assert!(engine.status().contains("已禁用"));
    assert!(engine.poll_error().is_none());
}

#[test]
fn audio_contract_is_strict_and_original_schema_remains_supported() {
    let story = include_str!("../games/demo/story.xml");
    let flow = include_str!("../games/demo/flow.xml");
    let loaded = parse_story(story, flow, Path::new("demo")).unwrap();
    assert_eq!(
        loaded.story.frames["opening"].audio.bgm.as_deref(),
        Some("bgm.room")
    );
    assert_eq!(loaded.story.frames["refuse"].audio, AudioCue::default());
    for (from, to) in [
        ("schema=\"2\"", "schema=\"1\""),
        ("se=\"se.knock\"", "se=\"bg.room\""),
        ("bgm=\"bgm.room\"", "bgm=\"missing\""),
        ("voice=\"voice.lin\"", "unknown=\"voice.lin\""),
        (
            "<audio bgm=\"bgm.room\" se=\"se.knock\" />",
            "<audio/><audio/>",
        ),
        (
            "src=\"assets/audio/room.wav\"",
            "src=\"assets/audio/room.mp3\"",
        ),
    ] {
        assert!(story.contains(from));
        let error = parse_story(&story.replace(from, to), flow, Path::new("demo"))
            .err()
            .unwrap();
        assert!(error.to_string().starts_with("story.xml:"), "{error:#}");
    }
    parse_story(
        include_str!("../doc/rust-mvc-example/story.xml"),
        flow,
        Path::new("demo"),
    )
    .unwrap();
}

#[test]
fn corrupt_or_empty_audio_fails_with_resource_identity() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo");
    let mut story = load_story(&root).unwrap().story;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bad.wav");
    story.assets.get_mut("bgm.room").unwrap().path = path.clone();
    for content in [b"not audio".as_slice(), b"".as_slice()] {
        std::fs::write(&path, content).unwrap();
        let error = PreparedAudio::load(&story).err().unwrap();
        assert!(format!("{error:#}").contains("bgm.room"));
        assert!(format!("{error:#}").contains("bad.wav"));
    }
}

#[test]
fn ogg_vorbis_fixture_decodes_without_output_device() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut story = load_story(&root.join("games/demo")).unwrap().story;
    story.assets.get_mut("voice.lin").unwrap().path = root.join("tests/fixtures/cue.ogg");
    let prepared = PreparedAudio::load(&story).unwrap();
    let clip = prepared.clip("voice.lin").unwrap();
    assert!(clip.total_duration().unwrap().as_secs_f32() > 0.5);
    assert!(clip.clone().any(|sample| sample.abs() > 0.01));
}
