//! Opt-in author catalogs, independent channels and transactional preferences.
use anyhow::Result;
use kakera_avg::{
    content::ContentSource,
    interface::{Data, data, host},
    loader,
    localization::{Requests, Target},
    model::AssetKind,
    settings::Settings,
};
use std::{collections::BTreeMap, path::Path};
#[derive(Clone)]
struct Source(BTreeMap<String, Vec<u8>>);
impl ContentSource for Source {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        self.0
            .get(&path.to_string_lossy().replace('\\', "/"))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing fixture file: {}", path.display()))
    }
}
const STORY: &str = r#"<story schema="8" id="locale-test" content-version="1" entry="opening"><localization enabled="true" src="languages.xml"/><interface src="ui.xml"/><scripts><script src="story.js"/></scripts><assets><font id="font" src="font.otf"/><audio id="voice" src="voice.wav"/><audio id="voice.b" src="other.wav"/></assets><characters><character id="hero" name-key="hero"/></characters><frames><frame id="opening" speaker="hero"><text key="line"/><audio voice="voice"/><options><option id="yes" text-key="yes"/></options></frame><frame id="end"><text>结束。</text></frame></frames><visuals/><next end="true"/></story>"#;
const LANGUAGES: &str = r#"<localization schema="1" default-text="zh-CN" default-voice="ja-JP"><texts><locale id="zh-CN" label="文本甲"><messages src="zh.ftl"/><font ref="font"/></locale><locale id="en-US" label="文本乙"><messages src="test.ftl"/><font ref="font"/></locale></texts><voices><locale id="ja-JP" label="语音甲"><clip key="voice" asset="voice"/></locale><locale id="zh-CN" label="语音乙"><clip key="voice" asset="voice.b"/></locale></voices></localization>"#;
const UI: &str = r#"<ui schema="3" initial="hud" title-key="title"><variables><string id="key" initial="counter"/><int id="n" initial="2"/></variables><screen id="hud" modal="false" pause="false"><label id="line" width="600" height="60" bind="game.text"/><label id="count" y="70" width="600" height="60" text-key-bind="ui.key"><text-arg name="n" bind="ui.n"/></label><button id="apply" x="10" y="180" width="120" height="40" text-key="apply" event="apply"/></screen></ui>"#;
fn fixture() -> Source {
    Source([("story.xml",STORY), ("languages.xml",LANGUAGES),("ui.xml",UI),("story.js","frame('opening').choice('yes', ()=> 'end');frame('end').end();ui('apply').bind('apply', ctx=>{ctx.locale('text','en-US');ctx.locale('voice','zh-CN');});"),("zh.ftl","title = 文本甲\nhero = 角色甲\nline = 台词甲\nyes = 选项甲\napply = 应用\ncounter = 数量 { $n }\n"),("test.ftl","title = TEXT-B\nhero = HERO-B\nline = LINE-B\napply = APPLY-B\ncounter = COUNT-B { $n }\n")].into_iter().map(|(k,v)|(k.into(),v.as_bytes().to_vec())).collect())
}
fn data(loaded: &loader::LoadedStory, state: &host::State) -> Data {
    let story = &loaded.story;
    let def = &loaded.interface;
    let session = story.start();
    Data::build(data::Input {
        definition: def,
        story,
        session: &session,
        available: &["yes".into()],
        state,
        profile: &host::Profile::new(&story.id, def),
        slots: &[],
        settings: &Settings::default(),
        complete: true,
    })
}
#[test]
fn opted_in_catalogs_project_story_ui_and_independent_voice_fallbacks() -> Result<()> {
    let (loaded, _) = loader::load_game(&fixture())?;
    let def = &loaded.interface;
    let mut state = host::State::new(def);
    state.languages = def
        .localization
        .selection(Requests::default(), vec!["en-US".into(), "zh-CN".into()]);
    state.refresh(def)?;
    assert_eq!(state.window_title, "TEXT-B");
    assert!(state.localized_text["count"].contains("COUNT-B"));
    let values = data(&loaded, &state);
    assert_eq!(values.values["game"]["text"], "LINE-B");
    assert_eq!(values.values["game"]["speaker"], "HERO-B");
    assert_eq!(
        values.lists["choices"][0]["text"], "选项甲",
        "missing message uses developer default"
    );
    let frame = &loaded.story.frames["opening"];
    assert_eq!(frame.text, "台词甲", "domain default stays immutable");
    assert_eq!(
        def.localization
            .cue(frame, &state.languages)
            .voice
            .as_deref(),
        Some("voice.b")
    );
    assert_eq!(state.languages.text.as_deref(), Some("en-US"));
    assert_eq!(state.languages.voice.as_deref(), Some("zh-CN"));
    assert_eq!(
        def.localization
            .text(&Target::Frame("opening".into()), Some("en-US"), ""),
        "LINE-B"
    );
    Ok(())
}

