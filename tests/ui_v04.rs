use kakera_avg::{
    content::{ContentSource, DirectorySource, validate_path},
    controller::{Intent, transition_with},
    interface::{
        self, Data, Definition, Event, data,
        host::{self, Command, Operation, Profile, State, UiValue},
    },
    loader::load_game,
    model::{Logic, Session, Story, VariableValue as V},
    save::{SaveStore, SlotId, SlotInfo, SlotState, Snapshot},
    script::ScriptLogic,
    settings::Settings,
};
use serde_json::{Value, json};
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
    // These mutation tests operate on one complete UI document, as published.
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
    slots: Vec<SlotInfo>,
    viewport: eframe::egui::Vec2,
    blocked: bool,
    focused: bool,
}
fn fixture() -> Fixture {
    let (loaded, logic) = load_game(&source(&UI, UI_JS)).unwrap();
    let definition = loaded.interface;
    let state = State::new(&definition);
    let profile = Profile::new(&loaded.story.id, &definition);
    let slots = SlotId::all()
        .map(|slot| SlotInfo {
            slot,
            state: SlotState::Empty,
        })
        .collect();
    Fixture {
        story: loaded.story,
        logic,
        definition,
        state,
        profile,
        settings: Settings::default(),
        slots,
        viewport: eframe::egui::vec2(1280.0, 720.0),
        blocked: false,
        focused: true,
    }
}
fn snapshot(f: &Fixture, session: &Session, complete: bool) -> Data {
    Data::build(data::Input {
        definition: &f.definition,
        story: &f.story,
        session,
        available: &f.logic.options(&f.story, session).unwrap(),
        state: &f.state,
        profile: &f.profile,
        slots: &f.slots,
        settings: &f.settings,
        complete,
    })
}
fn commands(f: &Fixture, name: &str, item: Value, value: Value) -> Vec<Command> {
    let session = f.story.start();
    f.logic
        .ui_event(
            &Event {
                sound: None,
                route: None,
                name: name.into(),
                item,
                value,
            },
            &session,
            &f.state.variables,
            &snapshot(f, &session, true).values,
        )
        .unwrap()
}
fn plan(f: &Fixture, commands: Vec<Command>) -> anyhow::Result<host::Plan> {
    host::prepare(
        &f.definition,
        &f.state,
        &f.settings,
        &f.profile,
        commands,
        false,
    )
}

#[test]
fn author_defined_pages_skins_and_callbacks_load_without_native_screen_code() {
    let f = fixture();
    assert_eq!(f.definition.screens.len(), 16);
    let opened = plan(&f, commands(&f, "open-save", Value::Null, Value::Null)).unwrap();
    assert_eq!(opened.state.stack, vec!["hud", "save"]);
    assert!(opened.state.paused(&f.definition));
    assert!(!opened.state.keyboard(&f.definition));
    let xml=UI.replace("</ui>","<screen id=\"credits\"><label id=\"creditsText\" width=\"1000\" height=\"100\" text=\"作者定制页\"/></screen></ui>")
        .replace("event=\"open-gallery\"","event=\"credits\"");
    let js = format!("{UI_JS}\non.ui('credits',ctx=>ctx.open('credits')); ");
    let (loaded, logic) = load_game(&source(&xml, &js)).unwrap();
    let def = loaded.interface;
    let cmds = logic
        .ui_event(
            &Event {
                sound: None,
                route: None,
                name: "credits".into(),
                item: Value::Null,
                value: Value::Null,
            },
            &loaded.story.start(),
            &def.initial_variables(),
            &json!({"game":{},"settings":{}}),
        )
        .unwrap();
    let result = host::prepare(
        &def,
        &State::new(&def),
        &Settings::default(),
        &Profile::new(&loaded.story.id, &def),
        cmds,
        false,
    )
    .unwrap();
    assert_eq!(result.state.stack.last().unwrap(), "credits");
}

