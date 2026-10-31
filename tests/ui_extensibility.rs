//! v0.10.0 acceptance: dynamic image viewer, configurable shortcuts and HSV colors.
use kakera_avg::{
    content::{ContentSource, DirectorySource, validate_path},
    interface::{
        self, Data, Definition, Event, data,
        host::{self, Command, Operation, Plan, Profile, State, UiValue},
    },
    loader::load_game,
    model::{Logic, Session, Story, VariableValue},
    script::ScriptLogic,
    settings::Settings,
};
use serde_json::{Value, json};
mod common;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
static STORY: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    let source =
        DirectorySource::new(&Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo")).unwrap();
    kakera_avg::story_chain::compile(&source).unwrap().xml
});
static UI: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    let source =
        DirectorySource::new(&Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo")).unwrap();
    interface::source::compile(&source, Path::new("ui.xml"))
        .unwrap()
        .xml
        .replace("initial=\"title\"", "initial=\"hud\"")
});
const UI_JS: &str = include_str!("../games/demo/scripts/ui.js");
struct Memory(BTreeMap<PathBuf, Vec<u8>>);
impl ContentSource for Memory {
    fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
        validate_path(path)?;
        self.0
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing {}", path.display()))
    }
}
fn source(ui: &str, script: &str) -> Memory {
    Memory(
        [
            ("story.xml", STORY.as_str()),
            ("ui.xml", ui),
            ("scripts/ui.js", script),
            (
                "scripts/story.js",
                include_str!("../games/demo/scripts/story.js"),
            ),
            (
                "scripts/endings.js",
                include_str!("../games/demo/scripts/endings.js"),
            ),
        ]
        .into_iter()
        .map(|(p, t)| (PathBuf::from(p), t.as_bytes().to_vec()))
        .collect(),
    )
}
struct Fixture {
    story: Story,
    logic: ScriptLogic,
    definition: Definition,
    state: State,
    profile: Profile,
    settings: Settings,
}
fn fixture_from(ui: &str, script: &str) -> Fixture {
    let (loaded, logic) = load_game(&source(ui, script)).unwrap();
    let state = State::new(&loaded.interface);
    let profile = Profile::new(&loaded.story.id, &loaded.interface);
    Fixture {
        story: loaded.story,
        logic,
        definition: loaded.interface,
        state,
        profile,
        settings: Settings::default(),
    }
}
fn demo() -> Fixture {
    fixture_from(&UI, UI_JS)
}
fn snapshot(f: &Fixture, state: &State, session: &Session) -> Data {
    Data::build(data::Input {
        definition: &f.definition,
        story: &f.story,
        session,
        available: &f.logic.options(&f.story, session).unwrap(),
        state,
        profile: &f.profile,
        slots: &[],
        settings: &f.settings,
        complete: true,
        slot_pages: &[],
        autosaves: &[],
    })
}
fn run_event(
    f: &Fixture,
    state: &State,
    name: &str,
    item: Value,
    value: Value,
) -> anyhow::Result<Plan> {
    let session = f.story.start();
    let data = snapshot(f, state, &session);
    let key = common::event_key(&f.definition, name);
    let commands = f.logic.ui_event(
        &Event {
            sound: None,
            route: None,
            key,
            item,
            value,
        },
        &session,
        &state.variables,
        &data.values,
    )?;
    host::prepare(
        &f.definition,
        state,
        &f.settings,
        &f.profile,
        commands,
        false,
    )
}
fn variable(state: &State, name: &str) -> VariableValue {
    state.variables[name].clone()
}
fn demo_state(f: &Fixture) -> State {
    let mut state = f.state.clone();
    state.stack = vec!["hud".into()];
    state
}