#[test]
fn voice_missing_silent_and_empty_catalogs_have_distinct_meanings() -> Result<()> {
    let mut src = fixture();
    src.0.insert(
        "languages.xml".into(),
        LANGUAGES
            .replace("<clip key=\"voice\" asset=\"voice.b\"/>", "")
            .into_bytes(),
    );
    let (loaded, _) = loader::load_game(&src)?;
    let catalog = &loaded.interface.localization;
    let selected = catalog.selection(
        Requests {
            text: None,
            voice: Some("zh-CN".into()),
        },
        Vec::new(),
    );
    assert_eq!(
        catalog
            .cue(&loaded.story.frames["opening"], &selected)
            .voice
            .as_deref(),
        Some("voice")
    );
    src.0.insert(
        "languages.xml".into(),
        LANGUAGES
            .replace("asset=\"voice.b\"", "silent=\"true\"")
            .into_bytes(),
    );
    let (loaded, _) = loader::load_game(&src)?;
    assert_eq!(
        loaded
            .interface
            .localization
            .cue(&loaded.story.frames["opening"], &selected)
            .voice,
        None
    );
    src.0.insert(
        "languages.xml".into(),
        LANGUAGES
            .replace("<clip key=\"voice\" asset=\"voice\"/>", "")
            .into_bytes(),
    );
    assert!(loader::load_game(&src).is_err());
    src.0.insert(
        "languages.xml".into(),
        format!(
            "{}<voices/></localization>",
            LANGUAGES
                .replace(" default-voice=\"ja-JP\"", "")
                .split("<voices>")
                .next()
                .unwrap()
        )
        .into_bytes(),
    );
    let (loaded, _) = loader::load_game(&src)?;
    let selected = loaded.interface.localization.selection(
        Requests {
            text: Some("en-US".into()),
            voice: Some("ja-JP".into()),
        },
        Vec::new(),
    );
    assert_eq!(selected.voice, None);
    assert_eq!(selected.requests.voice.as_deref(), Some("ja-JP"));
    assert_eq!(selected.text.as_deref(), Some("en-US"));
    Ok(())
}