#[test]
fn ui_xml_rejects_invalid_layout_bindings_references_and_events_with_location() {
    for (from, to) in [
        ("schema=\"3\"", "schema=\"99\""),
        (
            "width=\"1280\" height=\"720\"",
            "width=\"1280\" height=\"800\"",
        ),
        ("id=\"brand\"", "id=\"scene\""),
        ("width=\"510\"", "width=\"NaN\""),
        ("width=\"510\"", "width=\"-1\""),
        ("font-size=\"19\"", "font-size=\"0\""),
        ("color=\"#EEDDB8\"", "color=\"#GGGGGG\""),
        ("bind=\"game.speaker\"", "bind=\"game.missing\""),
        ("value=\"settings.text_speed\"", "value=\"settings.muted\""),
        ("visible=\"game.ended\"", "visible=\"game.text\""),
        ("visible=\"game.ended\"", "visible=\"not:not:game.ended\""),
        ("image=\"ui.panel\"", "image=\"voice.lin\""),
        ("image=\"ui.panel\"", "image=\"missing\""),
        ("event=\"open-gallery\"", "event=\"missing\""),
        ("event=\"open-gallery\"", "event=\"game.enter\""),
        ("source=\"slots\"", "source=\"unknown\""),
        ("columns=\"2\"", "columns=\"2.5\""),
        ("text=\"KAKERA / XML UI\"", "text=\"KAKERA\" css=\"x\""),
        ("fit=\"contain\"", "fit=\"unknown\""),
        ("initial=\"hud\"", "initial=\"gallery\""),
    ] {
        assert!(UI.contains(from));
        let error = load_game(&source(&UI.replace(from, to), UI_JS))
            .err()
            .unwrap();
        let message = format!("{error:#}");
        assert!(message.contains("ui.xml"), "{message}");
        if !from.starts_with("initial=") {
            assert!(message.contains("ui.xml:"), "{message}");
        }
    }
    let xml = UI.replace(
        "<scene id=\"scene\" width=\"1280\" height=\"720\" />",
        "<unknown id=\"x\" width=\"1\" height=\"1\" />",
    );
    assert!(load_game(&source(&xml, UI_JS)).is_err());
    let deep = format!(
        "<ui schema=\"3\" initial=\"hud\"><screen id=\"hud\" modal=\"false\">{}{}</screen></ui>",
        (0..9)
            .map(|i| format!("<panel id=\"p{i}\" width=\"1\" height=\"1\">"))
            .collect::<String>(),
        "</panel>".repeat(9)
    );
    assert!(load_game(&source(&deep, UI_JS)).is_err());
}

#[test]
fn command_batches_roll_back_local_state_settings_and_collection_on_any_error() {
    let f = fixture();
    let original = f.state.clone();
    let profile = f.profile.clone();
    let settings = f.settings.clone();
    for bad in [
        Command::Open {
            screen: "missing".into(),
        },
        Command::Load { slot: 0 },
        Command::SetUi {
            name: "missing".into(),
            value: UiValue::Bool(true),
        },
        Command::Setting {
            name: "bgm_volume".into(),
            value: json!(2.0),
        },
        Command::Unlock {
            id: "missing".into(),
        },
    ] {
        assert!(
            plan(
                &f,
                vec![
                    Command::SetUi {
                        name: "status".into(),
                        value: UiValue::String("partial".into())
                    },
                    bad
                ]
            )
            .is_err()
        );
        assert_eq!(f.state, original);
        assert_eq!(f.profile, profile);
        assert_eq!(f.settings, settings);
    }
    assert!(
        plan(
            &f,
            vec![
                Command::Save {
                    slot: 1,
                    overwrite: false
                },
                Command::Setting {
                    name: "muted".into(),
                    value: json!(true)
                }
            ]
        )
        .is_err()
    );
    assert!(
        plan(
            &f,
            vec![Command::SetUi {
                name: "cgImage".into(),
                value: UiValue::String("voice.lin".into())
            }]
        )
        .is_err()
    );
    assert!(
        plan(
            &f,
            vec![Command::SetUi {
                name: "pendingSlot".into(),
                value: UiValue::Int("201".into())
            }]
        )
        .is_err()
    );
    assert!(
        plan(
            &f,
            (0..33)
                .map(|_| Command::Message { text: "x".into() })
                .collect()
        )
        .is_err()
    );
}

