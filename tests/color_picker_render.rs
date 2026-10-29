//! Work package C: triangle picker interaction produces bounded UI events.
use kakera_avg::{
    assets::PreparedAssets,
    color::Rgba,
    content::DirectorySource,
    interface::{self, Data, data, host, render},
    settings::Settings,
};
use std::path::Path;

struct Fixture {
    ctx: eframe::egui::Context,
    definition: interface::Definition,
    state: host::State,
    data: Data,
    frame: kakera_avg::model::Frame,
    effects: kakera_avg::effects::Player,
    assets: kakera_avg::assets::AssetCache,
    pickers: render::PickerState,
}

fn fixture(ui: &str) -> Fixture {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo");
    let source = DirectorySource::new(&root).unwrap();
    let (loaded, _logic) = kakera_avg::loader::load_game(&source).unwrap();
    let handlers = ["pick".to_owned()].into_iter().collect();
    let definition = interface::parse(ui, "ui.xml", &loaded.story, &handlers).unwrap();
    let state = host::State::new(&definition);
    let profile = host::Profile::new(&loaded.story.id, &definition);
    let session = loaded.story.start();
    let frame = loaded.story.frame(&session).unwrap().clone();
    let data = Data::build(data::Input {
        definition: &definition,
        story: &loaded.story,
        session: &session,
        available: &[],
        state: &state,
        profile: &profile,
        slots: &[],
        settings: &Settings::default(),
        complete: true,
    });
    let assets = PreparedAssets::load_deferred(&loaded.story, &source).unwrap();
    let ctx = eframe::egui::Context::default();
    let cache = assets.install(&ctx);
    Fixture {
        ctx,
        definition,
        state,
        data,
        frame,
        effects: Default::default(),
        assets: cache,
        pickers: Default::default(),
    }
}

fn ui(alpha: bool) -> String {
    format!(
        r##"<ui schema="3" initial="hud">
  <variables><string id="color" initial="#FF0000FF"/></variables>
  <screen id="hud" modal="false" pause="false">
    <color-picker id="picker" x="100" y="100" width="256" height="320" value="ui.color" event="pick" alpha="{alpha}"/>
  </screen>
</ui>"##
    )
}

fn frame(fixture: &mut Fixture, events: Vec<eframe::egui::Event>) -> Option<interface::Event> {
    use eframe::egui;
    let mut out = None;
    let _ = fixture.ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 720.0),
            )),
            focused: true,
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                out = render::draw(
                    ui,
                    render::Input {
                        effects: &fixture.effects,
                        definition: &fixture.definition,
                        state: &fixture.state,
                        data: &fixture.data,
                        frame: &fixture.frame,
                        assets: &fixture.assets,
                        visible_text: "",
                        blocked: false,
                        pickers: &mut fixture.pickers,
                    },
                );
            });
        },
    );
    out
}

fn pointer(pos: eframe::egui::Pos2, pressed: bool) -> Vec<eframe::egui::Event> {
    use eframe::egui;
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        },
    ]
}
fn key(key: eframe::egui::Key) -> Vec<eframe::egui::Event> {
    vec![
        eframe::egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers: eframe::egui::Modifiers::default(),
        },
        eframe::egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: false,
            repeat: false,
            modifiers: eframe::egui::Modifiers::default(),
        },
    ]
}

fn tab(fixture: &mut Fixture, backwards: bool) {
    let mut events = key(eframe::egui::Key::Tab);
    for event in &mut events {
        if let eframe::egui::Event::Key { modifiers, .. } = event {
            modifiers.shift = backwards;
        }
    }
    frame(fixture, events);
    // egui defers reverse focus traversal until the next frame.
    frame(fixture, vec![]);
}

