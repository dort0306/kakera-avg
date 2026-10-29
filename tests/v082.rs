use anyhow::Result;
use image::{Rgba, RgbaImage};
use kakera_avg::{
    content::DirectorySource,
    controller::{self, Intent},
    image_layout::{NinePatch, Region},
    interface::{
        self,
        host::{self, Command, Profile, State, UiValue},
    },
    loader,
    package::{self, Archive, Snapshot, StagedProject},
    runtime::{Bundle, LoadOptions},
    settings::Settings,
};
use std::path::Path;

fn patch_image() -> RgbaImage {
    let mut image = RgbaImage::new(12, 10);
    for y in 1..9 {
        for x in 1..11 {
            image[(x, y)] = Rgba([20, 40, 80, 255]);
        }
    }
    for x in 4..8 {
        image[(x, 0)] = Rgba([0, 0, 0, 255]);
    }
    for y in 3..7 {
        image[(0, y)] = Rgba([0, 0, 0, 255]);
    }
    for x in 3..9 {
        image[(x, 9)] = Rgba([0, 0, 0, 255]);
    }
    for y in 2..8 {
        image[(11, y)] = Rgba([0, 0, 0, 255]);
    }
    image[(1, 1)] = Rgba([240, 20, 30, 255]);
    image
}
#[test]
fn nine_patch_removes_markers_preserves_corners_and_maps_padding_at_density() -> Result<()> {
    let (patch, image) = NinePatch::parse(patch_image())?;
    assert_eq!(image.dimensions(), (10, 8));
    assert_eq!(image[(0, 0)].0, [240, 20, 30, 255]);
    let (mesh, content) = patch.layout([100.0, 50.0], 1.0);
    assert_eq!(mesh.len(), 9);
    assert_eq!(mesh[0].target, [0.0, 0.0, 3.0, 2.0]);
    assert_eq!(content, [2.0, 1.0, 98.0, 49.0]);
    let (mesh, content) = patch.layout([100.0, 50.0], 2.0);
    assert_eq!(mesh[0].target, [0.0, 0.0, 6.0, 4.0]);
    assert_eq!(content, [4.0, 2.0, 96.0, 48.0]);
    for size in [[2.0, 1.0], [640.0, 180.0], [3840.0, 540.0]] {
        let (mesh, content) = patch.layout(size, 1.0);
        let area: f32 = mesh
            .iter()
            .map(|p| (p.target[2] - p.target[0]) * (p.target[3] - p.target[1]))
            .sum();
        assert!((area - size[0] * size[1]).abs() < 1.0);
        assert!(
            content[0] >= 0.0
                && content[2] <= size[0]
                && content[1] >= 0.0
                && content[3] <= size[1]
        );
    }
    Ok(())
}
#[test]
fn nine_patch_handles_multiple_stretches_and_rejects_malformed_borders() -> Result<()> {
    let mut image = patch_image();
    for x in 1..11 {
        image[(x, 0)] = Rgba([0; 4]);
    }
    for x in [2, 3, 7, 8, 9] {
        image[(x, 0)] = Rgba([0, 0, 0, 255]);
    }
    let (patch, _) = NinePatch::parse(image)?;
    assert_eq!(patch.horizontal, [[1, 3], [6, 9]]);
    let (mesh, _) = patch.layout([100.0, 40.0], 1.0);
    let widths: Vec<_> = mesh
        .iter()
        .filter(|p| p.source[1] == 0.0 && (p.source[0] == 1.0 || p.source[0] == 6.0))
        .map(|p| p.target[2] - p.target[0])
        .collect();
    assert!((widths[0] / widths[1] - 2.0 / 3.0).abs() < 0.00001);
    for (x, y, pixel) in [
        (0, 0, [0, 0, 0, 255]),
        (3, 0, [10, 10, 10, 255]),
        (11, 5, [0, 0, 0, 128]),
        (1, 9, [0, 0, 0, 255]),
    ] {
        let mut image = patch_image();
        image[(x, y)] = Rgba(pixel);
        assert!(NinePatch::parse(image).is_err());
    }
    let mut image = patch_image();
    for x in 1..11 {
        image[(x, 0)] = Rgba([0; 4]);
    }
    assert!(NinePatch::parse(image).is_err());
    assert!(NinePatch::parse(RgbaImage::new(2, 2)).is_err());
    Ok(())
}
#[test]
fn region_is_bounded_pixel_geometry_with_texel_safe_uvs() -> Result<()> {
    let region = Region::parse("32, 0, 32, 32")?;
    region.validate([64, 32])?;
    assert_eq!(
        region.uv([64.0, 32.0]),
        [32.5 / 64.0, 0.5 / 32.0, 63.5 / 64.0, 31.5 / 32.0]
    );
    for text in [
        "",
        "0,0,0,1",
        "-1,0,1,1",
        "0,0,1.5,1",
        "1,2,3",
        "1,2,3,4,5",
        "4294967295,0,1,1",
    ] {
        assert!(Region::parse(text).is_err(), "{text}");
    }
    assert!(Region::parse("63,0,2,8")?.validate([64, 32]).is_err());
    Ok(())
}