#[test]
fn modal_stack_root_switch_and_close_are_validated() {
    let f = fixture();
    assert!(plan(&f, vec![Command::Close]).is_err());
    assert!(
        plan(
            &f,
            vec![
                Command::Open {
                    screen: "save".into()
                },
                Command::Open {
                    screen: "save".into()
                }
            ]
        )
        .is_err()
    );
    let result = plan(
        &f,
        vec![
            Command::Open {
                screen: "save".into(),
            },
            Command::Open {
                screen: "confirm-save".into(),
            },
            Command::Close,
        ],
    )
    .unwrap();
    assert_eq!(result.state.stack, vec!["hud", "save"]);
    let result = host::prepare(
        &f.definition,
        &result.state,
        &f.settings,
        &f.profile,
        vec![Command::Open {
            screen: "hud".into(),
        }],
        false,
    )
    .unwrap();
    assert_eq!(result.state.stack, vec!["hud"]);
    assert!(result.state.keyboard(&f.definition));
}

#[test]
fn save_confirmation_and_settings_are_authored_in_js_and_xml() {
    let mut f = fixture();
    f.state.stack.push("save".into());
    let empty = plan(
        &f,
        commands(
            &f,
            "save-slot",
            json!({"slot":2,"exists":false}),
            Value::Null,
        ),
    )
    .unwrap();
    assert_eq!(
        empty.operation,
        Operation::Save(SlotId::new(2).unwrap(), false)
    );
    assert_eq!(empty.state.stack, vec!["hud"]);
    let existing = plan(
        &f,
        commands(
            &f,
            "save-slot",
            json!({"slot":2,"exists":true}),
            Value::Null,
        ),
    )
    .unwrap();
    assert_eq!(existing.operation, Operation::None);
    assert_eq!(existing.state.stack, vec!["hud", "save", "confirm-save"]);
    f.state = existing.state;
    assert_eq!(f.state.variables["pendingSlot"], V::Int(2));
    let confirm = plan(&f, commands(&f, "confirm-save", Value::Null, Value::Null)).unwrap();
    assert_eq!(
        confirm.operation,
        Operation::Save(SlotId::new(2).unwrap(), true)
    );
    assert_eq!(confirm.state.stack, vec!["hud"]);
    let change = plan(&f, commands(&f, "text-speed", Value::Null, json!(12))).unwrap();
    assert_eq!(change.settings.text_speed, 12.0);
    assert_eq!(change.operation, Operation::Settings);
    let dir = tempfile::tempdir().unwrap();
    change
        .settings
        .save(&dir.path().join("settings.json"))
        .unwrap();
    assert_eq!(
        Settings::load(&dir.path().join("settings.json")).unwrap(),
        change.settings
    );
}

#[test]
fn gallery_is_independent_of_story_slots_and_preserves_atomic_writes() {
    let f = fixture();
    assert!(f.profile.unlocked.contains("cg.room"));
    assert!(!f.profile.unlocked.contains("cg.lin"));
    let mut snapshot = snapshot(&f, &f.story.start(), true);
    snapshot.values["game"]["frameId"] = json!("accept");
    let event = Event {
        sound: None,
        route: None,
        name: "game.enter".into(),
        item: Value::Null,
        value: Value::Null,
    };
    let commands = f
        .logic
        .ui_event(
            &event,
            &f.story.start(),
            &f.state.variables,
            &snapshot.values,
        )
        .unwrap();
    let result = host::prepare(
        &f.definition,
        &f.state,
        &f.settings,
        &f.profile,
        commands,
        true,
    )
    .unwrap();
    assert_eq!(result.operation, Operation::Profile);
    assert!(result.profile.unlocked.contains("cg.lin"));
    let dir = tempfile::tempdir().unwrap();
    let profile_path = dir.path().join("profile.json");
    result.profile.save(&profile_path).unwrap();
    let loaded = Profile::load(&profile_path, &f.story.id, &f.definition).unwrap();
    assert_eq!(loaded, result.profile);
    let saves = SaveStore::at(dir.path().into());
    let slot = SlotId::new(1).unwrap();
    saves
        .save_with(slot, &f.story, &f.story.start(), &f.logic)
        .unwrap();
    saves.load_with(slot, &f.story, &f.logic).unwrap();
    assert_eq!(
        Profile::load(&profile_path, &f.story.id, &f.definition).unwrap(),
        loaded
    );
    let bytes = std::fs::read(&profile_path).unwrap();
    let mut bad: Value = serde_json::from_slice(&bytes).unwrap();
    bad["unlocked"] = json!(["unknown"]);
    std::fs::write(&profile_path, bad.to_string()).unwrap();
    assert!(Profile::load(&profile_path, &f.story.id, &f.definition).is_err());
    std::fs::write(&profile_path, bytes).unwrap();
    let blocked = dir.path().join("blocked");
    std::fs::create_dir(&blocked).unwrap();
    std::fs::write(blocked.join("keep"), "old").unwrap();
    assert!(loaded.save(&blocked).is_err());
    assert_eq!(
        std::fs::read_to_string(blocked.join("keep")).unwrap(),
        "old"
    );
}

