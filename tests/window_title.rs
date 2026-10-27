use anyhow::Result;
use kakera_avg::{
    content::{ContentSource, DirectorySource},
    interface::{
        self,
        host::{self, Command, Profile, State, UiValue},
    },
    loader,
    package::{self, Archive, Snapshot, StagedProject},
    settings::Settings,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const STORY: &str = r#"<story schema="8" id="title-test" content-version="8" entry="start"><scripts><script src="script.js"/></scripts><interface src="ui.xml"/><assets><font id="font" src="font.otf"/></assets><characters/><visuals/><variables/><frames><frame id="start"><text>Hello</text></frame></frames><next end="true"/></story>"#;
const SCRIPT: &str = r#"on.end('start');on.ui('caption',ctx=>ctx.setUi('caption',ctx.value));on.ui('game.enter',ctx=>ctx.setUi('caption',`章节 · ${ctx.game.frameId}`));"#;
struct Memory(BTreeMap<PathBuf, Vec<u8>>);
impl ContentSource for Memory {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        self.0
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing {}", path.display()))
    }
}
fn source(attributes: &str, initial: &str) -> Memory {
    let ui = format!(
        r#"<ui schema="3" initial="hud" {attributes}><variables><string id="caption" initial="{initial}"/><int id="number" initial="0"/></variables><screen id="hud" modal="false" pause="false"/></ui>"#
    );
    Memory(
        [
            ("story.xml", STORY.to_owned()),
            ("script.js", SCRIPT.to_owned()),
            ("ui.xml", ui),
        ]
        .into_iter()
        .map(|(p, s)| (p.into(), s.into_bytes()))
        .collect(),
    )
}
fn set(text: &str) -> Command {
    Command::SetUi {
        name: "caption".into(),
        value: UiValue::String(text.into()),
    }
}

#[test]
fn initial_titles_support_default_literal_binding_and_unicode() -> Result<()> {
    for (attributes, initial, expected) in [
        ("", "初始", "Kakera AVG"),
        (
            r#"title="中文 &amp; &quot;Game&quot; 🎮""#,
            "初始",
            "中文 & \"Game\" 🎮",
        ),
        (
            r#"title-bind="ui.caption""#,
            "初始标题 &amp; JS",
            "初始标题 & JS",
        ),
    ] {
        let (loaded, _) = loader::load_game(&source(attributes, initial))?;
        assert_eq!(State::new(&loaded.interface).window_title, expected);
    }
    let long = "🎮".repeat(256);
    let (loaded, _) = loader::load_game(&source(r#"title-bind="ui.caption""#, &long))?;
    assert_eq!(State::new(&loaded.interface).window_title, long);
    Ok(())
}

#[test]
fn invalid_title_configuration_keeps_xml_positions() {
    for (attributes, initial) in [
        (
            r#"title="a" title-bind="ui.caption""#.to_owned(),
            "valid".to_owned(),
        ),
        (r#"title="""#.into(), "valid".into()),
        (r#"title="  ""#.into(), "valid".into()),
        (r#"title="a&#xA;b""#.into(), "valid".into()),
        (r#"title-bind="settings.caption""#.into(), "valid".into()),
        (r#"title-bind="ui.unknown""#.into(), "valid".into()),
        (r#"title-bind="ui.number""#.into(), "valid".into()),
        (r#"title-bind="ui.caption""#.into(), " ".into()),
        (r#"title-bind="ui.caption""#.into(), "a&#x9;b".into()),
        (r#"title-bind="ui.caption""#.into(), "中".repeat(257)),
        (format!("title=\"{}\"", "a".repeat(257)), "valid".into()),
    ] {
        let error = loader::load_game(&source(&attributes, &initial))
            .err()
            .expect("must reject");
        assert!(format!("{error:#}").contains("ui.xml:1:"), "{error:#}");
    }
}

#[test]
fn js_title_changes_are_validated_with_the_whole_ui_transaction() -> Result<()> {
    let (loaded, logic) = loader::load_game(&source(r#"title-bind="ui.caption""#, "初始"))?;
    let def = &loaded.interface;
    let state = State::new(def);
    let original = state.clone();
    let settings = Settings::default();
    let profile = Profile::new(&loaded.story.id, def);
    let commands = logic.ui_event(
        &interface::Event {
            name: "caption".into(),
            item: json!(null),
            value: json!("新标题 · JS & 🎮"),
        },
        &loaded.story.start(),
        &state.variables,
        &json!({}),
    )?;
    let plan = host::prepare(def, &state, &settings, &profile, commands, false)?;
    assert_eq!(plan.state.window_title, "新标题 · JS & 🎮");
    for commands in [
        vec![set("")],
        vec![set("a\0b")],
        vec![set("a\nb")],
        vec![set(&"a".repeat(257))],
        vec![Command::SetUi {
            name: "caption".into(),
            value: UiValue::Bool(true),
        }],
        vec![
            set(" "),
            Command::Save {
                slot: 1,
                overwrite: false,
            },
        ],
        vec![
            set("有效标题"),
            Command::Open {
                screen: "unknown".into(),
            },
        ],
    ] {
        assert!(host::prepare(def, &state, &settings, &profile, commands, false).is_err());
        assert_eq!(state, original);
    }
    // Validate the final draft, rather than rejecting a value replaced within the batch.
    let plan = host::prepare(
        def,
        &state,
        &settings,
        &profile,
        vec![set(""), set("最终标题")],
        false,
    )?;
    assert_eq!(plan.state.window_title, "最终标题");
    let enter = logic.ui_event(
        &interface::Event {
            name: "game.enter".into(),
            item: json!(null),
            value: json!(null),
        },
        &loaded.story.start(),
        &state.variables,
        &json!({"game":{"frameId":"start"}}),
    )?;
    assert_eq!(
        host::prepare(def, &state, &settings, &profile, enter, true)?
            .state
            .window_title,
        "章节 · start"
    );
    assert_eq!(State::new(def).window_title, "初始");
    Ok(())
}

#[test]
fn both_packages_preserve_title_configuration_and_js_binding() -> Result<()> {
    let root = tempfile::tempdir()?;
    let output = tempfile::tempdir()?;
    for (path, bytes) in source(r#"title-bind="ui.caption""#, "发布标题").0 {
        std::fs::write(root.path().join(path), bytes)?;
    }
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/fonts/chinese.otf"),
        root.path().join("font.otf"),
    )?;
    for format in [1, 2] {
        let path = output.path().join(format!("title-{format}.kpak"));
        if format == 1 {
            let (snapshot, _) = Snapshot::directory(root.path())?.for_publication()?;
            package::write(&path, &snapshot.encode(1)?, false)?;
        } else {
            let mut staged = StagedProject::capture(root.path())?;
            staged.for_publication()?;
            package::write_streaming(&staged, &path, 1, false, |source| {
                loader::load_game(source)?;
                Ok(())
            })?;
        }
        let archive = Archive::open(&path)?;
        archive.verify_all()?;
        let (loaded, _) = loader::load_game(&archive)?;
        assert_eq!(State::new(&loaded.interface).window_title, "发布标题");
        let state = State::new(&loaded.interface);
        let plan = host::prepare(
            &loaded.interface,
            &state,
            &Settings::default(),
            &Profile::new(&loaded.story.id, &loaded.interface),
            vec![set("发布后修改")],
            false,
        )?;
        assert_eq!(plan.state.window_title, "发布后修改");
    }
    let (loaded, _) = loader::load_game(&DirectorySource::new(root.path())?)?;
    assert_eq!(State::new(&loaded.interface).window_title, "发布标题");
    Ok(())
}