const UI: &str = r#"<ui schema="3" initial="hud"><variables><string id="crop" initial="0,0,32,32"/><string id="sheet" initial="atlas"/></variables><screen id="hud" modal="false" pause="false"><nine-patch id="bubble" image="bubble" width="300" height="90"><label id="text" width="250" height="30" text="点九图"/></nine-patch><image id="sprite" x="320" width="64" height="64" image-bind="ui.sheet" region-bind="ui.crop"/><image id="second" x="400" width="64" height="64" image="atlas" region="32,0,32,32"/></screen></ui>"#;
fn game(root: &Path) -> Result<()> {
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/fonts/chinese.otf"),
        root.join("font.otf"),
    )?;
    let mut atlas = RgbaImage::new(64, 32);
    for (x, _, p) in atlas.enumerate_pixels_mut() {
        *p = Rgba(if x < 32 {
            [220, 40, 30, 255]
        } else {
            [30, 180, 210, 255]
        });
    }
    atlas.save(root.join("atlas.png"))?;
    RgbaImage::new(16, 16).save(root.join("tiny.png"))?;
    patch_image().save(root.join("bubble.9.png"))?;
    std::fs::write(
        root.join("story.xml"),
        r#"<story schema="8" id="v082" content-version="8" entry="opening"><scripts><script src="story.js"/></scripts><interface src="ui.xml"/><assets><font id="font" src="font.otf"/><image id="atlas" src="atlas.png"/><image id="tiny" src="tiny.png"/><nine-patch id="bubble" src="bubble.9.png"/></assets><characters/><visuals/><variables><int id="count" initial="0" min="0" max="2"/></variables><frames><frame name="opening"><text>开始</text></frame><frame name="question"><text>选择</text><options><option id="again">循环</option><option id="end">结束</option></options></frame><frame id="fixed" name="finish"><text>完成</text></frame></frames><next end="true"/></story>"#,
    )?;
    std::fs::write(root.join("ui.xml"), UI)?;
    std::fs::write(
        root.join("story.js"),
        r#"on.advance('opening',ctx=>{if(ctx.frameId!=='opening')throw Error('alias');return 'question';});on.choice('question','again',ctx=>{ctx.add('count',1);return 'question';},ctx=>ctx.vars.count<2n);on.choice('question','end',()=> 'finish');on.end('finish');on.ui('swap',ctx=>ctx.setUi('crop','32,0,32,32'));"#,
    )?;
    Ok(())
}
fn options() -> LoadOptions {
    LoadOptions {
        core: None,
        no_live2d: true,
    }
}
#[test]
fn nine_patch_is_excluded_from_eager_and_deferred_filter_caches() -> Result<()> {
    use kakera_avg::{
        assets::PreparedAssets,
        model::effects::{Filter, ImageEffect},
    };
    let root = tempfile::tempdir()?;
    game(root.path())?;
    let source = DirectorySource::new(root.path())?;
    let mut story = loader::load_game(&source)?.0.story;
    story.effects.insert(
        "gray".into(),
        ImageEffect {
            filter: Filter::Grayscale,
            ..Default::default()
        },
    );
    for deferred in [false, true] {
        let assets = if deferred {
            PreparedAssets::load_deferred(&story, &source)?
        } else {
            PreparedAssets::load_from(&story, &source)?
        };
        let cache = assets.install(&eframe::egui::Context::default());
        assert!(cache.image("bubble").is_some());
        assert!(cache.effected("bubble", Filter::Grayscale).is_none());
        assert!(cache.effected("atlas", Filter::Grayscale).is_some());
    }
    Ok(())
}
fn setting(name: &str, value: &str) -> Command {
    Command::SetUi {
        name: name.into(),
        value: UiValue::String(value.into()),
    }
}
#[test]
fn js_bound_regions_are_cached_and_entire_ui_plans_roll_back() -> Result<()> {
    let root = tempfile::tempdir()?;
    game(root.path())?;
    let bundle = Bundle::load(&DirectorySource::new(root.path())?, &options())?;
    let def = &bundle.loaded.interface;
    let state = State::new(def);
    let profile = Profile::new("v082", def);
    let settings = Settings::default();
    assert_eq!(state.regions["sprite"], Region([0, 0, 32, 32]));
    let commands = bundle.logic.ui_event(
        &interface::Event {
            sound: None,
            route: None,
            name: "swap".into(),
            item: serde_json::Value::Null,
            value: serde_json::Value::Null,
        },
        &bundle.loaded.story.start(),
        &state.variables,
        &serde_json::json!({"game":{}}),
    )?;
    let plan = host::prepare(def, &state, &settings, &profile, commands, false)?;
    assert_eq!(plan.state.regions["sprite"], Region([32, 0, 32, 32]));
    for commands in [
        vec![
            setting("crop", "63,0,2,8"),
            Command::Save {
                slot: 1,
                overwrite: false,
            },
        ],
        vec![setting("sheet", "tiny")],
        vec![setting("crop", "0,0,0,0")],
    ] {
        let error = host::prepare(def, &state, &settings, &profile, commands, false)
            .err()
            .expect("invalid region must fail")
            .to_string();
        assert!(
            error.contains("ui.xml:") && error.contains("sprite"),
            "{error}"
        );
        assert_eq!(state.regions["sprite"], Region([0, 0, 32, 32]));
    }
    Ok(())
}
#[test]
fn xml_rejects_region_and_nine_patch_type_mismatches_with_positions() -> Result<()> {
    let root = tempfile::tempdir()?;
    game(root.path())?;
    for ui in [
        UI.replace(
            "<nine-patch id=\"bubble\" image=\"bubble\"",
            "<nine-patch id=\"bubble\" image=\"atlas\"",
        ),
        UI.replace("image=\"atlas\" region=", "image=\"bubble\" region="),
        UI.replace(
            "region-bind=\"ui.crop\"",
            "region-bind=\"ui.crop\" region=\"0,0,1,1\"",
        ),
        UI.replace("density-missing", "unused")
            .replace("width=\"300\"", "region=\"0,0,1,1\" width=\"300\""),
        UI.replace(
            "region-bind=\"ui.crop\"",
            "region-bind=\"settings.fullscreen\"",
        ),
    ] {
        std::fs::write(root.path().join("ui.xml"), ui)?;
        let error = format!(
            "{:#}",
            loader::load_game(&DirectorySource::new(root.path())?)
                .err()
                .expect("must reject")
        );
        assert!(error.contains("ui.xml:"), "{error}");
    }
    std::fs::write(
        root.path().join("ui.xml"),
        UI.replace("region=\"32,0,32,32\"", "region=\"32,0,33,32\""),
    )?;
    assert!(Bundle::load(&DirectorySource::new(root.path())?, &options()).is_err());
    Ok(())
}

