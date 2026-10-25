mod common;
use kakera_avg::{
    audio::{AudioCommand, AudioEngine, AudioPlan, Channel, PreparedAudio},
    loader::load_game,
    model::AudioCue,
};
use kakera_avg::{
    content::ContentSource,
    model::{AssetKind, Story},
};
use rodio::Source;

fn clips(story: &Story) -> common::Memory {
    let source = common::demo();
    common::Memory(
        story
            .assets
            .values()
            .filter(|a| a.kind == AssetKind::Audio)
            .map(|a| (a.path.clone(), source.read(&a.path).unwrap()))
            .collect(),
    )
}

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
    let source = common::demo();
    let story = load_game(&source).unwrap().0.story;
    let prepared = PreparedAudio::load_from(&story, &source).unwrap();
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
fn corrupt_or_empty_audio_fails_with_resource_identity() {
    let mut story = load_game(&common::demo()).unwrap().0.story;
    let mut source = clips(&story);
    story.assets.get_mut("bgm.room").unwrap().path = "bad.wav".into();
    for content in [b"not audio".as_slice(), b"".as_slice()] {
        source.0.insert("bad.wav".into(), content.to_vec());
        let error = PreparedAudio::load_from(&story, &source).err().unwrap();
        assert!(format!("{error:#}").contains("bgm.room"));
        assert!(format!("{error:#}").contains("bad.wav"));
    }
}

#[test]
fn ogg_vorbis_fixture_decodes_without_output_device() {
    let mut story = load_game(&common::demo()).unwrap().0.story;
    let mut source = clips(&story);
    story.assets.get_mut("voice.lin").unwrap().path = "cue.ogg".into();
    source.0.insert(
        "cue.ogg".into(),
        include_bytes!("fixtures/cue.ogg").to_vec(),
    );
    let prepared = PreparedAudio::load_from(&story, &source).unwrap();
    let clip = prepared.clip("voice.lin").unwrap();
    assert!(clip.total_duration().unwrap().as_secs_f32() > 0.5);
    assert!(clip.clone().any(|sample| sample.abs() > 0.01));
}
