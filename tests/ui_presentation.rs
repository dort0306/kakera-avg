use anyhow::Result;
use kakera_avg::{
    content::{ContentSource, validate_path},
    interface::{
        self,
        host::{self, Command, Profile, State},
        presentation::{Action, Preferences},
    },
    loader,
    settings::Settings,
};
use std::{collections::BTreeMap, path::Path};
const STORY: &str = r#"<story schema="8" id="presentation" content-version="8" entry="start"><scripts><script src="story.js"/></scripts><interface src="ui.xml"/><assets><font id="font" src="font.otf"/><audio id="voice.a" src="a.wav"/><audio id="knock" src="b.wav"/><video id="intro" src="v.mp4"/></assets><characters><character id="a" name="甲"/></characters><visuals/><frames><frame id="start"><text>开始。</text></frame></frames><next end="true"/></story>"#;
const UI: &str = r#"<ui schema="3" initial="hud"><include src="ui/media.xml"/><screen id="hud" modal="false" pause="false"><scene id="scene" width="1280" height="720"/><button id="single" width="100" height="40" sound="voice.a" text="单音效"/><button id="random" width="100" height="40" sound="bank" text="随机"/><button id="run" width="100" height="40" text="运行" event="run"/><toggle id="role" width="100" height="40" value="audio.actor.a" event="role"/></screen><screen id="menu" sequence="opening"><label id="title" width="400" height="50" text="欢迎"/></screen></ui>"#;
const MEDIA: &str = r#"<ui schema="3"><ui-media><sequence id="opening" finish="open:hud"><fade target="title" duration="10"/><sound ref="bank"/><video asset="intro"/></sequence><sound id="bank"><clip asset="voice.a" actor="a" weight="2"/><clip asset="knock" weight="1"/></sound></ui-media></ui>"#;
const JS: &str = "frame('start').end();ui('role').bind('role', ctx=>ctx.soundActor('a',ctx.value));ui('run').bind('run', ctx=>{ctx.playSound('bank');ctx.sequence('opening');ctx.soundExclude('voice.a',true);});";
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
fn source(media: &str, ui: &str) -> Memory {
    Memory(
        [
            ("story.xml", STORY),
            ("ui.xml", ui),
            ("ui/media.xml", media),
            ("story.js", JS),
        ]
        .into_iter()
        .map(|(p, s)| (p.into(), s.as_bytes().to_vec()))
        .collect(),
    )
}
#[test]
fn weighted_selection_filters_first_and_never_falls_back_to_forbidden_sounds() -> Result<()> {
    let (loaded, _) = loader::load_game(&source(MEDIA, UI))?;
    let c = &loaded.interface.media;
    let mut prefs = Preferences::new("presentation");
    for (n, expected) in [(0, "voice.a"), (1, "voice.a"), (2, "knock")] {
        assert_eq!(
            c.choose_with("bank", &prefs, |limit| {
                assert_eq!(limit, 3);
                Ok(n)
            })?
            .as_deref(),
            Some(expected)
        );
    }
    c.preference(&mut prefs, &Action::Actor("a".into(), false))?;
    assert_eq!(
        c.choose_with("bank", &prefs, |limit| {
            assert_eq!(limit, 1);
            Ok(0)
        })?
        .as_deref(),
        Some("knock")
    );
    assert_eq!(
        c.choose_with("voice.a", &prefs, |_| panic!(
            "direct selection needs no RNG"
        ))?,
        None
    );
    c.preference(&mut prefs, &Action::Exclude("knock".into(), true))?;
    assert_eq!(
        c.choose_with("bank", &prefs, |_| panic!("empty pool must not sample"))?,
        None
    );
    c.preference(&mut prefs, &Action::Actor("a".into(), true))?;
    c.preference(&mut prefs, &Action::Enabled(false))?;
    assert_eq!(
        c.choose_with("bank", &prefs, |_| panic!("disabled master"))?,
        None
    );
    assert!(
        c.preference(&mut prefs, &Action::Exclude("intro".into(), true))
            .is_err()
    );
    Ok(())
}
#[test]
fn author_disabled_sound_cannot_be_bypassed_by_resource_or_another_bank() -> Result<()> {
    let media = MEDIA
        .replace(
            "asset=\"voice.a\" actor=\"a\" weight=\"2\"",
            "asset=\"voice.a\" actor=\"a\" weight=\"2\" enabled=\"false\"",
        )
        .replace(
            "</ui-media>",
            "<sound id=\"other\"><clip asset=\"voice.a\" actor=\"a\"/></sound></ui-media>",
        );
    let (loaded, _) = loader::load_game(&source(&media, UI))?;
    let c = &loaded.interface.media;
    let p = Preferences::new("presentation");
    assert_eq!(c.choose_with("voice.a", &p, |_| panic!())?, None);
    assert_eq!(c.choose_with("other", &p, |_| panic!())?, None);
    Ok(())
}
#[test]
fn split_ui_validates_forward_references_locations_and_budgets() {
    for (before, after) in [
        ("weight=\"2\"", "weight=\"0\""),
        ("actor=\"a\"", "actor=\"unknown\""),
        ("target=\"title\"", "target=\"missing\""),
        ("duration=\"10\"", "duration=\"NaN\""),
        ("asset=\"intro\"", "asset=\"knock\""),
        ("ref=\"bank\"", "ref=\"missing\""),
        ("open:hud", "open:missing"),
        (
            "<clip asset=\"knock\" weight=\"1\"/>",
            "<clip asset=\"knock\" extra=\"x\"/>",
        ),
    ] {
        let err = loader::load_game(&source(&MEDIA.replace(before, after), UI))
            .err()
            .expect("reject invalid media");
        let text = format!("{err:#}");
        assert!(text.contains("ui/media.xml:"), "{text}");
    }
    for ui in [
        UI.replace("audio.actor.a", "audio.actor.missing"),
        UI.replace("sound=\"bank\"", "sound=\"missing\""),
        UI.replace("sequence=\"opening\"", "sequence=\"missing\""),
    ] {
        assert!(loader::load_game(&source(MEDIA, &ui)).is_err());
    }
    let too_many = MEDIA.replace(
        "<video asset=\"intro\"/>",
        &"<video asset=\"intro\"/>".repeat(64),
    );
    assert!(loader::load_game(&source(&too_many, UI)).is_err());
}
#[test]
fn js_commands_validate_atomically_and_lifecycle_cannot_start_media() -> Result<()> {
    let (loaded, logic) = loader::load_game(&source(MEDIA, UI))?;
    let d = &loaded.interface;
    let state = State::new(d);
    let profile = Profile::new("presentation", d);
    let settings = Settings::default();
    let event = interface::Event::ui("run", "run");
    let commands = logic.ui_event(
        &event,
        &loaded.story.start(),
        &state.variables,
        &serde_json::json!({}),
    )?;
    let plan = host::prepare(d, &state, &settings, &profile, commands, false)?;
    assert_eq!(plan.ui_actions.len(), 3);
    assert!(matches!(plan.operation, host::Operation::UiAudio));
    for commands in [
        vec![
            Command::Sequence {
                id: "opening".into(),
            },
            Command::Sequence {
                id: "opening".into(),
            },
        ],
        vec![Command::SoundActor {
            actor: "missing".into(),
            enabled: false,
        }],
        vec![Command::PlaySound { id: "intro".into() }],
    ] {
        assert!(host::prepare(d, &state, &settings, &profile, commands, false).is_err());
    }
    assert!(
        host::prepare(
            d,
            &state,
            &settings,
            &profile,
            vec![Command::Sequence {
                id: "opening".into()
            }],
            true
        )
        .is_err()
    );
    assert_eq!(state.stack, vec!["hud"]);
    Ok(())
}
#[test]
fn preferences_survive_reload_and_atomic_failure_preserves_previous_file() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("ui-audio.json");
    let mut p = Preferences::new("presentation");
    p.disabled_actors.insert("a".into());
    p.excluded.insert("voice.a".into());
    p.save(&path)?;
    let bytes = std::fs::read(&path)?;
    let saved = Preferences::load(&path, "presentation")?;
    assert!(saved.disabled_actors.contains("a"));
    assert!(saved.excluded.contains("voice.a"));
    assert!(Preferences::load(&path, "wrong").is_err());
    let invalid = dir.path().join("blocked");
    std::fs::create_dir(&invalid)?;
    std::fs::write(invalid.join("marker"), b"old")?;
    assert!(p.save(&invalid).is_err());
    assert_eq!(std::fs::read(&path)?, bytes);
    assert_eq!(std::fs::read(invalid.join("marker"))?, b"old");
    for bad in [
        "{}",
        r#"{"schema":1,"story_id":"presentation","enabled":true,"disabled_actors":[],"excluded":[],"extra":0}"#,
    ] {
        std::fs::write(&path, bad)?;
        assert!(Preferences::load(&path, "presentation").is_err());
    }
    Ok(())
}