#[test]
fn picker_focus_invalidates_for_parent_visibility_disabling_and_clipping() {
    let xml = ui(true)
        .replace(
            "<color-picker",
            "<panel id=\"parent\" width=\"1280\" height=\"720\"><color-picker",
        )
        .replace("</screen>", "</panel></screen>");
    let mut f = fixture(&xml);
    let original = f.definition.clone();
    for case in 0..5 {
        f.definition = original.clone();
        frame(&mut f, pointer(eframe::egui::pos2(237.2, 215.3), true));
        frame(&mut f, pointer(eframe::egui::pos2(237.2, 215.3), false));
        assert!(f.pickers.focus.is_some());
        f.pickers.take_release_request();
        let parent = &mut f.definition.screens.get_mut("hud").unwrap().nodes[0];
        match case {
            0 => parent.visible = "false".into(),
            1 => parent.enabled = "false".into(),
            2 => parent.children[0].enabled = "false".into(),
            3 => parent.children[0].rect[0] = 2000.0,
            _ => parent.rect[2] = 50.0,
        }
        frame(&mut f, vec![]);
        assert!(f.pickers.focus.is_none(), "case {case}");
        assert!(
            f.ctx.memory(|memory| memory.focused().is_none()),
            "case {case}"
        );
        assert!(f.pickers.take_release_request(), "case {case}");
    }
}

#[test]
fn tab_traverses_buttons_and_two_pickers_in_both_directions() {
    let xml = ui(true).replace("<color-picker", "<button id=\"before\" width=\"80\" height=\"30\" text=\"前\" event=\"pick\"/><color-picker")
        .replace("</screen>", "<color-picker id=\"second\" x=\"500\" y=\"100\" width=\"256\" height=\"320\" value=\"ui.color\" event=\"pick\" alpha=\"false\"/><button id=\"after\" x=\"900\" width=\"80\" height=\"30\" text=\"后\" event=\"pick\"/></screen>");
    let mut f = fixture(&xml);
    frame(&mut f, vec![]);
    tab(&mut f, false);
    assert!(f.pickers.focus.is_none());
    assert_eq!(
        frame(&mut f, key(eframe::egui::Key::Enter)).unwrap().value,
        serde_json::Value::Null
    );
    for (node, area) in [
        ("picker", render::PickerArea::Ring),
        ("picker", render::PickerArea::Triangle),
        ("picker", render::PickerArea::Alpha),
        ("second", render::PickerArea::Ring),
        ("second", render::PickerArea::Triangle),
    ] {
        tab(&mut f, false);
        let focus = f.pickers.focus.as_ref().unwrap();
        assert_eq!((&*focus.node, focus.area), (node, area));
    }
    tab(&mut f, false);
    assert!(f.pickers.focus.is_none());
    assert_eq!(
        frame(&mut f, key(eframe::egui::Key::Enter)).unwrap().value,
        serde_json::Value::Null
    );
    tab(&mut f, true);
    assert_eq!(f.pickers.focus.as_ref().unwrap().node, "second");
    assert_eq!(
        f.pickers.focus.as_ref().unwrap().area,
        render::PickerArea::Triangle
    );
    tab(&mut f, true);
    assert_eq!(
        f.pickers.focus.as_ref().unwrap().area,
        render::PickerArea::Ring
    );
    tab(&mut f, true);
    assert_eq!(f.pickers.focus.as_ref().unwrap().node, "picker");
    assert_eq!(
        f.pickers.focus.as_ref().unwrap().area,
        render::PickerArea::Alpha
    );
}