#[test]
fn history_variable_gallery_and_last_page_slot_metadata_use_current_text() -> Result<()> {
    use kakera_avg::{
        controller::{self, Intent},
        save::{SaveStore, SlotId},
    };
    let mut src = fixture();
    let story=STORY.replace("<characters>","<variables><int id=\"score\" initial=\"42\" label-key=\"score\"/></variables><characters>")
        .replace("</assets>","<cg id=\"picture\" src=\"picture.png\"/></assets>");
    src.0.insert("story.xml".into(), story.into_bytes());
    src.0.insert("ui.xml".into(),UI.replace("<variables>","<gallery><cg id=\"picture\" image=\"picture\" title-key=\"gallery-title\"/></gallery><variables>").into_bytes());
    src.0
        .get_mut("zh.ftl")
        .unwrap()
        .extend_from_slice("score = 得分\ngallery-title = 画廊甲\n".as_bytes());
    src.0
        .get_mut("test.ftl")
        .unwrap()
        .extend_from_slice(b"score = SCORE-B\ngallery-title = GALLERY-B\n");
    let (loaded, logic) = loader::load_game(&src)?;
    let story = &loaded.story;
    let def = &loaded.interface;
    let mut state = host::State::new(def);
    state.languages = def.localization.selection(
        Requests {
            text: Some("en-US".into()),
            voice: None,
        },
        Vec::new(),
    );
    state.slot_page = 20;
    state.refresh(def)?;
    let initial = story.start();
    let session =
        controller::transition_with(story, &initial, Intent::Choose("yes".into()), &logic)?.session;
    let root = tempfile::tempdir()?;
    let store = SaveStore::at(root.path().to_owned());
    store.save_with(SlotId::new(200)?, story, &initial, &logic)?;
    store.autosave_with(story, &initial, &logic)?;
    let slots = store.list_page(story, 20)?;
    let profile = host::Profile::new(&story.id, def);
    let values = Data::build(data::Input {
        definition: def,
        story,
        session: &session,
        available: &[],
        state: &state,
        profile: &profile,
        slots: &slots,
        settings: &Settings::default(),
        complete: true,
    });
    assert_eq!(values.lists["history"][0]["text"], "LINE-B");
    assert_eq!(values.lists["history"][0]["speaker"], "HERO-B");
    assert!(
        values.lists["history"][0]["choice"]
            .as_str()
            .unwrap()
            .contains("选项甲")
    );
    assert_eq!(values.lists["variables"][0]["label"], "SCORE-B");
    assert_eq!(values.lists["variables"][0]["value"], "42");
    assert_eq!(values.lists["gallery"][0]["title"], "GALLERY-B");
    assert_eq!(values.lists["slots"][9]["preview"], "LINE-B");
    let (automatic, _) = store.auto_metadata_with(story, &|id| {
        def.localization
            .frame_text(&story.frames[id], Some("en-US"))
    });
    assert_eq!(automatic[0]["preview"], "LINE-B");
    assert!(store.load_with(SlotId::new(200)?, story, &logic)?.frame_id == "opening");
    Ok(())
}
#[test]
fn disabled_catalogs_ignore_unreferenced_bad_files_but_reject_new_key_syntax() -> Result<()> {
    let mut src = fixture();
    src.0.insert("languages.xml".into(), b"broken".to_vec());
    src.0.insert(
        "story.xml".into(),
        STORY
            .replace("<localization enabled=\"true\" src=\"languages.xml\"/>", "")
            .into_bytes(),
    );
    assert!(
        loader::load_game(&src).is_err(),
        "key syntax cannot silently activate localization"
    );
    let text = String::from_utf8(src.0["story.xml"].clone())?
        .replace("name-key=\"hero\"", "name=\"甲\"")
        .replace("<text key=\"line\"/>", "<text>甲。</text>")
        .replace(
            "<option id=\"yes\" text-key=\"yes\"/>",
            "<option id=\"yes\">是</option>",
        );
    src.0.insert("story.xml".into(), text.into_bytes());
    src.0.insert(
        "ui.xml".into(),
        br#"<ui schema="3" initial="hud"><screen id="hud" modal="false"/></ui>"#.to_vec(),
    );
    src.0.insert(
        "story.js".into(),
        b"frame('opening').choice('yes', ()=> 'end');frame('end').end();".to_vec(),
    );
    let (loaded, _) = loader::load_game(&src)?;
    assert!(!loaded.interface.localization.enabled());
    Ok(())
}
#[test]
fn catalog_errors_and_final_dynamic_ui_bindings_fail_atomically() -> Result<()> {
    let src = fixture();
    for invalid in [
        LANGUAGES.replace("default-text=\"zh-CN\"", "default-text=\"fr-FR\""),
        LANGUAGES.replace("asset=\"voice\"", "asset=\"font\""),
        LANGUAGES.replace("id=\"en-US\"", "id=\"en_us\""),
    ] {
        let mut bad = src.clone();
        bad.0.insert("languages.xml".into(), invalid.into_bytes());
        assert!(loader::load_game(&bad).is_err());
    }
    let (loaded, _) = loader::load_game(&src)?;
    let def = &loaded.interface;
    let state = host::State::new(def);
    let profile = host::Profile::new(&loaded.story.id, def);
    let settings = Settings::default();
    let commands = vec![
        host::Command::Locale {
            channel: "text".into(),
            locale: Some("en-US".into()),
        },
        host::Command::SetUi {
            name: "key".into(),
            value: host::UiValue::String("missing".into()),
        },
    ];
    assert!(host::prepare(def, &state, &settings, &profile, commands, false).is_err());
    assert_eq!(state.window_title, "文本甲");
    assert!(
        host::prepare(
            def,
            &state,
            &settings,
            &profile,
            vec![host::Command::Locale {
                channel: "text".into(),
                locale: Some("fr-FR".into())
            }],
            false
        )
        .is_err()
    );
    assert!(
        host::prepare(
            def,
            &state,
            &settings,
            &profile,
            vec![host::Command::Locale {
                channel: "text".into(),
                locale: None
            }],
            true
        )
        .is_err()
    );
    let mut bad = src.clone();
    bad.0
        .insert("test.ftl".into(), b"line = { missing }".to_vec());
    assert!(loader::load_game(&bad).is_err());
    Ok(())
}
#[test]
fn locale_resources_survive_ui_and_story_assembly_and_preferences_stay_private() -> Result<()> {
    let mut src = fixture();
    src.0.insert(
        "ui.xml".into(),
        br#"<ui schema="3" initial="hud" title-key="title"><include src="page.xml"/></ui>"#
            .to_vec(),
    );
    src.0.insert(
        "page.xml".into(),
        UI.replace(" initial=\"hud\" title-key=\"title\"", "")
            .into_bytes(),
    );
    let (loaded, _) = loader::load_game(&src)?;
    assert!(loaded.interface.localization.enabled());
    let directory = tempfile::tempdir()?;
    for file in ["language.json", "LANGUAGE.JSON"] {
        std::fs::write(directory.path().join(file), b"private")?;
    }
    assert!(kakera_avg::package::files(directory.path())?.is_empty());
    assert!(loaded.story.assets["font"].kind == AssetKind::Font);
    Ok(())
}