#[test]
fn one_viewer_and_one_image_switch_across_three_resources() {
    let f = demo();
    let state = demo_state(&f);
    let session = f.story.start();
    let opened = run_event(
        &f,
        &state,
        "view-cg",
        json!({"id":"cg.room","image":"bg.room","title":"午后房间","unlocked":true}),
        Value::Null,
    )
    .unwrap();
    assert_eq!(opened.operation, Operation::None);
    assert_eq!(opened.state.stack, vec!["hud", "cg-view"]);
    assert_eq!(
        variable(&opened.state, "cgImage"),
        VariableValue::String("bg.room".into())
    );
    assert_eq!(
        variable(&opened.state, "cgTitle"),
        VariableValue::String("午后房间".into())
    );
    assert_eq!(
        variable(&opened.state, "cgCounter"),
        VariableValue::String("1 / 3".into())
    );
    assert_eq!(
        variable(&opened.state, "cgPrevOk"),
        VariableValue::Bool(false)
    );
    assert_eq!(
        variable(&opened.state, "cgNextOk"),
        VariableValue::Bool(true)
    );
    assert_eq!(
        opened.profile, f.profile,
        "viewing a CG must not change unlocks"
    );
    assert_eq!(opened.state.stack.len(), 2);
    assert_eq!(session.history.len(), 1);

    let next = run_event(&f, &opened.state, "cg-next", Value::Null, Value::Null).unwrap();
    assert_eq!(
        next.state.stack,
        vec!["hud", "cg-view"],
        "switching never reopens the page"
    );
    assert_eq!(
        variable(&next.state, "cgImage"),
        VariableValue::String("portrait.lin".into())
    );
    assert_eq!(
        variable(&next.state, "cgCurrent"),
        VariableValue::String("cg.portrait".into())
    );
    assert_eq!(
        variable(&next.state, "cgCounter"),
        VariableValue::String("2 / 3".into())
    );
    assert_eq!(variable(&next.state, "cgPrevOk"), VariableValue::Bool(true));
    assert_eq!(variable(&next.state, "cgNextOk"), VariableValue::Bool(true));

    let last = run_event(&f, &next.state, "cg-next", Value::Null, Value::Null).unwrap();
    assert_eq!(
        variable(&last.state, "cgImage"),
        VariableValue::String("cg.square".into())
    );
    assert_eq!(
        variable(&last.state, "cgCurrent"),
        VariableValue::String("cg.square".into())
    );
    assert_eq!(
        variable(&last.state, "cgCounter"),
        VariableValue::String("3 / 3".into())
    );
    assert_eq!(
        variable(&last.state, "cgNextOk"),
        VariableValue::Bool(false)
    );
    assert_eq!(
        run_event(&f, &last.state, "cg-next", Value::Null, Value::Null)
            .unwrap()
            .state
            .stack,
        vec!["hud", "cg-view"],
        "the last item is disabled, not wrapped"
    );
    let back = run_event(&f, &last.state, "cg-prev", Value::Null, Value::Null).unwrap();
    assert_eq!(
        variable(&back.state, "cgImage"),
        VariableValue::String("portrait.lin".into())
    );
    assert_eq!(back.profile, f.profile);
}

#[test]
fn locked_unknown_and_background_toolbar_routes_are_bounded() {
    let f = demo();
    let state = demo_state(&f);
    let locked = run_event(
        &f,
        &state,
        "view-cg",
        json!({"id":"cg.lin","image":"portrait.lin","title":"一同出发","unlocked":false}),
        Value::Null,
    )
    .unwrap();
    assert_eq!(
        locked.state.stack,
        vec!["hud"],
        "locked entries are rejected in JS"
    );
    assert_eq!(
        variable(&locked.state, "cgImage"),
        VariableValue::String("bg.room".into())
    );

    // An unlocked gallery entry outside the demo's browse table still opens as a single image.
    let single = run_event(
        &f,
        &state,
        "view-cg",
        json!({"id":"cg.ending","image":"cg.ending","title":"结尾 CG","unlocked":true}),
        Value::Null,
    )
    .unwrap();
    assert_eq!(single.state.stack, vec!["hud", "cg-view"]);
    assert_eq!(
        variable(&single.state, "cgPrevOk"),
        VariableValue::Bool(false)
    );
    assert_eq!(
        variable(&single.state, "cgNextOk"),
        VariableValue::Bool(false)
    );
    assert_eq!(
        variable(&single.state, "cgCounter"),
        VariableValue::String("".into())
    );

    let hidden = run_event(
        &f,
        &single.state,
        "cg-hide-toolbar",
        Value::Null,
        Value::Null,
    )
    .unwrap();
    assert_eq!(
        variable(&hidden.state, "cgToolbar"),
        VariableValue::Bool(false)
    );
    assert_eq!(hidden.state.stack, vec!["hud", "cg-view"]);
    let restored = run_event(&f, &hidden.state, "cg-background", Value::Null, Value::Null).unwrap();
    assert_eq!(
        variable(&restored.state, "cgToolbar"),
        VariableValue::Bool(true)
    );
    assert_eq!(
        restored.state.stack,
        vec!["hud", "cg-view"],
        "first background click only restores"
    );
    let closed = run_event(
        &f,
        &restored.state,
        "cg-background",
        Value::Null,
        Value::Null,
    )
    .unwrap();
    assert_eq!(closed.state.stack, vec!["hud"]);
    assert!(
        host::prepare(
            &f.definition,
            &closed.state,
            &f.settings,
            &f.profile,
            vec![Command::Close],
            false
        )
        .is_err()
    );
}