#[test]
fn regions_share_one_texture_and_switch_uv_without_uploading_images() -> Result<()> {
    let root = tempfile::tempdir()?;
    game(root.path())?;
    let bundle = Bundle::load(&DirectorySource::new(root.path())?, &options())?;
    let def = bundle.loaded.interface;
    let story = bundle.loaded.story;
    let ctx = eframe::egui::Context::default();
    let cache = bundle.assets.images.install(&ctx);
    let texture = cache.image("atlas").unwrap().id();
    let mut state = State::new(&def);
    let profile = Profile::new("v082", &def);
    let settings = Settings::default();
    let effects = kakera_avg::effects::Player::default();
    let session = story.start();
    let draw = |state: &State| {
        let data = interface::Data::build(interface::data::Input {
            definition: &def,
            story: &story,
            session: &session,
            available: &[],
            state,
            profile: &profile,
            slots: &[],
            settings: &settings,
            complete: true,
        });
        ctx.run(
            eframe::egui::RawInput {
                screen_rect: Some(eframe::egui::Rect::from_min_size(
                    eframe::egui::Pos2::ZERO,
                    eframe::egui::vec2(1280.0, 720.0),
                )),
                ..Default::default()
            },
            |ctx| {
                eframe::egui::CentralPanel::default().show(ctx, |ui| {
                    interface::render::draw(
                        ui,
                        interface::render::Input {
                            effects: &effects,
                            definition: &def,
                            state,
                            data: &data,
                            frame: story.frame(&session).unwrap(),
                            assets: &cache,
                            visible_text: "",
                            blocked: false,
                        },
                    );
                });
            },
        )
    };
    let _ = draw(&state);
    let _ = draw(&state);
    state = host::prepare(
        &def,
        &state,
        &settings,
        &profile,
        vec![setting("crop", "32,0,32,32")],
        false,
    )?
    .state;
    let output = draw(&state);
    assert!(output.textures_delta.set.is_empty());
    let meshes = ctx.tessellate(output.shapes, output.pixels_per_point);
    let sprites: Vec<_> = meshes
        .iter()
        .filter_map(|p| match &p.primitive {
            eframe::egui::epaint::Primitive::Mesh(m) if m.texture_id == texture => Some(m),
            _ => None,
        })
        .collect();
    assert!(!sprites.is_empty());
    assert!(
        sprites
            .iter()
            .flat_map(|m| &m.vertices)
            .all(|v| v.uv.x >= 32.5 / 64.0 && v.uv.x <= 63.5 / 64.0)
    );
    assert_eq!(cache.image("atlas").unwrap().id(), texture);
    Ok(())
}