#[test]
fn triangle_ring_alpha_and_keyboard_emit_one_canonical_event() {
    let mut f = fixture(&ui(true));
    // Triangle interior near the hue vertex (top of the triangle).
    let event = frame(&mut f, pointer(eframe::egui::pos2(237.2, 215.3), true)).unwrap();
    let color = Rgba::parse(event.value.as_str().unwrap()).unwrap();
    assert_eq!(event.name, "pick");
    assert_eq!(event.value.as_str().unwrap(), color.format());
    assert_ne!(event.value.as_str().unwrap(), "#FF0000FF");
    assert!(f.pickers.focus.is_some());
    assert!(frame(&mut f, pointer(eframe::egui::pos2(237.2, 215.3), false)).is_none());

    // Pressing the preview area does not start editing or emit a value.
    assert!(frame(&mut f, pointer(eframe::egui::pos2(140.0, 340.0), true)).is_none());
    assert!(frame(&mut f, pointer(eframe::egui::pos2(140.0, 340.0), false)).is_none());

    // Hue ring at 120 degrees.
    let event = frame(&mut f, pointer(eframe::egui::pos2(322.0, 267.0), true)).unwrap();
    assert!(event.value.as_str().unwrap().starts_with('#'));
    assert!(!event.value.as_str().unwrap().starts_with("#FF0000"));
    assert_eq!(
        f.pickers.focus.as_ref().unwrap().area,
        render::PickerArea::Ring
    );
    frame(&mut f, pointer(eframe::egui::pos2(322.0, 267.0), false));

    // The alpha bar at the bottom edits only alpha, and the preview stays inert.
    let event = frame(&mut f, pointer(eframe::egui::pos2(200.0, 365.0), true)).unwrap();
    assert!(!event.value.as_str().unwrap().ends_with("FF"));
    assert_eq!(
        f.pickers.focus.as_ref().unwrap().area,
        render::PickerArea::Alpha
    );
    frame(&mut f, pointer(eframe::egui::pos2(200.0, 365.0), false));

    // Refocus the ring before testing keyboard steps.
    frame(&mut f, pointer(eframe::egui::pos2(322.0, 267.0), true));
    frame(&mut f, pointer(eframe::egui::pos2(322.0, 267.0), false));
    assert_eq!(
        f.pickers.focus.as_ref().unwrap().area,
        render::PickerArea::Ring
    );

    // Arrow keys edit the focused ring and Tab moves to the triangle.
    let original = f.data.values["ui"]["color"].as_str().unwrap().to_owned();
    let event = frame(&mut f, key(eframe::egui::Key::ArrowRight)).unwrap();
    assert_ne!(event.value.as_str().unwrap(), original);
    frame(&mut f, key(eframe::egui::Key::Tab));
    assert_eq!(
        f.pickers.focus.as_ref().unwrap().area,
        render::PickerArea::Triangle
    );
    frame(&mut f, key(eframe::egui::Key::Tab));
    assert_eq!(
        f.pickers.focus.as_ref().unwrap().area,
        render::PickerArea::Alpha
    );
    frame(&mut f, key(eframe::egui::Key::Tab));
    assert!(f.pickers.focus.is_none());
}

#[test]
fn disabled_picker_and_alpha_false_skip_hidden_areas() {
    let f = fixture(&ui(false));
    assert_eq!(f.pickers.focus, None);
    let disabled = ui(false).replace("event=\"pick\"", "event=\"pick\" enabled=\"false\"");
    let mut f2 = fixture(&disabled);
    assert!(frame(&mut f2, pointer(eframe::egui::pos2(237.2, 215.3), true)).is_none());
    assert!(f2.pickers.focus.is_none());
    assert!(frame(&mut f2, pointer(eframe::egui::pos2(237.2, 215.3), false)).is_none());

    // alpha=false still shows a preview; clicking its old alpha row must not edit.
    let picker = f.definition.screens["hud"]
        .nodes
        .iter()
        .find(|node| node.id == "picker")
        .unwrap();
    assert!(matches!(
        picker.kind,
        interface::Kind::ColorPicker { alpha: false }
    ));
}

#[test]
fn press_and_release_in_one_frame_still_edits_the_picker() {
    use eframe::egui;
    let mut f = fixture(&ui(true));
    let events = vec![
        egui::Event::PointerMoved(egui::pos2(237.2, 215.3)),
        egui::Event::PointerButton {
            pos: egui::pos2(237.2, 215.3),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        },
        egui::Event::PointerButton {
            pos: egui::pos2(237.2, 215.3),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        },
    ];
    let event = frame(&mut f, events).unwrap();
    assert_eq!(event.name, "pick");
    assert_ne!(event.value.as_str().unwrap(), "#FF0000FF");
    assert_eq!(
        f.pickers.focus.as_ref().unwrap().area,
        render::PickerArea::Triangle
    );
}