#[test]
fn image_bind_types_paths_urls_and_batch_failures_are_rejected() {
    let f = demo();
    let bad = format!(
        "{UI_JS}\n(event.register('bad-batch', ctx => {{ ctx.setUi('cgImage', 'assets/bg/room.png'); ctx.setUi('cgTitle', '坏图'); ctx.open('cg-view'); }}), ui('bad-batch').bind('bad-batch'));"
    );
    let bad_fixture = fixture_from(&common::probe(&UI, &["bad-batch"]), &bad);
    let bad_state = demo_state(&bad_fixture);
    let error = run_event(
        &bad_fixture,
        &bad_state,
        "bad-batch",
        Value::Null,
        Value::Null,
    )
    .err()
    .expect("invalid image must fail")
    .to_string();
    assert!(
        error.contains("UI 图片变量引用不存在"),
        "unexpected error: {error}"
    );
    assert_eq!(
        variable(&bad_state, "cgImage"),
        VariableValue::String("bg.room".into())
    );
    assert_eq!(
        variable(&bad_state, "cgTitle"),
        VariableValue::String("午后房间".into())
    );
    assert_eq!(bad_state.stack, vec!["hud"]);

    let minimal = r#"<ui schema="3" initial="hud"><variables><string id="pic" initial="bg.room"/></variables><screen id="hud" modal="false" pause="false"><image id="only" width="100" height="100" image-bind="ui.pic"/></screen></ui>"#;
    let handlers = common::declarations(minimal);
    let parsed = interface::parse(minimal, "ui.xml", &f.story, &handlers).unwrap();
    let profile = Profile::new(&f.story.id, &parsed);
    let settings = Settings::default();
    let mut custom_state = State::new(&parsed);
    custom_state.stack = vec!["hud".into()];
    for value in [
        "https://example.com/a.png",
        "$/ui/panel.png",
        "",
        "missing.image",
    ] {
        let result = host::prepare(
            &parsed,
            &custom_state,
            &settings,
            &profile,
            vec![Command::SetUi {
                name: "pic".into(),
                value: UiValue::String(value.into()),
            }],
            false,
        );
        assert!(result.is_err(), "{value} must be rejected");
    }
    let wrong_type = host::prepare(
        &parsed,
        &custom_state,
        &settings,
        &profile,
        vec![Command::SetUi {
            name: "pic".into(),
            value: UiValue::Bool(true),
        }],
        false,
    );
    assert!(wrong_type.is_err());
    let invalid_initial = minimal.replace("initial=\"bg.room\"", "initial=\"assets/bg/room.png\"");
    assert!(
        interface::parse(&invalid_initial, "ui.xml", &f.story, &handlers).is_err(),
        "an invalid initial image binding fails at load time"
    );
    let wrong_binding = minimal.replace("image-bind=\"ui.pic\"", "image-bind=\"game.frameId\"");
    assert!(interface::parse(&wrong_binding, "ui.xml", &f.story, &handlers).is_err());
}

#[test]
fn image_regions_validate_against_the_switched_texture() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo");
    let source = DirectorySource::new(&root).unwrap();
    let (mut loaded, _logic) = load_game(&source).unwrap();
    let ui = r#"<ui schema="3" initial="hud"><variables><string id="region" initial="0,0,64,64"/></variables><screen id="hud" modal="false" pause="false"><image id="crop" width="100" height="100" image="ui.panel" region-bind="ui.region"/></screen></ui>"#;
    let custom = interface::parse(ui, "ui.xml", &loaded.story, &[]).unwrap();
    let assets = kakera_avg::assets::PreparedAssets::load_deferred(&loaded.story, &source).unwrap();
    let mut custom = custom;
    assets.validate_interface(&mut custom).unwrap();
    let state = State::new(&custom);
    let error = host::prepare(
        &custom,
        &state,
        &Settings::default(),
        &Profile::new(&loaded.story.id, &custom),
        vec![Command::SetUi {
            name: "region".into(),
            value: UiValue::String("1000,500,100,100".into()),
        }],
        false,
    )
    .err()
    .expect("region must be rejected")
    .to_string();
    assert!(
        error.contains("crop"),
        "region error names the node: {error}"
    );
    assert_eq!(state.regions["crop"].0, [0, 0, 64, 64]);
    let _ = &mut loaded.interface;
}

