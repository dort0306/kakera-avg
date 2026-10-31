//! v0.11.3: master volume and bounded 2D image particles.
use kakera_avg::{
    content::{ContentSource, DirectorySource, validate_path},
    interface::{
        self, Node,
        host::{self, Command, Profile, State},
    },
    loader::{self, LoadedStory},
    particles::Action,
    script::ScriptLogic,
    settings::Settings,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

struct Game(BTreeMap<PathBuf, Vec<u8>>);
impl ContentSource for Game {
    fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
        validate_path(path)?;
        self.0
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing {}", path.display()))
    }
}
const STORY: &str = r#"<story schema="8" id="v0113" content-version="11" entry="opening">
  <scripts><script src="scripts/ui.js"/></scripts>
  <interface src="ui.xml"/>
  <assets><font id="font" src="font.otf"/><image id="dot" src="dot.png"/></assets>
  <characters/><visuals/>
  <frames><frame id="opening"><text>开场</text></frame></frames>
  <next end="true"/>
</story>"#;
fn source(ui: &str, script: &str) -> Game {
    Game(
        [
            ("story.xml", STORY),
            ("ui.xml", ui),
            ("scripts/ui.js", script),
        ]
        .into_iter()
        .map(|(path, text)| (PathBuf::from(path), text.as_bytes().to_vec()))
        .collect(),
    )
}
fn load(ui: &str, script: &str) -> anyhow::Result<(LoadedStory, ScriptLogic)> {
    loader::load_game(&source(ui, script))
}
fn error(ui: &str, script: &str) -> String {
    match load(ui, script) {
        Ok(_) => panic!("invalid content must be rejected"),
        Err(cause) => format!("{cause:#}"),
    }
}
fn ui(body: &str) -> String {
    format!(
        r#"<ui schema="3" initial="hud"><variables><bool id="flag" initial="false"/></variables><screen id="hud" modal="false" pause="false">{body}</screen></ui>"#
    )
}
fn particles(attributes: &str) -> String {
    format!(r#"<particles id="p" x="0" y="0" width="100" height="100" image="dot" {attributes}/>"#)
}
fn find<'a>(nodes: &'a [Node], id: &str) -> Option<&'a Node> {
    nodes.iter().find_map(|node| {
        if node.id == id {
            Some(node)
        } else {
            find(&node.children, id)
        }
    })
}
const NOOP: &str = "frame('opening').end();event.register('noop', ctx => ctx.setUi('flag', true));";

#[test]
fn master_volume_is_a_validated_number_and_never_rewrites_channels() -> anyhow::Result<()> {
    let xml = ui(
        r#"<slider id="master" x="0" y="0" width="200" height="30" value="settings.master_volume" min="0" max="1" event="master"/><button id="b" x="0" y="0" width="10" height="10" text="b" event="noop"/>"#,
    );
    let script =
        format!("event.register('master', ctx => ctx.setting('master_volume', ctx.value));{NOOP}");
    let (loaded, logic) = load(&xml, &script)?;
    let definition = &loaded.interface;
    let state = State::new(definition);
    let profile = Profile::new(&loaded.story.id, definition);
    let settings = Settings::default();
    let prepare = |name: &str, value: Value| {
        host::prepare(
            definition,
            &state,
            &settings,
            &profile,
            vec![Command::Setting {
                name: name.into(),
                value,
            }],
            false,
        )
    };
    let plan = prepare("master_volume", json!(0.25))?;
    assert_eq!(plan.settings.master_volume, 0.25);
    assert!(matches!(plan.operation, host::Operation::Settings));
    for bad in [
        json!(1.000_000_1),
        json!(-0.25),
        json!(2),
        json!("0.5"),
        json!(true),
        json!(null),
    ] {
        assert!(
            prepare("master_volume", bad.clone()).is_err(),
            "{bad} must be rejected before the f32 cast"
        );
    }
    let plan = prepare("master_volume", json!(0.0))?;
    assert_eq!(plan.settings.master_volume, 0.0);
    assert_eq!(plan.settings.bgm_volume, settings.bgm_volume);
    assert_eq!(plan.settings.voice_volume, settings.voice_volume);
    assert_eq!(plan.settings.se_volume, settings.se_volume);
    assert_eq!(plan.settings.video_volume, settings.video_volume);
    // The XML slider resolves through the ordinary `settings.*` number whitelist.
    let slider = find(&definition.screens["hud"].nodes, "master").expect("slider");
    assert_eq!(slider.bind.as_deref(), Some("settings.master_volume"));
    assert!(matches!(slider.kind, interface::Kind::Slider { .. }));
    let _ = logic;
    Ok(())
}