#[test]
fn lifecycle_is_not_run_by_history_replay_and_cannot_reenter_gameplay() {
    let f = fixture();
    let question = transition_with(&f.story, &f.story.start(), Intent::Advance, &f.logic)
        .unwrap()
        .session;
    let snapshot = Snapshot::capture_with(&f.story, &question, &f.logic).unwrap();
    assert_eq!(snapshot.restore_with(&f.story, &f.logic).unwrap(), question);
    assert!(!f.profile.unlocked.contains("cg.lin"));
    for command in [
        Command::Advance,
        Command::Load { slot: 1 },
        Command::Open {
            screen: "gallery".into(),
        },
        Command::Setting {
            name: "muted".into(),
            value: json!(true),
        },
    ] {
        assert!(
            host::prepare(
                &f.definition,
                &f.state,
                &f.settings,
                &f.profile,
                vec![command],
                true
            )
            .is_err()
        );
    }
    let js = UI_JS.replace(
        "if (ctx.game.frameId === \"accept\") ctx.unlock(\"cg.lin\");",
        "ctx.advance();",
    );
    assert!(load_game(&source(&UI, &js)).is_err());
}

#[test]
fn ui_callbacks_are_read_only_isolated_bounded_and_reject_async() {
    let f = fixture();
    for body in [
        "ctx.game.frameId='bad';",
        "ctx.ui.status='bad';",
        "ctx.item.x=1;",
        "return Promise.resolve();",
        "Promise.resolve().then(()=>ctx.close());",
        "while(true){}",
        "for(let i=0;i<33;i++)ctx.close();",
    ] {
        let js = format!("{UI_JS}\non.ui('test',ctx=>{{{body}}});");
        let (loaded, logic) = load_game(&source(&UI, &js)).unwrap();
        assert!(
            logic
                .ui_event(
                    &Event {
                        sound: None,
                        route: None,
                        name: "test".into(),
                        item: json!({}),
                        value: Value::Null
                    },
                    &loaded.story.start(),
                    &f.state.variables,
                    &snapshot(&f, &f.story.start(), true).values
                )
                .is_err(),
            "{body}"
        );
    }
    let js = format!("{UI_JS}\nlet n=0;on.ui('test',ctx=>ctx.message(String(++n))); ");
    let (loaded, logic) = load_game(&source(&UI, &js)).unwrap();
    for _ in 0..2 {
        let out = logic
            .ui_event(
                &Event {
                    sound: None,
                    route: None,
                    name: "test".into(),
                    item: Value::Null,
                    value: Value::Null,
                },
                &loaded.story.start(),
                &f.state.variables,
                &snapshot(&f, &f.story.start(), true).values,
            )
            .unwrap();
        assert!(matches!(&out[0],Command::Message{text} if text=="1"));
    }
}