#[test]
fn viewer_layout_is_one_contain_image_over_black_and_comes_from_an_include() {
    let f = demo();
    let screen = &f.definition.screens["cg-view"];
    let image_nodes: Vec<_> = screen
        .nodes
        .iter()
        .filter(|node| node.image_bind.as_deref() == Some("ui.cgImage"))
        .collect();
    assert_eq!(image_nodes.len(), 1, "one fixed image node, not one per CG");
    assert_eq!(image_nodes[0].id, "cgFull");
    assert_eq!(image_nodes[0].image_bind.as_deref(), Some("ui.cgImage"));
    assert_eq!(image_nodes[0].fit, "contain");
    assert_eq!(image_nodes[0].rect, [0.0, 0.0, 1280.0, 720.0]);
    let background = screen
        .nodes
        .iter()
        .find(|node| node.id == "viewerBackground")
        .unwrap();
    assert_eq!(background.color, [0, 0, 0, 255]);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo");
    let compiled =
        interface::source::compile(&DirectorySource::new(&root).unwrap(), Path::new("ui.xml"))
            .unwrap();
    assert!(compiled.files.iter().any(|file| file == "ui/cg-view.xml"));
    assert!(compiled.xml.contains("id=\"cg-view\""));
    assert_eq!(
        compiled.xml.matches("id=\"cgFull\"").count(),
        1,
        "include merging keeps exactly one viewer image"
    );
}

#[test]
fn static_image_cache_survives_hundreds_of_binding_switches() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo");
    let source = DirectorySource::new(&root).unwrap();
    let (mut loaded, _logic) = load_game(&source).unwrap();
    let assets = kakera_avg::assets::PreparedAssets::load_deferred(&loaded.story, &source).unwrap();
    assets.validate_interface(&mut loaded.interface).unwrap();
    let ctx = eframe::egui::Context::default();
    let cache = assets.install(&ctx);
    let ids: Vec<_> = ["bg.room", "portrait.lin", "cg.square"]
        .iter()
        .map(|id| cache.image(id).unwrap().id())
        .collect();
    let mut state = State::new(&loaded.interface);
    state.stack = vec!["hud".into()];
    for index in 0..100 {
        let id = ["bg.room", "portrait.lin", "cg.square"][index % 3];
        let plan = host::prepare(
            &loaded.interface,
            &state,
            &Settings::default(),
            &Profile::new(&loaded.story.id, &loaded.interface),
            vec![Command::SetUi {
                name: "cgImage".into(),
                value: UiValue::String(id.into()),
            }],
            false,
        )
        .unwrap();
        state = plan.state;
    }
    let after: Vec<_> = ["bg.room", "portrait.lin", "cg.square"]
        .iter()
        .map(|id| cache.image(id).unwrap().id())
        .collect();
    assert_eq!(
        ids, after,
        "switching bindings never replaces static textures"
    );
}

fn palette_ui() -> String {
    r##"<ui schema="3" initial="hud">
  <variables><string id="color" initial="#FF0000FF"/><string id="text" initial="#F4EAD8FF"/></variables>
  <palette><color id="accent" initial="#10253BFF"/></palette>
  <screen id="hud" modal="false" pause="false">
    <panel id="tinted" width="400" height="300" color-bind="ui.color"/>
    <label id="tintedText" width="400" height="40" bind="ui.text" color-bind="colors.accent"/>
    <color-picker id="picker" x="100" y="100" width="256" height="282" value="ui.color" event="pick"/>
  </screen>
</ui>"##
        .to_owned()
}
fn color_fixture(ui: &str) -> (Story, Definition, Vec<interface::Registration>) {
    let demo = demo();
    let handlers = common::declarations(ui);
    let definition = interface::parse(ui, "ui.xml", &demo.story, &handlers).unwrap();
    (demo.story, definition, handlers)
}

