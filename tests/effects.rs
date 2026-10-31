use kakera_avg::{
    assets::{PreparedAssets, filter_image},
    content::{ContentSource, DirectorySource},
    effects::{self, Player},
    interface::{
        self, Event,
        host::{self, Profile, State},
    },
    loader::load_game,
    model::{Logic, effects::*},
    settings::Settings,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
mod common;

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
});
const JS: &str = include_str!("../games/demo/scripts/ui.js");
struct Source(BTreeMap<PathBuf, Vec<u8>>);
impl ContentSource for Source {
    fn read(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
        if let Some(v) = self.0.get(path) {
            return Ok(v.clone());
        }
        DirectorySource::new(&Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo"))?.read(path)
    }
}
fn source(story: &str, ui: &str, js: &str) -> Source {
    Source(
        [
            ("story.xml", story),
            ("ui.xml", ui),
            ("scripts/ui.js", js),
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
        .map(|(p, s)| (PathBuf::from(p), s.as_bytes().to_vec()))
        .collect(),
    )
}

#[test]
fn all_transition_endpoints_and_directions_are_bounded() {
    use TransitionKind::*;
    for kind in [
        None, Dissolve, FadeBlack, FadeWhite, WipeLeft, WipeRight, WipeUp, WipeDown, SlideLeft,
        SlideRight, SlideUp, SlideDown,
    ] {
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let b = effects::blend(kind, t);
            for l in &b.layers {
                assert!(l.clip.iter().all(|v| (0.0..=1.0).contains(v)));
                assert!(l.offset.iter().all(|v| (-1.0..=1.0).contains(v)));
                assert!((0.0..=1.0).contains(&l.opacity));
            }
            if t == 1.0 {
                assert_eq!(b.layers.len(), 1);
                assert!(!b.layers[0].old);
                assert_eq!(b.layers[0].opacity, 1.0);
                assert_eq!(b.layers[0].offset, [0.0; 2]);
                assert!(b.veil.is_none());
            }
        }
    }
    assert_eq!(effects::blend(FadeBlack, 0.5).veil, Some(([0; 3], 1.0)));
    assert_eq!(effects::blend(FadeWhite, 0.5).veil, Some(([255; 3], 1.0)));
    assert_eq!(
        effects::blend(WipeLeft, 0.25).layers[1].clip,
        [0.75, 0.0, 1.0, 1.0]
    );
    assert_eq!(
        effects::blend(SlideRight, 0.25).layers[1].offset,
        [-0.75, 0.0]
    );
}
#[test]
fn animations_settle_and_easing_is_monotonic() {
    for e in [
        Easing::Linear,
        Easing::EaseIn,
        Easing::EaseOut,
        Easing::EaseInOut,
    ] {
        let values: Vec<_> = (0..=100)
            .map(|i| effects::ease(i as f32 / 100.0, e))
            .collect();
        assert_eq!(values[0], 0.0);
        assert_eq!(values[100], 1.0);
        assert!(values.windows(2).all(|w| w[0] <= w[1]));
    }
    for kind in [
        AnimationKind::None,
        AnimationKind::FadeIn,
        AnimationKind::ZoomIn,
        AnimationKind::ZoomOut,
        AnimationKind::Shake,
        AnimationKind::Flash,
        AnimationKind::SlideLeft,
        AnimationKind::SlideRight,
        AnimationKind::SlideUp,
        AnimationKind::SlideDown,
    ] {
        let a = Animation {
            kind,
            duration: 500,
            ..Default::default()
        };
        assert_eq!(effects::motion(a, 0.5), Default::default());
        assert_eq!(effects::motion(a, 100.0), Default::default());
    }
    assert_eq!(effects::progress(0.0, 0), 1.0);
}
#[test]
fn clock_pause_skip_restore_and_self_loop_are_transient() {
    let (loaded, _) = load_game(&source(&STORY, &UI, JS)).unwrap();
    let frame = loaded.story.frames["opening"].clone();
    let transition = Transition {
        kind: TransitionKind::Dissolve,
        duration: 1000,
        easing: Easing::Linear,
    };
    let mut p = Player::default();
    assert!(!p.has_entered);
    p.enter(Some(frame.clone()), transition, false);
    assert!(p.has_entered);
    assert!(p.active());
    p.tick(0.2);
    let age = p.age;
    p.tick(0.0);
    assert_eq!(p.age, age);
    p.finish();
    assert!(!p.active());
    p.enter(Some(frame.clone()), transition, false);
    assert!(p.active());
    p.animate(
        "scene".into(),
        Animation {
            kind: AnimationKind::Shake,
            ..Default::default()
        },
    );
    p.enter(Some(frame), transition, true);
    assert!(!p.active());
    assert_eq!(p.ui_age, 6.0);
    assert!(p.animations.is_empty());
}
#[test]
fn rgba_filters_preserve_transparency_and_produce_expected_pixels() {
    let red = image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 128]));
    let gray = filter_image(&red, Filter::Grayscale);
    assert_eq!(gray.get_pixel(0, 0).0, [76, 76, 76, 128]);
    let sepia = filter_image(&red, Filter::Sepia);
    let p = sepia.get_pixel(0, 0);
    assert!(p[0] > p[1] && p[1] > p[2]);
    assert_eq!(p[3], 128);
    for filter in [Filter::None, Filter::Blur(4), Filter::Pixelate(3)] {
        let out = filter_image(&red, filter);
        assert_eq!(out.dimensions(), (4, 4));
        assert_eq!(out.get_pixel(2, 2)[3], 128);
    }
    let transparent = image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 0]));
    assert!(
        filter_image(&transparent, Filter::Blur(4))
            .pixels()
            .all(|p| p.0 == [0; 4])
    );
    let mut checker = image::RgbaImage::new(8, 8);
    for (x, y, p) in checker.enumerate_pixels_mut() {
        *p = image::Rgba([if (x + y) % 2 == 0 { 255 } else { 0 }, 0, 0, 255]);
    }
    assert_ne!(filter_image(&checker, Filter::Pixelate(4)), checker);
}
#[test]
fn effect_xml_is_strict_and_reference_errors_have_positions() {
    for (from, to) in [
        ("kind=\"dissolve\"", "kind=\"missing\""),
        ("duration=\"700\"", "duration=\"5001\""),
        ("blur=\"4\"", "blur=\"NaN\""),
        ("pixel-size=\"24\"", "pixel-size=\"0\""),
        ("effect=\"portrait.enter\"", "effect=\"absent\""),
        ("filter=\"sepia\"", "filter=\"sepia\" opacity=\"2\""),
        ("filter=\"sepia\"", "filter=\"sepia\" blur=\"4\""),
        ("filter=\"sepia\"", "filter=\"sepia\" easing=\"invalid\""),
        ("filter=\"sepia\"", "filter=\"sepia\" rotation=\"361\""),
        ("filter=\"sepia\"", "filter=\"sepia\" flip-x=\"yes\""),
        ("filter=\"sepia\"", "filter=\"sepia\" tint=\"#GGGGGG\""),
        ("filter=\"sepia\"", "filter=\"sepia\" scale=\"0\""),
        ("id=\"fx.gray\"", "id=\"fx.sepia\""),
    ] {
        assert!(STORY.contains(from));
        let error = load_game(&source(&STORY.replace(from, to), &UI, JS))
            .err()
            .unwrap();
        assert!(format!("{error:#}").contains("story.xml:"), "{error:#}");
    }
    let ui = UI.replace("effect=\"fx.gray\"", "effect=\"missing\"");
    assert!(
        format!("{:#}", load_game(&source(&STORY, &ui, JS)).err().unwrap()).contains("ui.xml:")
    );
}
#[test]
fn js_effect_commands_are_validated_atomically_and_transition_is_lifecycle_only() {
    let js = format!(
        "{JS}\n(event.register('test', ctx=>{{ctx.setUi('status','changed');ctx.animate('missing','shake');}}), ui('test').bind('test'));(event.register('bad-transition', ctx=>ctx.transition('dissolve')), ui('bad-transition').bind('bad-transition'));(event.register('good', ctx=>ctx.animate('scene','shake',600,20)), ui('good').bind('good'));(event.register('repeat-target', ctx=>ctx.animate('cgThumbnail','fade-in')), ui('repeat-target').bind('repeat-target')); "
    );
    let ui = common::probe(&UI, &["test", "bad-transition", "repeat-target", "good"]);
    let (loaded, logic) = load_game(&source(&STORY, &ui, &js)).unwrap();
    let def = loaded.interface;
    let state = State::new(&def);
    let profile = Profile::new(&loaded.story.id, &def);
    let settings = Settings::default();
    for name in ["test", "bad-transition", "repeat-target", "good"] {
        let commands = logic
            .ui_event(
                &Event::ui(name, name),
                &loaded.story.start(),
                &state.variables,
                &json!({"game":{},"settings":{}}),
            )
            .unwrap();
        let result = host::prepare(&def, &state, &settings, &profile, commands, false);
        if name == "good" {
            assert_eq!(result.unwrap().animations.len(), 1);
        } else {
            assert!(result.is_err());
        }
        assert_eq!(state.variables["status"].to_string(), "");
    }
    for body in [
        "ctx.transition('bad')",
        "ctx.transition('dissolve',-1)",
        "ctx.animate('scene','shake',500,999)",
        "ctx.transition('dissolve');ctx.transition('fade-black')",
    ] {
        let js = JS.replace(
            "if (ctx.ui.fxTransition) ctx.transition(ctx.ui.fxTransition, Number(ctx.ui.fxDuration));",
            body,
        );
        assert!(load_game(&source(&STORY, &UI, &js)).is_err(), "{body}");
    }
}
#[test]
fn effect_preparation_and_mesh_rendering_use_cached_assets_and_fixed_clip() {
    let source = source(&STORY, &UI, JS);
    let (loaded, logic) = load_game(&source).unwrap();
    let context = eframe::egui::Context::default();
    let assets = PreparedAssets::load_from(&loaded.story, &source)
        .unwrap()
        .install(&context);
    for f in [
        Filter::None,
        Filter::Grayscale,
        Filter::Sepia,
        Filter::Blur(4),
        Filter::Pixelate(24),
    ] {
        assert!(assets.effected("bg.room", f).is_some());
    }
    let session = loaded.story.start();
    let def = loaded.interface;
    let state = State::new(&def);
    let profile = Profile::new(&loaded.story.id, &def);
    let settings = Settings::default();
    let data = interface::Data::build(interface::data::Input {
        definition: &def,
        story: &loaded.story,
        session: &session,
        available: &logic.options(&loaded.story, &session).unwrap(),
        state: &state,
        profile: &profile,
        slots: &[],
        settings: &settings,
        complete: false,
    });
    let mut player = Player::default();
    player.enter(
        Some(loaded.story.frames["question"].clone()),
        Transition {
            kind: TransitionKind::SlideLeft,
            duration: 1000,
            easing: Easing::Linear,
        },
        false,
    );
    player.tick(0.5);
    use eframe::egui;
    let out = context.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 720.0),
            )),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    assert!(
                        interface::render::draw(
                            ui,
                            interface::render::Input {
                                pickers: &mut Default::default(),
                                effects: &player,
                                definition: &def,
                                state: &state,
                                data: &data,
                                frame: &loaded.story.frames["opening"],
                                assets: &assets,
                                visible_text: "",
                                blocked: false
                            }
                        )
                        .is_none()
                    );
                });
        },
    );
    assert!(!out.shapes.is_empty());
    assert!(
        out.shapes
            .iter()
            .all(|s| s.clip_rect.min.x >= 0.0 && s.clip_rect.max.x <= 1280.0)
    );
}