#[test]
fn bound_lists_and_progress_obey_story_history_and_choice_gating() {
    let mut f = fixture();
    let question = transition_with(&f.story, &f.story.start(), Intent::Advance, &f.logic)
        .unwrap()
        .session;
    let data = snapshot(&f, &question, false);
    assert!(!data.flag("game.complete", &Value::Null));
    assert!(data.flag("game.canAdvance", &Value::Null));
    let data = snapshot(&f, &question, true);
    assert!(!data.flag("game.canAdvance", &Value::Null));
    assert_eq!(data.lists["choices"].len(), 6);
    assert_eq!(data.lists["slots"].len(), 200);
    assert!(!data.lists["slots"][0]["loadable"].as_bool().unwrap());
    f.slots[0].state = SlotState::Invalid("broken".into());
    let data = snapshot(&f, &question, true);
    assert!(data.lists["slots"][0]["exists"].as_bool().unwrap());
    assert!(!data.lists["slots"][0]["loadable"].as_bool().unwrap());
    assert_eq!(data.lists["history"].len(), 2);
    assert_eq!(data.lists["variables"].len(), 4);
}

#[test]
fn ui_and_all_skins_are_loaded_through_the_content_source_boundary() {
    let directory =
        DirectorySource::new(&Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo")).unwrap();
    let mut memory = source(&UI, UI_JS);
    let (loaded, _) = load_game(&memory).unwrap();
    for asset in loaded.story.assets.values() {
        memory
            .0
            .insert(asset.path.clone(), directory.read(&asset.path).unwrap());
    }
    let assets = kakera_avg::assets::PreparedAssets::load_from(&loaded.story, &memory).unwrap();
    assets.validate_text(&loaded.interface.text()).unwrap();
    assert!(!memory.0.contains_key(Path::new("flow.xml")));
    let mut missing = source(&UI, UI_JS);
    missing.0.remove(Path::new("ui.xml"));
    assert!(load_game(&missing).is_err());
}

fn render_frame(
    ctx: &eframe::egui::Context,
    f: &Fixture,
    assets: &kakera_avg::assets::AssetCache,
    session: &Session,
    data: &Data,
    events: Vec<eframe::egui::Event>,
) -> Option<Event> {
    use eframe::egui;
    let mut event = None;
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, f.viewport)),
            events,
            focused: f.focused,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    event = interface::render::draw(
                        ui,
                        interface::render::Input {
                            effects: &Default::default(),
                            definition: &f.definition,
                            state: &f.state,
                            data,
                            frame: &f.story.frames[&session.frame_id],
                            assets,
                            visible_text: if data.flag("game.complete", &Value::Null) {
                                &f.story.frames[&session.frame_id].text
                            } else {
                                ""
                            },
                            blocked: f.blocked,
                        },
                    );
                });
        },
    );
    event
}
fn click(
    ctx: &eframe::egui::Context,
    f: &Fixture,
    assets: &kakera_avg::assets::AssetCache,
    session: &Session,
    data: &Data,
    x: f32,
    y: f32,
) -> Option<Event> {
    use eframe::egui;
    render_frame(ctx, f, assets, session, data, vec![]);
    let point = egui::pos2(x, y);
    render_frame(
        ctx,
        f,
        assets,
        session,
        data,
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
    );
    render_frame(
        ctx,
        f,
        assets,
        session,
        data,
        vec![egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    )
}
#[test]
fn renderer_uses_xml_hitboxes_and_repeat_items_without_running_callbacks() {
    let mut f = fixture();
    let directory =
        DirectorySource::new(&Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo")).unwrap();
    let ctx = eframe::egui::Context::default();
    let assets = kakera_avg::assets::PreparedAssets::load_from(&f.story, &directory)
        .unwrap()
        .install(&ctx);
    let session = f.story.start();
    let data = snapshot(&f, &session, true);
    let event = click(&ctx, &f, &assets, &session, &data, 940.0, 39.0).unwrap();
    assert_eq!(event.name, "open-save");
    assert_eq!(f.state.stack, vec!["hud"]);
    f.state.stack.push("save".into());
    let data = snapshot(&f, &session, true);
    assert!(click(&ctx, &f, &assets, &session, &data, 940.0, 39.0).is_none());
    let event = click(&ctx, &f, &assets, &session, &data, 545.0, 198.0).unwrap();
    assert_eq!(event.name, "save-slot");
    assert_eq!(event.item["slot"], json!(1));
    f.state.stack = vec!["hud".into()];
    let question = transition_with(&f.story, &session, Intent::Advance, &f.logic)
        .unwrap()
        .session;
    let data = snapshot(&f, &question, false);
    // Hidden choices leave background available for revealing the unfinished line.
    assert_eq!(
        click(&ctx, &f, &assets, &question, &data, 1000.0, 184.0)
            .unwrap()
            .name,
        "background-advance"
    );
    let data = snapshot(&f, &question, true);
    let event = click(&ctx, &f, &assets, &question, &data, 1000.0, 184.0).unwrap();
    assert_eq!(event.item["id"], json!("accept"));
}

#[test]
fn background_gestures_respect_buttons_menus_bars_focus_and_press_origin() {
    use eframe::egui::{self, PointerButton as B};
    let mut f = fixture();
    let directory =
        DirectorySource::new(&Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo")).unwrap();
    let ctx = egui::Context::default();
    let assets = kakera_avg::assets::PreparedAssets::load_from(&f.story, &directory)
        .unwrap()
        .install(&ctx);
    let session = f.story.start();
    let gesture = |f: &Fixture, button, press: [f32; 2], release: [f32; 2]| {
        let data = snapshot(f, &session, true);
        render_frame(&ctx, f, &assets, &session, &data, vec![]);
        render_frame(
            &ctx,
            f,
            &assets,
            &session,
            &data,
            vec![
                egui::Event::PointerMoved(press.into()),
                egui::Event::PointerButton {
                    pos: press.into(),
                    button,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        render_frame(
            &ctx,
            f,
            &assets,
            &session,
            &data,
            vec![
                egui::Event::PointerMoved(release.into()),
                egui::Event::PointerButton {
                    pos: release.into(),
                    button,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ],
        )
    };
    assert_eq!(
        gesture(&f, B::Primary, [400.0, 300.0], [400.0, 300.0])
            .unwrap()
            .name,
        "background-advance"
    );
    assert_eq!(
        gesture(&f, B::Secondary, [400.0, 300.0], [400.0, 300.0])
            .unwrap()
            .name,
        "toggle-hud"
    );
    assert!(gesture(&f, B::Secondary, [940.0, 39.0], [940.0, 39.0]).is_none());
    assert_eq!(
        gesture(&f, B::Primary, [940.0, 39.0], [940.0, 39.0])
            .unwrap()
            .name,
        "open-save"
    );
    // A short drag off a button is still not a background click.
    assert!(
        gesture(&f, B::Primary, [983.0, 39.0], [987.0, 39.0]).is_none_or(|e| e.name == "open-save")
    );
    assert!(gesture(&f, B::Primary, [300.0, 300.0], [600.0, 300.0]).is_none());
    let data = snapshot(&f, &session, true);
    let button = |b, pressed, x| egui::Event::PointerButton {
        pos: egui::pos2(x, 300.0),
        button: b,
        pressed,
        modifiers: Default::default(),
    };
    let both = render_frame(
        &ctx,
        &f,
        &assets,
        &session,
        &data,
        vec![
            egui::Event::PointerMoved(egui::pos2(400.0, 300.0)),
            button(B::Primary, true, 400.0),
            button(B::Secondary, true, 400.0),
            button(B::Primary, false, 400.0),
            button(B::Secondary, false, 400.0),
        ],
    )
    .unwrap();
    assert_eq!(both.name, "toggle-hud");
    // Coalesced events must use the raw press position, not just the final pointer position.
    let event = render_frame(
        &ctx,
        &f,
        &assets,
        &session,
        &data,
        vec![
            egui::Event::PointerMoved(egui::pos2(983.0, 39.0)),
            egui::Event::PointerButton {
                pos: egui::pos2(983.0, 39.0),
                button: B::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
            egui::Event::PointerMoved(egui::pos2(987.0, 39.0)),
            egui::Event::PointerButton {
                pos: egui::pos2(987.0, 39.0),
                button: B::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
        ],
    );
    assert!(event.is_none_or(|event| event.name == "open-save"));
    f.blocked = true;
    assert!(gesture(&f, B::Primary, [400.0, 300.0], [400.0, 300.0]).is_none());
    f.blocked = false;
    f.focused = false;
    assert!(gesture(&f, B::Secondary, [400.0, 300.0], [400.0, 300.0]).is_none());
    f.focused = true;
    f.viewport = egui::vec2(1600.0, 720.0);
    assert!(gesture(&f, B::Primary, [60.0, 300.0], [60.0, 300.0]).is_none());
    assert_eq!(
        gesture(&f, B::Secondary, [600.0, 300.0], [600.0, 300.0])
            .unwrap()
            .name,
        "toggle-hud"
    );
    f.viewport = egui::vec2(1280.0, 720.0);
    f.state.stack.push("load".into());
    assert!(gesture(&f, B::Secondary, [400.0, 300.0], [400.0, 300.0]).is_none());
    assert!(gesture(&f, B::Primary, [545.0, 198.0], [545.0, 198.0]).is_none()); // disabled empty-slot button
    // Even an explicitly enabled menu background must respect disabled buttons and its repeat area.
    f.definition
        .screens
        .get_mut("load")
        .unwrap()
        .background_click = Some("background-advance".into());
    assert!(gesture(&f, B::Primary, [545.0, 198.0], [545.0, 198.0]).is_none());
    assert!(gesture(&f, B::Primary, [300.0, 300.0], [300.0, 300.0]).is_none());
    assert_eq!(
        gesture(&f, B::Primary, [60.0, 300.0], [60.0, 300.0])
            .unwrap()
            .name,
        "background-advance"
    );
}

#[test]
fn hud_callbacks_restore_before_advancing_and_screen_handlers_are_validated() {
    let mut f = fixture();
    let hidden = plan(&f, commands(&f, "toggle-hud", Value::Null, Value::Null)).unwrap();
    assert_eq!(hidden.operation, Operation::None);
    assert_eq!(hidden.state.variables["hudVisible"], V::Bool(false));
    f.state = hidden.state;
    let restored = plan(
        &f,
        commands(&f, "background-advance", Value::Null, Value::Null),
    )
    .unwrap();
    assert_eq!(restored.operation, Operation::None);
    assert_eq!(restored.state.variables["hudVisible"], V::Bool(true));
    f.state = restored.state;
    let advance = plan(
        &f,
        commands(&f, "background-advance", Value::Null, Value::Null),
    )
    .unwrap();
    assert_eq!(advance.operation, Operation::Advance);
    for handler in ["missing-handler", "game.enter"] {
        let xml = UI.replace(
            "background-click=\"background-advance\"",
            &format!("background-click=\"{handler}\""),
        );
        assert!(load_game(&source(&xml, UI_JS)).is_err());
    }
}

#[test]
fn declarative_button_route_is_emitted_by_renderer_and_disabled_buttons_are_blocked() {
    use eframe::egui;
    let mut f = fixture();
    let directory =
        DirectorySource::new(&Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo")).unwrap();
    let ctx = egui::Context::default();
    let assets = kakera_avg::assets::PreparedAssets::load_from(&f.story, &directory)
        .unwrap()
        .install(&ctx);
    let session = f.story.start();
    let data = snapshot(&f, &session, true);
    let click = |f: &Fixture| {
        render_frame(&ctx, f, &assets, &session, &data, vec![]);
        render_frame(
            &ctx,
            f,
            &assets,
            &session,
            &data,
            vec![
                egui::Event::PointerMoved(egui::pos2(740., 39.)),
                egui::Event::PointerButton {
                    pos: egui::pos2(740., 39.),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        render_frame(
            &ctx,
            f,
            &assets,
            &session,
            &data,
            vec![egui::Event::PointerButton {
                pos: egui::pos2(740., 39.),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        )
    };
    let event = click(&f).unwrap();
    assert_eq!(
        event.route,
        Some(interface::UiRoute::Open("settings".into()))
    );
    assert!(event.name.is_empty());
    let button = f
        .definition
        .screens
        .get_mut("hud")
        .unwrap()
        .nodes
        .iter_mut()
        .find(|n| n.id == "hudControls")
        .unwrap()
        .children
        .iter_mut()
        .find(|n| n.id == "settingsButton")
        .unwrap();
    button.enabled = "false".into();
    assert!(click(&f).is_none());
}