#[test]
fn palette_color_bind_and_picker_declarations_are_strict() {
    let (story, definition, handlers) = color_fixture(&palette_ui());
    let state = State::new(&definition);
    assert_eq!(definition.palette["accent"].initial.format(), "#10253BFF");
    assert_eq!(state.resolved_colors["accent"].format(), "#10253BFF");
    assert_eq!(state.color_binds["tinted"].format(), "#FF0000FF");
    assert_eq!(state.color_binds["tintedText"].format(), "#10253BFF");
    let picker = definition.screens["hud"]
        .nodes
        .iter()
        .find(|node| node.id == "picker")
        .unwrap();
    assert!(matches!(
        picker.kind,
        interface::Kind::ColorPicker { alpha: true }
    ));
    assert_eq!(picker.bind.as_deref(), Some("ui.color"));

    let profile = Profile::new(&story.id, &definition);
    let settings = Settings::default();
    let changed = host::prepare(
        &definition,
        &state,
        &settings,
        &profile,
        vec![Command::ColorPreference {
            slot: "accent".into(),
            value: Some("#00FF00AA".into()),
        }],
        false,
    )
    .unwrap();
    assert_eq!(changed.operation, Operation::Colors);
    assert_eq!(
        changed.state.resolved_colors["accent"].format(),
        "#00FF00AA"
    );
    assert_eq!(
        changed.state.color_binds["tintedText"].format(),
        "#00FF00AA"
    );
    assert_eq!(changed.state.colors.overrides.len(), 1);
    let reset = host::prepare(
        &definition,
        &changed.state,
        &settings,
        &profile,
        vec![Command::ResetColors {}],
        false,
    )
    .unwrap();
    assert_eq!(reset.state.resolved_colors["accent"].format(), "#10253BFF");
    assert!(reset.state.colors.overrides.is_empty());
    for (commands, label) in [
        (
            vec![Command::ColorPreference {
                slot: "missing".into(),
                value: Some("#00FF00FF".into()),
            }],
            "unknown slot",
        ),
        (
            vec![Command::ColorPreference {
                slot: "accent".into(),
                value: Some("red".into()),
            }],
            "bad color",
        ),
        (
            vec![
                Command::ColorPreference {
                    slot: "accent".into(),
                    value: None,
                },
                Command::ColorPreference {
                    slot: "accent".into(),
                    value: None,
                },
            ],
            "duplicate slot",
        ),
        (
            vec![
                Command::ColorPreference {
                    slot: "accent".into(),
                    value: None,
                },
                Command::ResetColors {},
            ],
            "reset mixed with item",
        ),
        (
            vec![
                Command::ColorPreference {
                    slot: "accent".into(),
                    value: None,
                },
                Command::Setting {
                    name: "muted".into(),
                    value: json!(true),
                },
            ],
            "mixed settings",
        ),
    ] {
        assert!(
            host::prepare(&definition, &state, &settings, &profile, commands, false).is_err(),
            "{label} must be rejected"
        );
    }

    let invalid = |ui: &str, reason: &str| {
        assert!(
            interface::parse(ui, "ui.xml", &story, &handlers).is_err(),
            "{reason} must be rejected"
        );
    };
    invalid(
        &palette_ui().replace("id=\"accent\"", "id=\"Accent\""),
        "slot id case",
    );
    invalid(
        &palette_ui().replace("initial=\"#10253BFF\"", "initial=\"steelblue\""),
        "slot initial",
    );
    invalid(
        &palette_ui().replace(
            "<palette><color id=\"accent\" initial=\"#10253BFF\"/></palette>",
            "<palette><color id=\"a\" initial=\"#10253BFF\"/><color id=\"a\" initial=\"#10253BFF\"/></palette>",
        ),
        "duplicate slot",
    );
    invalid(
        &palette_ui().replace(
            "color-bind=\"ui.color\"",
            "color=\"#FFFFFFFF\" color-bind=\"ui.color\"",
        ),
        "color and color-bind",
    );
    invalid(
        &palette_ui().replace(
            "color-bind=\"colors.accent\"",
            "color-bind=\"colors.missing\"",
        ),
        "unknown color slot",
    );
    invalid(
        &palette_ui().replace("value=\"ui.color\"", "value=\"colors.accent\""),
        "picker needs ui string",
    );
    invalid(
        &palette_ui().replace(" value=\"ui.color\"", ""),
        "picker needs value",
    );
    invalid(
        &palette_ui().replace(
            "value=\"ui.color\"",
            "value=\"ui.color\" color=\"#FFFFFFFF\"",
        ),
        "picker color attr",
    );
    invalid(
        &palette_ui().replace(
            "width=\"256\" height=\"282\"",
            "width=\"120\" height=\"282\"",
        ),
        "picker size",
    );
    // The colour disc is min(width, height - reserved alpha row) and needs 128.
    for (replacement, reason) in [
        ("width=\"128\" height=\"153\"", "alpha picker height"),
        (
            "width=\"128\" height=\"128\"",
            "alpha picker compact square",
        ),
        ("width=\"127\" height=\"282\"", "alpha picker width"),
    ] {
        invalid(
            &palette_ui().replace("width=\"256\" height=\"282\"", replacement),
            reason,
        );
    }
    for (width, height, alpha) in [("128", "128", "false"), ("128", "154", "true")] {
        let ui = palette_ui().replace(
            "width=\"256\" height=\"282\"",
            &format!("width=\"{width}\" height=\"{height}\" alpha=\"{alpha}\""),
        );
        assert!(
            interface::parse(&ui, "ui.xml", &story, &handlers).is_ok(),
            "{width}x{height} alpha={alpha} must be accepted"
        );
    }
    invalid(
        &palette_ui().replace("event=\"pick\"", "event=\"missing\""),
        "picker event",
    );
    // No legacy preview/HEX appearance switches exist; unknown attributes stay rejected.
    for attr in [
        "show-preview=\"false\"",
        "show-hex=\"false\"",
        "preview=\"false\"",
    ] {
        invalid(
            &palette_ui().replace("event=\"pick\"", &format!("event=\"pick\" {attr}")),
            "picker unknown appearance attribute",
        );
    }
    let mut many = String::from(
        r##"<ui schema="3" initial="hud"><variables><string id="color" initial="#FF0000FF"/></variables><screen id="hud" modal="false" pause="false">"##,
    );
    for index in 0..9 {
        many.push_str(&format!(
            r##"<color-picker id="p{index}" x="0" y="0" width="128" height="154" value="ui.color" event="pick" alpha="false"/>"##
        ));
    }
    many.push_str("</screen></ui>");
    assert!(
        interface::parse(&many, "ui.xml", &story, &handlers).is_err(),
        "nine pickers must be rejected"
    );
    invalid(
        &palette_ui()
            .replace("initial=\"#F4EAD8FF\"", "initial=\"not-a-color\"")
            .replace("value=\"ui.color\"", "value=\"ui.text\""),
        "picker initial must still be a valid color",
    );
    let child = palette_ui().replace(
        "value=\"ui.color\" event=\"pick\"/>",
        "value=\"ui.color\" event=\"pick\"><label id=\"child\" width=\"10\" height=\"10\"/></color-picker>",
    );
    assert!(interface::parse(&child, "ui.xml", &story, &handlers).is_err());
}