#[test]
fn sprite_fit_rotation_and_mirror_keep_the_selected_uv_range() -> Result<()> {
    use eframe::egui::{self, Pos2, Rect};
    let root = tempfile::tempdir()?;
    game(root.path())?;
    let bundle = Bundle::load(&DirectorySource::new(root.path())?, &options())?;
    let ctx = egui::Context::default();
    let cache = bundle.assets.images.install(&ctx);
    let effect = kakera_avg::model::effects::ImageEffect {
        flip_x: true,
        rotation: 90.0,
        ..Default::default()
    };
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(128.0, 128.0))),
            ..Default::default()
        },
        |ctx| {
            kakera_avg::effects::render::image(
                &ctx.layer_painter(egui::LayerId::background()),
                &cache,
                kakera_avg::effects::render::Image {
                    region: Some(Region([32, 0, 32, 16])),
                    id: "atlas",
                    bounds: Rect::from_min_size(Pos2::ZERO, egui::vec2(128.0, 128.0)),
                    fit: "contain",
                    tint: egui::Color32::WHITE,
                    effect: &effect,
                    motion: Default::default(),
                    opacity: 1.0,
                },
            );
        },
    );
    let mesh = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Mesh(mesh) => Some(mesh),
            _ => None,
        })
        .expect("sprite mesh");
    assert!((mesh.vertices[0].pos.x - 96.0).abs() < 0.001 && mesh.vertices[0].pos.y.abs() < 0.001);
    assert_eq!(mesh.vertices[0].uv, Pos2::new(63.5 / 64.0, 0.5 / 32.0));
    assert!(
        mesh.vertices
            .iter()
            .all(|v| v.uv.x >= 32.5 / 64.0 && v.uv.x <= 63.5 / 64.0 && v.uv.y <= 15.5 / 32.0)
    );
    Ok(())
}