#[test]
fn settings_files_require_the_complete_current_field_set() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("settings.json");
    let settings = Settings {
        master_volume: 0.4,
        ..Settings::default()
    };
    settings.save(&path)?;
    assert_eq!(Settings::load(&path)?, settings);
    let saved: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    assert_eq!(saved["master_volume"].as_f64().unwrap() as f32, 0.4);
    let object = saved.as_object().expect("settings object").clone();
    // A file written before this field existed is rejected, not silently upgraded.
    let mut old = object.clone();
    old.remove("master_volume");
    assert!(!old.contains_key("master_volume"));
    std::fs::write(&path, serde_json::to_vec(&old)?)?;
    assert!(Settings::load(&path).is_err());
    let mut unknown = object.clone();
    unknown.insert("extra".into(), json!(1));
    std::fs::write(&path, serde_json::to_vec(&unknown)?)?;
    assert!(Settings::load(&path).is_err());
    for bad in [1.5, -0.1, f32::NAN, f32::INFINITY] {
        assert!(
            Settings {
                master_volume: bad,
                ..settings.clone()
            }
            .save(&path)
            .is_err()
        );
    }
    // The previous valid file survives every failed write.
    settings.save(&path)?;
    assert_eq!(Settings::load(&path)?, settings);
    Ok(())
}