#[test]
fn preference_files_are_excluded_from_discovery_and_explicit_package_includes() {
    let root = tempfile::tempdir().unwrap();
    let font = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/fonts/chinese.otf");
    std::fs::copy(font, root.path().join("font.otf")).unwrap();
    std::fs::write(
        root.path().join("story.xml"),
        r#"<story schema="8" id="prefs" content-version="1" entry="a"><scripts><script src="story.js"/></scripts><interface src="ui.xml"/><assets><font id="font" src="font.otf"/></assets><characters/><visuals/><frames><frame id="a"><text>甲</text></frame></frames><next end="true"/></story>"#,
    )
    .unwrap();
    std::fs::write(
        root.path().join("ui.xml"),
        r#"<ui schema="3" initial="hud"><screen id="hud" modal="false" pause="false"><scene id="scene" width="1280" height="720"/></screen></ui>"#,
    )
    .unwrap();
    std::fs::write(root.path().join("story.js"), "frame('a').end();").unwrap();
    for name in [
        "keybindings.json",
        "KeyBindings.JSON",
        "ui-colors.json",
        "UI-Colors.Json",
    ] {
        std::fs::write(root.path().join(name), b"{}").unwrap();
    }
    let files = kakera_avg::package::files(root.path()).unwrap();
    for name in [
        "keybindings.json",
        "KeyBindings.JSON",
        "ui-colors.json",
        "UI-Colors.Json",
    ] {
        assert!(
            !files.iter().any(|path| path == Path::new(name)),
            "{name} must not be collected as game content"
        );
    }
    let options = kakera_avg::runtime::LoadOptions {
        core: None,
        no_live2d: true,
    };
    let selection =
        kakera_avg::package::selection::Selection::discover(root.path(), &options).unwrap();
    for path in &selection.paths {
        let lower = path.to_string_lossy().to_ascii_lowercase();
        assert!(!lower.contains("keybindings") && !lower.contains("ui-colors"));
    }
    std::fs::write(
        root.path().join("kakera.package.json"),
        br#"{"schema":1,"include":["keybindings.json"]}"#,
    )
    .unwrap();
    assert!(kakera_avg::package::selection::Selection::discover(root.path(), &options).is_err());
    std::fs::write(
        root.path().join("kakera.package.json"),
        br#"{"schema":1,"include":["UI-COLORS.JSON"]}"#,
    )
    .unwrap();
    assert!(kakera_avg::package::selection::Selection::discover(root.path(), &options).is_err());
}