#[test]
fn review_hidden_picker_must_release_focus() {
    let mut f = fixture(&ui(true));
    frame(&mut f, pointer(eframe::egui::pos2(237.2, 215.3), true));
    frame(&mut f, pointer(eframe::egui::pos2(237.2, 215.3), false));
    assert!(f.pickers.focus.is_some());
    f.definition.screens.get_mut("hud").unwrap().nodes[0].visible = "false".into();
    frame(&mut f, vec![]);
    assert!(
        f.pickers.focus.is_none(),
        "hidden picker retained keyboard ownership"
    );
}
#[test]
fn tab_enters_each_picker_area_without_mouse_and_leaves() {
    let mut f = fixture(&ui(true));
    frame(&mut f, vec![]);
    for area in [
        render::PickerArea::Ring,
        render::PickerArea::Triangle,
        render::PickerArea::Alpha,
    ] {
        frame(&mut f, key(eframe::egui::Key::Tab));
        assert_eq!(f.pickers.focus.as_ref().map(|focus| focus.area), Some(area));
    }
    frame(&mut f, key(eframe::egui::Key::Tab));
    assert!(f.pickers.focus.is_none());
}
#[test]
fn review_picker_first_focus_must_block_same_frame_advance() -> anyhow::Result<()> {
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
    std::fs::write(
        game.path().join("story.xml"),
        r#"<story schema="8" id="review-input" content-version="1" entry="opening"><scripts><script src="story.js"/></scripts><interface src="ui.xml"/><assets><font id="font" src="font.otf"/></assets><characters/><visuals/><frames><frame id="opening"><text>第一句。</text></frame><frame id="second"><text>第二句。</text></frame></frames><next end="true"/></story>"#,
    )?;
    std::fs::copy(
        "games/demo/assets/fonts/chinese.otf",
        game.path().join("font.otf"),
    )?;
    std::fs::write(
        game.path().join("story.js"),
        "on.advance('opening',()=> 'second');on.end('second');on.ui('start',ctx=>ctx.restart('opening'));on.ui('pick',ctx=>{ctx.setUi('color',ctx.value);ctx.setUi('show',false);});on.ui('game.enter',ctx=>ctx.setUi('title',ctx.game.frameId));",
    )?;
    std::fs::write(
        game.path().join("ui.xml"),
        r##"<ui schema="3" initial="hud" title-bind="ui.title"><variables><string id="title" initial="ready"/><string id="color" initial="#FF0000FF"/><bool id="show" initial="true"/></variables><screen id="hud" modal="false" pause="false"><button id="start" x="10" y="10" width="80" height="30" event="start" text="开始"/><color-picker id="picker" x="100" y="100" width="256" height="320" value="ui.color" event="pick" visible="ui.show"/></screen></ui>"##,
    )?;
    Settings {
        text_speed: 0.0,
        autosave: false,
        ..Default::default()
    }
    .save(&saves.path().join("settings.json"))?;
    let b = Bundle::load(
        &Snapshot::directory(game.path())?,
        &LoadOptions {
            core: None,
            no_live2d: true,
        },
    )?;
    let ctx = egui::Context::default();
    let mut app = PlayerApp::new_context(
        &ctx,
        b.loaded,
        b.assets,
        SaveStore::at(saves.path().to_owned()),
        true,
        b.logic,
        Renderer::cpu(),
    )?;
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
    update(&mut app, pointer(egui::pos2(30.0, 20.0), true));
    update(&mut app, pointer(egui::pos2(30.0, 20.0), false));
    update(&mut app, vec![]);
    assert_eq!(app.window_title(), "opening");
    let mut events = pointer(egui::pos2(228.0, 116.0), true);
    events.extend(key(egui::Key::Space));
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
    assert_eq!(
        app.window_title(),
        "opening",
        "picker click and Space advanced story in one update"
    );
    update(&mut app, pointer(egui::pos2(228.0, 116.0), false));
    // The author's color callback hides the currently focused picker. A key
    // still held while the control disappears must not leak into the story.
    update(&mut app, key(egui::Key::ArrowRight));
    let space_down = egui::Event::Key {
        key: egui::Key::Space,
        physical_key: Some(egui::Key::Space),
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    };
    update(&mut app, vec![space_down]);
    assert_eq!(app.window_title(), "opening");
    update(&mut app, key(egui::Key::Space));
    update(&mut app, vec![]);
    update(&mut app, key(egui::Key::Space));
    assert_eq!(
        app.window_title(),
        "second",
        "fresh press must work after the hidden picker releases ownership"
    );
    Ok(())
}