#[test]
fn both_package_formats_preserve_all_languages_aliases_and_source_bytes() -> Result<()> {
    use kakera_avg::{
        package::{self, Archive, Snapshot, StagedProject, selection::Selection},
        runtime::LoadOptions,
    };
    let game = tempfile::tempdir()?;
    let out = tempfile::tempdir()?;
    let mut src = fixture();
    // Exercise publication UUID generation and merged UI without rewriting source files.
    src.0.insert(
        "story.xml".into(),
        STORY
            .replace("id=\"opening\"", "name=\"opening\"")
            .into_bytes(),
    );
    src.0.insert(
        "ui.xml".into(),
        br#"<ui schema="3" initial="hud" title-key="title"><include src="page.xml"/></ui>"#
            .to_vec(),
    );
    src.0.insert(
        "page.xml".into(),
        UI.replace(" initial=\"hud\" title-key=\"title\"", "")
            .into_bytes(),
    );
    for (name, bytes) in &src.0 {
        std::fs::write(game.path().join(name), bytes)?;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets");
    std::fs::copy(root.join("fonts/chinese.otf"), game.path().join("font.otf"))?;
    std::fs::copy(root.join("audio/lin.wav"), game.path().join("voice.wav"))?;
    std::fs::copy(root.join("audio/knock.wav"), game.path().join("other.wav"))?;
    std::fs::write(game.path().join("language.json"), b"private")?;
    let options = LoadOptions {
        core: None,
        no_live2d: true,
    };
    let selection = Selection::discover(game.path(), &options)?;
    assert!(
        selection.paths.contains(Path::new("zh.ftl"))
            && selection.paths.contains(Path::new("test.ftl"))
    );
    assert!(!selection.paths.contains(Path::new("language.json")));
    let paths = selection.paths.iter().cloned().collect::<Vec<_>>();
    for format in [1, 2] {
        let path = out.path().join(format!("locale-{format}.kpak"));
        if format == 1 {
            let (published, _) =
                Snapshot::directory_selected(game.path(), &paths)?.for_publication()?;
            package::write(&path, &published.encode(1)?, false)?;
        } else {
            let mut staged = StagedProject::capture_selected(game.path(), paths.clone())?;
            staged.for_publication()?;
            package::write_streaming(&staged, &path, 1, false, |source| {
                loader::load_game(source)?;
                Ok(())
            })?;
        }
        let archive = Archive::open(&path)?;
        archive.verify_all()?;
        let (loaded, _) = loader::load_game(&archive)?;
        let state = host::State::new(&loaded.interface);
        assert_eq!(loaded.interface.localization.texts.len(), 2);
        assert_eq!(loaded.interface.localization.voices.len(), 2);
        assert_eq!(
            loaded.story.frames[&loaded.story.entries[0]].reference(),
            "opening"
        );
        assert_ne!(loaded.story.entries[0], "opening");
        assert!(archive.read(Path::new("page.xml")).is_err());
        assert_eq!(state.window_title, "文本甲");
    }
    assert_eq!(
        std::fs::read(game.path().join("story.xml"))?,
        src.0["story.xml"]
    );
    Ok(())
}

#[test]
fn language_preference_commit_and_failed_atomic_write_keep_old_ui() -> Result<()> {
    use eframe::egui;
    use kakera_avg::{
        app::PlayerApp,
        package::Snapshot,
        render::Renderer,
        runtime::{Bundle, LoadOptions},
        save::SaveStore,
    };
    let game = tempfile::tempdir()?;
    let saves = tempfile::tempdir()?;
    for (name, bytes) in fixture().0 {
        std::fs::write(game.path().join(name), bytes)?;
    }
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/fonts/chinese.otf"),
        game.path().join("font.otf"),
    )?;
    let wav = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/audio/lin.wav");
    std::fs::copy(&wav, game.path().join("voice.wav"))?;
    std::fs::copy(&wav, game.path().join("other.wav"))?;
    let open = || -> Result<(PlayerApp, egui::Context)> {
        let b = Bundle::load(
            &Snapshot::directory(game.path())?,
            &LoadOptions {
                core: None,
                no_live2d: true,
            },
        )?;
        let ctx = egui::Context::default();
        let app = PlayerApp::new_context(
            &ctx,
            b.loaded,
            b.assets,
            SaveStore::at(saves.path().to_owned()),
            true,
            b.logic,
            Renderer::cpu(),
        )?;
        Ok((app, ctx))
    };
    let (mut app, ctx) = open()?;
    let update = |app: &mut PlayerApp, events| {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 720.0),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |ctx| app.update_ui(ctx),
        );
    };
    update(&mut app, vec![]);
    std::fs::create_dir(saves.path().join("language.json"))?;
    let click = |pressed| {
        vec![
            egui::Event::PointerMoved(egui::pos2(50.0, 200.0)),
            egui::Event::PointerButton {
                pos: egui::pos2(50.0, 200.0),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            },
        ]
    };
    let before = app.window_title().to_string();
    update(&mut app, click(true));
    update(&mut app, click(false));
    assert_eq!(app.window_title(), before);
    drop(app);
    std::fs::remove_dir(saves.path().join("language.json"))?;
    let (mut app, ctx) = open()?;
    let mut update = |events| {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 720.0),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |ctx| app.update_ui(ctx),
        );
    };
    update(vec![]);
    update(click(true));
    update(click(false));
    drop(update);
    assert_eq!(app.window_title(), "TEXT-B");
    let file = std::fs::read(saves.path().join("language.json"))?;
    assert_eq!(
        kakera_avg::localization::Preferences::load(&file, "locale-test")?
            .voice
            .as_deref(),
        Some("zh-CN")
    );
    assert_eq!(open()?.0.window_title(), "TEXT-B");
    Ok(())
}