#[test]
fn publication_generates_uuid_v4_and_preserves_named_routes_history_and_saves() -> Result<()> {
    let root = tempfile::tempdir()?;
    let out = tempfile::tempdir()?;
    game(root.path())?;
    let path = root.path().join("story.xml");
    std::fs::write(
        &path,
        std::fs::read_to_string(&path)?.replace("entry=\"opening\"", "entry=\"opening finish\""),
    )?;
    let original = std::fs::read(root.path().join("story.xml"))?;
    let project = loader::load_game(&DirectorySource::new(root.path())?)?;
    assert_eq!(project.0.story.entry, "opening");
    let mut ids = Vec::new();
    for version in [1, 2] {
        let path = out.path().join(format!("game-{version}.kpak"));
        if version == 1 {
            let (snapshot, _) = Snapshot::directory(root.path())?.for_publication()?;
            package::write(&path, &snapshot.encode(1)?, false)?;
        } else {
            let mut stage = StagedProject::capture(root.path())?;
            stage.for_publication()?;
            package::write_streaming(&stage, &path, 1, false, |a| {
                Bundle::load(a, &options())?;
                Ok(())
            })?;
        }
        let archive = Archive::open(&path)?;
        let bundle = Bundle::load(&archive, &options())?;
        let story = &bundle.loaded.story;
        let logic = &bundle.logic;
        assert_eq!(story.entries.len(), 2);
        assert_eq!(story.start_at("finish").unwrap().frame_id, "fixed");
        let id = story.resolve_frame("opening").unwrap();
        let pieces: Vec<_> = id.split('-').collect();
        assert_eq!(
            pieces.iter().map(|p| p.len()).collect::<Vec<_>>(),
            [8, 4, 4, 4, 12]
        );
        assert_eq!(&pieces[2][..1], "4");
        assert!("89ab".contains(&pieces[3][..1]));
        ids.push(id.to_owned());
        assert_eq!(story.resolve_frame("finish"), Some("fixed"));
        let mut session =
            controller::transition_with(story, &story.start(), Intent::Advance, logic)?.session;
        assert_eq!(session.frame_id, story.resolve_frame("question").unwrap());
        for _ in 0..2 {
            let result = controller::transition_with(
                story,
                &session,
                Intent::Choose("again".into()),
                logic,
            )?;
            assert!(result.entered);
            session = result.session;
        }
        let old = session.clone();
        assert!(
            controller::transition_with(story, &session, Intent::Choose("again".into()), logic)
                .is_err()
        );
        assert_eq!(session, old);
        let save = kakera_avg::save::Snapshot::capture_with(story, &session, logic)?;
        let reloaded = loader::load_game(&archive)?;
        assert_eq!(save.restore_with(&reloaded.0.story, &reloaded.1)?, session);
        let done =
            controller::transition_with(story, &session, Intent::Choose("end".into()), logic)?
                .session;
        assert_eq!(done.frame_id, "fixed");
        assert!(
            kakera_avg::save::Snapshot::capture_with(
                &project.0.story,
                &project.0.story.start(),
                &project.1
            )?
            .restore_with(story, logic)
            .is_err()
        );
        assert_eq!(std::fs::read(root.path().join("story.xml"))?, original);
    }
    assert_ne!(ids[0], ids[1]);
    Ok(())
}
#[test]
fn anonymous_frame_aliases_are_strict_and_diagnostics_keep_source_position() -> Result<()> {
    let root = tempfile::tempdir()?;
    game(root.path())?;
    let original = std::fs::read_to_string(root.path().join("story.xml"))?;
    for xml in [
        original.replace("name=\"opening\"", ""),
        original.replace("name=\"opening\"", "id=\"\" name=\"opening\""),
        original.replace("name=\"question\"", "name=\"opening\""),
        original.replace("name=\"question\"", "name=\"fixed\""),
        original.replace("name=\"question\"", "name=\"bad name\""),
    ] {
        std::fs::write(root.path().join("story.xml"), xml)?;
        let error = format!(
            "{:#}",
            loader::load_game(&DirectorySource::new(root.path())?)
                .err()
                .expect("must reject")
        );
        assert!(error.contains("story.xml:"), "{error}");
    }
    Ok(())
}