#[test]
fn non_modal_single_image_root_returns_by_an_explicit_author_route() {
    let f = demo();
    let ui = r##"<ui schema="3" initial="hud">
  <variables><string id="pic" initial="bg.room"/></variables>
  <screen id="hud" modal="false" pause="false"><scene id="scene" width="1280" height="720"/></screen>
  <screen id="solo" modal="false" pause="false"><panel id="soloBg" width="1280" height="720" color="#000000FF"/><image id="soloImage" width="1280" height="720" image-bind="ui.pic" fit="contain"/></screen>
</ui>"##;
    let definition = interface::parse(ui, "ui.xml", &f.story, &[]).unwrap();
    let profile = Profile::new(&f.story.id, &definition);
    let settings = Settings::default();
    let state = State::new(&definition);
    let plan = host::prepare(
        &definition,
        &state,
        &settings,
        &profile,
        vec![Command::Open {
            screen: "solo".into(),
        }],
        false,
    )
    .unwrap();
    assert_eq!(plan.state.stack, vec!["solo"]);
    assert_eq!(plan.operation, Operation::None);
    assert!(
        host::prepare(
            &definition,
            &plan.state,
            &settings,
            &profile,
            vec![Command::Close],
            false
        )
        .is_err()
    );
    let back = host::prepare(
        &definition,
        &plan.state,
        &settings,
        &profile,
        vec![Command::Open {
            screen: "hud".into(),
        }],
        false,
    )
    .unwrap();
    assert_eq!(
        back.state.stack,
        vec!["hud"],
        "non-modal return replaces the page stack"
    );
    let image = definition.screens["solo"]
        .nodes
        .iter()
        .find(|node| node.id == "soloImage")
        .unwrap();
    assert_eq!(image.image_bind.as_deref(), Some("ui.pic"));
    assert_eq!(image.fit, "contain");
    assert_eq!(
        definition.screens["solo"]
            .nodes
            .iter()
            .filter(|node| node.image_bind.as_deref() == Some("ui.pic"))
            .count(),
        1
    );
}

#[test]
fn shared_image_binding_controls_two_nodes_without_touching_independent_bindings() {
    let f = demo();
    let ui = r#"<ui schema="3" initial="hud"><variables><string id="shared" initial="bg.room"/><string id="own" initial="ui.panel"/></variables><screen id="hud" modal="false" pause="false"><image id="a" width="100" height="100" image-bind="ui.shared"/><image id="b" width="100" height="100" image-bind="ui.shared"/><image id="c" width="100" height="100" image-bind="ui.own"/></screen></ui>"#;
    let definition = interface::parse(ui, "ui.xml", &f.story, &[]).unwrap();
    let state = State::new(&definition);
    let profile = Profile::new(&f.story.id, &definition);
    let plan = host::prepare(
        &definition,
        &state,
        &Settings::default(),
        &profile,
        vec![Command::SetUi {
            name: "shared".into(),
            value: UiValue::String("portrait.lin".into()),
        }],
        false,
    )
    .unwrap();
    assert_eq!(
        plan.state.variables["shared"],
        VariableValue::String("portrait.lin".into())
    );
    assert_eq!(
        plan.state.variables["own"],
        VariableValue::String("ui.panel".into())
    );
    assert_eq!(plan.state.stack, vec!["hud"]);
}