#[test]
fn particle_declarations_are_strict_typed_and_bounded() -> anyhow::Result<()> {
    let (loaded, _) = load(&ui(&particles(r#"rate="12""#)), NOOP)?;
    let emitter = &loaded.interface.particles["p"];
    assert_eq!(emitter.screen, "hud");
    assert_eq!(emitter.max, 128);
    assert!(emitter.fade && !emitter.autostart);
    assert_eq!(emitter.life, [2.0, 2.0]);

    let cases = [
        (particles(r#"rate="10" unknown="x""#), "未知属性"),
        (
            r#"<particles id="p" x="0" y="0" width="100" height="100" rate="10"/>"#.to_string(),
            "缺少非空属性",
        ),
        (particles(r#""#), "rate 或 burst"),
        (particles(r#"rate="0" burst="0""#), "rate 或 burst"),
        (particles(r#"rate="10" burst="129""#), "整数参数必须是 0–"),
        (particles(r#"rate="10" max="0""#), "max 必须至少为 1"),
        (particles(r#"rate="10" max="513""#), "整数参数必须是 0–"),
        (particles(r#"rate="201""#), "必须在"),
        (particles(r#"rate="10" life="9" life-max="12""#), "必须在"),
        (particles(r#"rate="10" size="10" size-max="4""#), "不能小于"),
        (particles(r#"rate="10" vx="10" vx-max="-10""#), "不能小于"),
        (particles(r#"rate="10" fade="yes""#), "true/false"),
        (particles(r##"rate="10" color="#12""##), "颜色"),
        (
            r#"<particles id="p" x="0" y="0" width="100" height="100" image="missing" rate="10"/>"#.to_string(),
            "UI 图片引用不存在",
        ),
        (
            r#"<repeat id="r" x="0" y="0" width="100" height="100" source="variables" item-width="10" item-height="10"><particles id="p" x="0" y="0" width="10" height="10" image="dot" rate="1"/></repeat>"#.to_string(),
            "不能声明在 repeat 内",
        ),
    ];
    for (body, expected) in cases {
        let message = error(&ui(&body), NOOP);
        assert!(
            message.contains(expected),
            "expected {expected:?} for {body}: {message}"
        );
    }
    // At most four emitters per page and 32 declarations overall.
    let many = |screens: usize, per_screen: usize| {
        let mut body = String::new();
        for screen in 0..screens {
            let id = if screen == 0 {
                "hud".to_owned()
            } else {
                format!("s{screen}")
            };
            let modal = if screen == 0 { "false" } else { "true" };
            body.push_str(&format!(
                "<screen id=\"{id}\" modal=\"{modal}\" pause=\"false\">"
            ));
            for index in 0..per_screen {
                body.push_str(&format!(
                    "<particles id=\"p{screen}_{index}\" x=\"0\" y=\"0\" width=\"10\" height=\"10\" image=\"dot\" rate=\"1\"/>"
                ));
            }
            body.push_str("</screen>");
        }
        format!("<ui schema=\"3\" initial=\"hud\">{body}</ui>")
    };
    let message = error(&many(1, 5), NOOP);
    assert!(message.contains("的粒子发射器超过"), "{message}");
    let message = error(&many(9, 4), NOOP);
    assert!(message.contains("最多 32"), "{message}");
    Ok(())
}

#[test]
fn particle_commands_only_control_visible_pages_and_never_commit_partial_state()
-> anyhow::Result<()> {
    let xml = r#"<ui schema="3" initial="hud"><variables><bool id="flag" initial="false"/></variables><screen id="hud" modal="false" pause="false"><particles id="rain" x="0" y="0" width="100" height="100" image="dot" rate="10"/><particles id="spark" x="0" y="0" width="100" height="100" image="dot" burst="8"/><button id="b" x="0" y="0" width="10" height="10" text="b" event="noop"/></screen><screen id="menu"><particles id="petals" x="0" y="0" width="100" height="100" image="dot" rate="10"/><button id="c" x="0" y="0" width="10" height="10" text="c" event="noop"/></screen></ui>"#;
    let (loaded, _) = load(xml, NOOP)?;
    let definition = &loaded.interface;
    let state = State::new(definition);
    let profile = Profile::new(&loaded.story.id, definition);
    let settings = Settings::default();
    let prepare = |id: &str, action: &str, entering: bool| {
        host::prepare(
            definition,
            &state,
            &settings,
            &profile,
            vec![Command::Particles {
                id: id.into(),
                action: action.into(),
            }],
            entering,
        )
    };
    let plan = prepare("rain", "start", false)?;
    assert_eq!(plan.particles, vec![("rain".to_owned(), Action::Start)]);
    assert!(matches!(plan.operation, host::Operation::None));
    prepare("spark", "burst", false)?;
    assert!(prepare("missing", "start", false).is_err());
    assert!(prepare("petals", "start", false).is_err(), "hidden page");
    assert!(
        prepare("rain", "burst", false).is_err(),
        "no burst declared"
    );
    assert!(
        prepare("spark", "start", false).is_err(),
        "no rate declared"
    );
    assert!(prepare("rain", "explode", false).is_err());
    assert!(
        prepare("rain", "start", true).is_err(),
        "game.enter is closed"
    );
    // Two actions for one emitter in the same batch are rejected as a whole.
    let batch = host::prepare(
        definition,
        &state,
        &settings,
        &profile,
        vec![
            Command::Particles {
                id: "rain".into(),
                action: "start".into(),
            },
            Command::Particles {
                id: "rain".into(),
                action: "stop".into(),
            },
        ],
        false,
    );
    let message = format!("{:#}", batch.err().expect("duplicate action"));
    assert!(message.contains("一次只能提交一个操作"), "{message}");
    Ok(())
}

#[test]
fn demo_ships_a_bounded_particle_page_and_a_master_volume_slider() -> anyhow::Result<()> {
    let source = DirectorySource::new(&Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo"))?;
    let (loaded, _) = loader::load_game(&source)?;
    let definition = &loaded.interface;
    let ids = ["rainFall", "snowFall", "petals", "sparkle"];
    assert_eq!(definition.particles.len(), ids.len());
    for id in ids {
        let emitter = &definition.particles[id];
        assert_eq!(emitter.screen, "particles");
        assert!(emitter.max <= kakera_avg::particles::MAX_PER_EMITTER);
        assert!(emitter.rate > 0.0 || emitter.burst > 0);
    }
    assert!(definition.screens.contains_key("particles"));
    for event in [
        "particles-start",
        "particles-stop",
        "particles-clear",
        "particles-burst",
    ] {
        assert!(definition.host_handlers.contains(event), "{event}");
    }
    let slider = find(&definition.screens["settings"].nodes, "masterVolume").expect("slider");
    assert_eq!(slider.bind.as_deref(), Some("settings.master_volume"));
    Ok(())
}

#[test]
fn particle_image_must_be_a_declared_picture_asset() {
    // `dot` is declared as an image, so only a missing or wrong kind is rejected.
    let (loaded, _) = load(&ui(&particles(r#"rate="10""#)), NOOP).expect("valid emitter");
    assert!(loaded.interface.images.contains("dot"));
    let font = r#"<particles id="p" x="0" y="0" width="100" height="100" image="font" rate="10"/>"#;
    assert!(error(&ui(font), NOOP).contains("UI 图片引用"));
}

#[test]
fn emitters_draw_one_textured_quad_per_live_particle_and_respect_visibility() -> anyhow::Result<()>
{
    use eframe::egui;
    use kakera_avg::{
        interface::{Data, data, host::UiValue, render},
        particles::System,
        runtime::{Bundle, LoadOptions},
    };
    let root = tempfile::tempdir()?;
    let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets");
    std::fs::copy(
        demo.join("ui/particle-snow.png"),
        root.path().join("dot.png"),
    )?;
    std::fs::copy(demo.join("fonts/chinese.otf"), root.path().join("font.otf"))?;
    std::fs::create_dir_all(root.path().join("scripts"))?;
    std::fs::write(root.path().join("story.xml"), STORY)?;
    std::fs::write(
        root.path().join("ui.xml"),
        r#"<ui schema="3" initial="hud"><variables><bool id="show" initial="true"/></variables><screen id="hud" modal="false" pause="false"><scene id="scene" width="1280" height="720"/><particles id="p" x="0" y="0" width="400" height="300" image="dot" rate="200" max="32" life="4" size="10" fade="false" visible="ui.show" autostart="true"/></screen></ui>"#,
    )?;
    std::fs::write(root.path().join("scripts/ui.js"), NOOP)?;
    let bundle = Bundle::load(
        &DirectorySource::new(root.path())?,
        &LoadOptions {
            core: None,
            no_live2d: true,
        },
    )?;
    let definition = bundle.loaded.interface;
    let story = bundle.loaded.story;
    let ctx = egui::Context::default();
    let cache = bundle.assets.images.install(&ctx);
    let texture = cache.image("dot").expect("particle texture").id();
    let state = State::new(&definition);
    let profile = Profile::new("v0113", &definition);
    let settings = Settings::default();
    let effects = kakera_avg::effects::Player::default();
    let mut system = System::seeded(definition.particles.clone(), 0x5EED);
    system.sync(&state.stack);
    system.tick(0.1);
    let expected = system.particles("p").map(<[_]>::len).unwrap_or(0);
    assert_eq!(expected, 20, "200/s over a 0.1 s step spawns 20");
    let session = story.start();
    let draw = |state: &State, system: &System| {
        let data = Data::build(data::Input {
            definition: &definition,
            story: &story,
            session: &session,
            available: &[],
            state,
            profile: &profile,
            slots: &[],
            settings: &settings,
            complete: true,
            slot_pages: &[],
            autosaves: &[],
        });
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 720.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    render::draw(
                        ui,
                        render::Input {
                            pickers: &mut Default::default(),
                            effects: &effects,
                            definition: &definition,
                            state,
                            data: &data,
                            frame: story.frame(&session).unwrap(),
                            assets: &cache,
                            particles: system,
                            visible_text: "",
                            blocked: false,
                        },
                    );
                });
            },
        );
        let meshes = ctx.tessellate(output.shapes, output.pixels_per_point);
        meshes
            .iter()
            .filter(|primitive| {
                matches!(&primitive.primitive, egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == texture)
            })
            .map(|primitive| match &primitive.primitive {
                egui::epaint::Primitive::Mesh(mesh) => (mesh.vertices.len(), mesh.indices.len()),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>()
    };
    let _ = draw(&state, &system);
    // One mesh per emitter, four vertices and six indices per live particle.
    let quads = draw(&state, &system);
    assert_eq!(quads, vec![(expected * 4, expected * 6)]);
    let hidden = host::prepare(
        &definition,
        &state,
        &settings,
        &profile,
        vec![Command::SetUi {
            name: "show".into(),
            value: UiValue::Bool(false),
        }],
        false,
    )?;
    assert!(
        draw(&hidden.state, &system).is_empty(),
        "visible gates drawing"
    );
    // The texture is shared with ordinary image nodes; no extra upload happens.
    let again = draw(&state, &system);
    assert_eq!(again, quads);
    Ok(())
}
