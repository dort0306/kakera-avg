//! Work package C: HSL square picker interaction, author composition and release echo.
use eframe::egui;
use kakera_avg::{
    assets::PreparedAssets,
    color::{Hsla, Rgba},
    content::DirectorySource,
    interface::{self, Data, data, host, render},
    settings::Settings,
};
use std::path::Path;
mod common;

const PICKER_AT: [f32; 2] = [100.0, 100.0];
const ALPHA_SIZE: [f32; 2] = [256.0, 282.0];
const PLAIN_SIZE: [f32; 2] = [256.0, 256.0];

struct Fixture {
    ctx: egui::Context,
    definition: interface::Definition,
    state: host::State,
    data: Data,
    frame: kakera_avg::model::Frame,
    effects: kakera_avg::effects::Player,
    assets: kakera_avg::assets::AssetCache,
    pickers: render::PickerState,
    viewport: egui::Rect,
    markers: Vec<egui::Pos2>,
    squares: Vec<egui::Rect>,
    shapes: Vec<egui::epaint::ClippedShape>,
}

fn fixture(ui: &str) -> Fixture {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo");
    let source = DirectorySource::new(&root).unwrap();
    let (loaded, _logic) = kakera_avg::loader::load_game(&source).unwrap();
    let handlers = common::declarations(ui);
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
        slot_pages: &[],
        autosaves: &[],
    });
    let assets = PreparedAssets::load_deferred(&loaded.story, &source).unwrap();
    let ctx = egui::Context::default();
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
        viewport: egui::Rect::NOTHING,
        markers: Vec::new(),
        squares: Vec::new(),
        shapes: Vec::new(),
    }
}

fn ui(alpha: bool) -> String {
    let [width, height] = if alpha { ALPHA_SIZE } else { PLAIN_SIZE };
    format!(
        r##"<ui schema="3" initial="hud">
  <variables><string id="color" initial="#FF0000FF"/></variables>
  <screen id="hud" modal="false" pause="false">
    <color-picker id="picker" x="{}" y="{}" width="{width}" height="{height}" value="ui.color" event="pick" alpha="{alpha}"/>
  </screen>
</ui>"##,
        PICKER_AT[0], PICKER_AT[1]
    )
}

fn frame(fixture: &mut Fixture, events: Vec<egui::Event>) -> Option<interface::Event> {
    let mut out = None;
    let output = fixture.ctx.run(
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
                fixture.viewport = ui.max_rect();
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
    fixture.markers = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Circle(circle)
                if circle.fill == egui::Color32::WHITE && circle.radius < 6.0 =>
            {
                Some(circle.center)
            }
            _ => None,
        })
        .collect();
    fixture.squares = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Mesh(mesh) if mesh.vertices.len() == 33 * 33 => Some(
                egui::Rect::from_two_pos(mesh.vertices.first()?.pos, mesh.vertices.last()?.pos),
            ),
            _ => None,
        })
        .collect();
    fixture.shapes = output.shapes;
    out
}

impl Fixture {
    fn stage(&self) -> (egui::Pos2, f32) {
        let scale = (self.viewport.width() / 1280.0).min(self.viewport.height() / 720.0);
        (
            self.viewport.center() - egui::vec2(640.0, 360.0) * scale,
            scale,
        )
    }
    fn node(&self, id: &str) -> &interface::Node {
        self.definition.screens["hud"]
            .nodes
            .iter()
            .find(|node| node.id == id)
            .unwrap()
    }
    fn node_rect(&self, id: &str) -> egui::Rect {
        let node = self.node(id);
        let (origin, scale) = self.stage();
        egui::Rect::from_min_size(
            origin + egui::vec2(node.rect[0], node.rect[1]) * scale,
            egui::vec2(node.rect[2], node.rect[3]) * scale,
        )
    }
    fn scale(&self) -> f32 {
        self.stage().1
    }
    fn square(&self) -> egui::Rect {
        self.square_at(0)
    }
    fn square_at(&self, index: usize) -> egui::Rect {
        *self
            .squares
            .get(index)
            .unwrap_or_else(|| panic!("missing rendered HSL grid {index}: {:?}", self.squares))
    }
    fn square_point(&self, index: usize, saturation: f32, lightness: f32) -> egui::Pos2 {
        let square = self.square_at(index);
        egui::pos2(
            square.left() + square.width() * saturation,
            square.bottom() - square.height() * lightness,
        )
    }
    fn ring_outer(&self, index: usize) -> f32 {
        let square = self.square_at(index);
        square.width() / (0.78 * std::f32::consts::SQRT_2)
    }
    fn ring_point(&self, index: usize, hue: f32) -> egui::Pos2 {
        let center = self.square_at(index).center();
        let (inner, outer) = (self.ring_outer(index) * 0.82, self.ring_outer(index));
        let point = kakera_avg::color::hue_point([center.x, center.y], (inner + outer) * 0.5, hue);
        egui::pos2(point[0], point[1])
    }
    /// Alpha bar centre for a picker laid out with `alpha=true`; the bar hangs
    /// below the disc with an 8 unit gap and is 18 units tall.
    fn alpha_point(&self, id: &str, index: usize, alpha: f32) -> egui::Pos2 {
        let bounds = self.node_rect(id);
        let scale = self.scale();
        let center = self.square_at(index).center();
        let top = center.y + self.ring_outer(index) + 14.0 * scale;
        let width = bounds.width() - 16.0 * scale;
        egui::pos2(
            bounds.left() + 8.0 * scale + width * alpha,
            top + 9.0 * scale,
        )
    }
    fn texts(&self) -> Vec<(egui::Pos2, String)> {
        self.shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some((text.pos, text.galley.text().to_owned())),
                _ => None,
            })
            .collect()
    }
    fn fills(&self) -> Vec<(egui::Rect, egui::Color32)> {
        self.shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill != egui::Color32::TRANSPARENT => {
                    Some((rect.rect, rect.fill))
                }
                _ => None,
            })
            .collect()
    }
    fn texts_inside(&self, id: &str) -> Vec<String> {
        let bounds = self.node_rect(id);
        self.texts()
            .into_iter()
            .filter(|(pos, _)| bounds.contains(*pos))
            .map(|(_, text)| text)
            .collect()
    }
    fn fills_inside(&self, id: &str) -> Vec<(egui::Rect, egui::Color32)> {
        let bounds = self.node_rect(id);
        self.fills()
            .into_iter()
            .filter(|(rect, _)| bounds.contains_rect(*rect))
            .collect()
    }
}

fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
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

fn key(key: egui::Key) -> Vec<egui::Event> {
    vec![
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        },
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        },
    ]
}

fn color_of(event: &interface::Event) -> Rgba {
    Rgba::parse(event.value.as_str().unwrap()).unwrap()
}

fn expected(saturation: f32, lightness: f32) -> Rgba {
    Hsla {
        h: 0.0,
        s: saturation,
        l: lightness,
        a: 1.0,
    }
    .to_rgba()
}

fn assert_rgba(actual: Rgba, expected: Rgba) {
    assert!(
        actual.r.abs_diff(expected.r) <= 1
            && actual.g.abs_diff(expected.g) <= 1
            && actual.b.abs_diff(expected.b) <= 1
            && actual.a.abs_diff(expected.a) <= 1,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn square_interior_and_release_position_survive_binding_echo() {
    let mut f = fixture(&ui(true));
    frame(&mut f, vec![]);
    assert_eq!(f.squares.len(), 1);
    let points = [[0.3, 0.7], [0.8, 0.4], [0.4, 0.6]];
    for (index, [s, l]) in points.into_iter().enumerate() {
        let point = f.square_point(0, s, l);
        let events = match index {
            0 => pointer(point, true),
            1 => vec![egui::Event::PointerMoved(point)],
            _ => pointer(point, false),
        };
        let event = frame(&mut f, events).expect("each new position must emit a candidate");
        assert_rgba(color_of(&event), expected(s, l));
        // Simulate on.ui accepting the candidate via ctx.setUi, then an idle frame.
        f.data.values["ui"]["color"] = event.value;
        assert!(frame(&mut f, vec![]).is_none());
    }
    // Keyboard edits must continue from the last accepted color after release.
    let accepted = Rgba::parse(f.data.values["ui"]["color"].as_str().unwrap()).unwrap();
    assert_rgba(accepted, expected(0.4, 0.6));
    let event = frame(&mut f, key(egui::Key::ArrowUp)).unwrap();
    assert_rgba(color_of(&event), expected(0.4, 0.61));
}

#[test]
fn square_entire_area_and_achromatic_edges_keep_the_marker_after_release() {
    let mut f = fixture(&ui(true));
    frame(&mut f, vec![]);
    let square = f.square();
    assert!((square.width() - square.height()).abs() < 0.001);
    // Full grid includes four corners, all edges, gray axis and interior points.
    for row in 0..=4 {
        for column in 0..=4 {
            let point = egui::pos2(
                egui::lerp(square.x_range(), column as f32 / 4.0),
                egui::lerp(square.y_range(), row as f32 / 4.0),
            );
            if let Some(event) = frame(&mut f, pointer(point, true)) {
                f.data.values["ui"]["color"] = event.value;
            }
            assert_eq!(
                f.pickers
                    .focus
                    .as_ref()
                    .unwrap_or_else(|| panic!(
                        "missing square focus at {point:?}, row={row}, column={column}"
                    ))
                    .area,
                render::PickerArea::Square
            );
            assert!(frame(&mut f, pointer(point, false)).is_none());
            assert!(frame(&mut f, vec![]).is_none());
            assert!(
                f.markers.iter().any(|marker| marker.distance(point) < 0.01),
                "marker moved away from {point:?} at row={row}, column={column}: {:?}",
                f.markers
            );
        }
    }
    // The active square gesture clamps outside, even when the pointer crosses the ring.
    let start = square.center();
    if let Some(event) = frame(&mut f, pointer(start, true)) {
        f.data.values["ui"]["color"] = event.value;
    }
    let outside = square.right_bottom() + egui::vec2(50.0, 50.0);
    let event = frame(&mut f, pointer(outside, false)).unwrap();
    assert_eq!(event.value, "#000000FF");
    f.data.values["ui"]["color"] = event.value;
    frame(&mut f, vec![]);
    assert!(
        f.markers
            .iter()
            .any(|marker| marker.distance(square.right_bottom()) < 0.01)
    );
}

fn tab(fixture: &mut Fixture, backwards: bool) {
    let mut events = key(egui::Key::Tab);
    for event in &mut events {
        if let egui::Event::Key { modifiers, .. } = event {
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
        frame(&mut f, vec![]);
        let point = f.square_point(0, 0.4, 0.6);
        frame(&mut f, pointer(point, true));
        frame(&mut f, pointer(point, false));
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
        .replace("</screen>", "<color-picker id=\"second\" x=\"500\" y=\"100\" width=\"256\" height=\"256\" value=\"ui.color\" event=\"pick\" alpha=\"false\"/><button id=\"after\" x=\"900\" width=\"80\" height=\"30\" text=\"后\" event=\"pick\"/></screen>");
    let mut f = fixture(&xml);
    frame(&mut f, vec![]);
    tab(&mut f, false);
    assert!(f.pickers.focus.is_none());
    assert_eq!(
        frame(&mut f, key(egui::Key::Enter)).unwrap().value,
        serde_json::Value::Null
    );
    for (node, area) in [
        ("picker", render::PickerArea::Ring),
        ("picker", render::PickerArea::Square),
        ("picker", render::PickerArea::Alpha),
        ("second", render::PickerArea::Ring),
        ("second", render::PickerArea::Square),
    ] {
        tab(&mut f, false);
        let focus = f.pickers.focus.as_ref().unwrap();
        assert_eq!((&*focus.node, focus.area), (node, area));
    }
    tab(&mut f, false);
    assert!(f.pickers.focus.is_none());
    assert_eq!(
        frame(&mut f, key(egui::Key::Enter)).unwrap().value,
        serde_json::Value::Null
    );
    tab(&mut f, true);
    assert_eq!(f.pickers.focus.as_ref().unwrap().node, "second");
    assert_eq!(
        f.pickers.focus.as_ref().unwrap().area,
        render::PickerArea::Square
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
fn square_ring_alpha_and_keyboard_emit_one_canonical_event() {
    let mut f = fixture(&ui(true));
    frame(&mut f, vec![]);
    // No built-in preview text exists any more; the ring/square/alpha row are the control.
    assert!(f.texts_inside("picker").is_empty(), "{:?}", f.texts());

    // Clicking the blank space inside the control edits nothing, starts no gesture and
    // does not steal the keyboard focus.
    let gap = f.node_rect("picker").left_top() + egui::vec2(10.0, 10.0) * f.scale();
    assert!(frame(&mut f, pointer(gap, true)).is_none());
    assert!(f.pickers.focus.is_none());
    assert!(frame(&mut f, pointer(gap, false)).is_none());

    // Square interior near the hue vertex (top of the square).
    let point = f.square_point(0, 0.2, 0.8);
    let event = frame(&mut f, pointer(point, true)).unwrap();
    let color = color_of(&event);
    assert_eq!(event.name(), "pick");
    assert_eq!(event.value.as_str().unwrap(), color.format());
    assert_ne!(event.value.as_str().unwrap(), "#FF0000FF");
    assert_eq!(
        f.pickers.focus.as_ref().unwrap().area,
        render::PickerArea::Square
    );
    assert!(frame(&mut f, pointer(point, false)).is_none());
    // Simulate on.ui accepting the candidate, as the real App does in the same update.
    f.data.values["ui"]["color"] = event.value;

    // Hue ring at 120 degrees.
    let ring = f.ring_point(0, 120.0);
    let event = frame(&mut f, pointer(ring, true)).unwrap();
    let ring_color = color_of(&event);
    assert!(event.value.as_str().unwrap().starts_with('#'));
    assert!(!event.value.as_str().unwrap().starts_with("#FF0000"));
    assert_eq!(
        f.pickers.focus.as_ref().unwrap().area,
        render::PickerArea::Ring
    );
    f.data.values["ui"]["color"] = event.value;
    frame(&mut f, pointer(ring, false));

    // The alpha bar edits only alpha.
    let bar = f.alpha_point("picker", 0, 0.5);
    let event = frame(&mut f, pointer(bar, true)).unwrap();
    let alpha_color = color_of(&event);
    assert_eq!(
        (alpha_color.r, alpha_color.g, alpha_color.b),
        (ring_color.r, ring_color.g, ring_color.b)
    );
    assert_ne!(alpha_color.a, 255);
    assert!(
        alpha_color.a > 100 && alpha_color.a < 160,
        "{alpha_color:?}"
    );
    assert_eq!(
        f.pickers.focus.as_ref().unwrap().area,
        render::PickerArea::Alpha
    );
    frame(&mut f, pointer(bar, false));

    // Return to a saturated colour so a one degree hue step survives quantisation.
    let saturated = f.square_point(0, 1.0, 0.5);
    let event = frame(&mut f, pointer(saturated, true)).unwrap();
    f.data.values["ui"]["color"] = event.value;
    frame(&mut f, pointer(saturated, false));

    // Refocus the ring on a different hue before testing keyboard steps.
    let ring = f.ring_point(0, 240.0);
    let event = frame(&mut f, pointer(ring, true)).unwrap();
    f.data.values["ui"]["color"] = event.value;
    frame(&mut f, pointer(ring, false));
    assert_eq!(
        f.pickers.focus.as_ref().unwrap().area,
        render::PickerArea::Ring
    );

    // Arrow keys edit the focused ring and Tab moves to the square and alpha row.
    let original = f.data.values["ui"]["color"].as_str().unwrap().to_owned();
    let event = frame(&mut f, key(egui::Key::ArrowRight)).unwrap();
    assert_ne!(event.value.as_str().unwrap(), original);
    frame(&mut f, key(egui::Key::Tab));
    assert_eq!(
        f.pickers.focus.as_ref().unwrap().area,
        render::PickerArea::Square
    );
    frame(&mut f, key(egui::Key::Tab));
    assert_eq!(
        f.pickers.focus.as_ref().unwrap().area,
        render::PickerArea::Alpha
    );
    frame(&mut f, key(egui::Key::Tab));
    assert!(f.pickers.focus.is_none());
}

#[test]
fn disabled_picker_and_alpha_false_skip_hidden_areas() {
    let disabled = ui(false).replace("event=\"pick\"", "event=\"pick\" enabled=\"false\"");
    let mut f = fixture(&disabled);
    frame(&mut f, vec![]);
    let point = f.square_point(0, 0.5, 0.5);
    assert!(frame(&mut f, pointer(point, true)).is_none());
    assert!(f.pickers.focus.is_none());
    assert!(frame(&mut f, pointer(point, false)).is_none());

    let mut plain = fixture(&ui(false));
    frame(&mut plain, vec![]);
    let picker = plain.node("picker");
    assert!(matches!(
        picker.kind,
        interface::Kind::ColorPicker { alpha: false }
    ));
    // alpha=false renders no bar, no checkerboard and no reserved preview row.
    assert!(
        plain.fills_inside("picker").is_empty(),
        "alpha=false must not paint bottom rows: {:?}",
        plain.fills_inside("picker")
    );
    // Only ring and square take Tab focus when the alpha row does not exist.
    frame(&mut plain, key(egui::Key::Tab));
    assert_eq!(
        plain.pickers.focus.as_ref().unwrap().area,
        render::PickerArea::Ring
    );
    frame(&mut plain, key(egui::Key::Tab));
    assert_eq!(
        plain.pickers.focus.as_ref().unwrap().area,
        render::PickerArea::Square
    );
    frame(&mut plain, key(egui::Key::Tab));
    assert!(plain.pickers.focus.is_none());
    let point = plain.square_point(0, 0.5, 0.5);
    assert!(frame(&mut plain, pointer(point, true)).is_some());
    frame(&mut plain, pointer(point, false));
}

#[test]
fn press_and_release_in_one_frame_still_edits_the_picker() {
    let mut f = fixture(&ui(true));
    frame(&mut f, vec![]);
    let point = f.square_point(0, 0.6, 0.3);
    let events = vec![
        egui::Event::PointerMoved(point),
        egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        },
        egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        },
    ];
    let event = frame(&mut f, events).unwrap();
    assert_eq!(event.name(), "pick");
    assert_ne!(event.value.as_str().unwrap(), "#FF0000FF");
    assert_eq!(
        f.pickers.focus.as_ref().unwrap().area,
        render::PickerArea::Square
    );
}

#[test]
fn review_hidden_picker_must_release_focus() {
    let mut f = fixture(&ui(true));
    frame(&mut f, vec![]);
    let point = f.square_point(0, 0.4, 0.4);
    frame(&mut f, pointer(point, true));
    frame(&mut f, pointer(point, false));
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
        render::PickerArea::Square,
        render::PickerArea::Alpha,
    ] {
        frame(&mut f, key(egui::Key::Tab));
        assert_eq!(f.pickers.focus.as_ref().map(|focus| focus.area), Some(area));
    }
    frame(&mut f, key(egui::Key::Tab));
    assert!(f.pickers.focus.is_none());
}

#[test]
fn only_ring_square_and_optional_alpha_are_painted() {
    // alpha=true: ring mesh, square mesh, alpha checkerboard/gradient, markers, focus feedback.
    let mut f = fixture(&ui(true));
    frame(&mut f, vec![]);
    let bounds = f.node_rect("picker");
    let scale = f.scale();
    assert!(f.texts_inside("picker").is_empty(), "{:?}", f.texts());
    let fills = f.fills_inside("picker");
    assert!(!fills.is_empty(), "alpha row must paint a checkerboard");
    assert!(
        fills
            .iter()
            .all(|(rect, _)| rect.height() <= 18.0 * scale + 0.01),
        "unexpected tall fill inside the picker: {fills:?}"
    );
    let bar = fills
        .iter()
        .find(|(rect, _)| (rect.width() - (ALPHA_SIZE[0] - 16.0) * scale).abs() < 0.01)
        .expect("alpha bar background");
    assert!((bar.0.height() - 18.0 * scale).abs() < 0.01);
    let square = f.square();
    let outer = f.ring_outer(0);
    assert!((square.width() - outer * 0.78 * std::f32::consts::SQRT_2).abs() < 0.01);
    assert!((square.width() - square.height()).abs() < 0.01);
    // The content group is centred: D=min(256, 282-26)=256.
    let center = bounds.min + egui::vec2(128.0, 128.0) * scale;
    assert!(
        square.center().distance(center) < 0.01,
        "{:?}",
        square.center()
    );
    assert!(
        (bar.0.top() - (bounds.top() + (256.0 + 8.0) * scale)).abs() < 0.01,
        "{:?}",
        bar.0
    );
    assert!((outer - (128.0 - 6.0) * scale).abs() < 0.01);

    // alpha=false: no extra rows at all, only the ring/square and focus feedback.
    let mut plain = fixture(&ui(false));
    frame(&mut plain, vec![]);
    let bounds = plain.node_rect("picker");
    let scale = plain.scale();
    assert!(
        plain.texts_inside("picker").is_empty(),
        "{:?}",
        plain.texts()
    );
    assert!(
        plain.fills_inside("picker").is_empty(),
        "alpha=false must not paint any fill: {:?}",
        plain.fills_inside("picker")
    );
    let square = plain.square();
    assert!((square.width() - 122.0 * 0.78 * std::f32::consts::SQRT_2 * scale).abs() < 0.01);
    assert!(
        square
            .center()
            .distance(bounds.min + egui::vec2(128.0, 128.0) * scale)
            < 0.01
    );
}

#[test]
fn two_pickers_keep_independent_float_positions_and_values() {
    let xml = ui(true).replace(
        "<variables><string id=\"color\" initial=\"#FF0000FF\"/></variables>",
        "<variables><string id=\"color\" initial=\"#FF0000FF\"/><string id=\"text\" initial=\"#00FF00FF\"/></variables>",
    )
    .replace(
        "</screen>",
        "<color-picker id=\"second\" x=\"500\" y=\"100\" width=\"256\" height=\"256\" value=\"ui.text\" event=\"pick\" alpha=\"false\"/></screen>",
    );
    let mut f = fixture(&xml);
    frame(&mut f, vec![]);
    assert_eq!(f.squares.len(), 2);
    // Edit the second picker's square; the first control keeps its own cached position.
    let point = f.square_point(1, 0.25, 0.25);
    if let Some(event) = frame(&mut f, pointer(point, true)) {
        f.data.values["ui"]["text"] = event.value;
    }
    frame(&mut f, pointer(point, false));
    assert!(frame(&mut f, vec![]).is_none());
    assert_eq!(
        f.data.values["ui"]["color"],
        serde_json::Value::String("#FF0000FF".into())
    );
    assert!(f.markers.iter().any(|marker| marker.distance(point) < 0.01));
    assert!(
        f.markers
            .iter()
            .any(|marker| marker.distance(f.square_point(0, 1.0, 0.5)) < 0.01)
    );
}

#[test]
fn review_picker_first_focus_must_block_same_frame_advance() -> anyhow::Result<()> {
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
        "frame('opening').advance(()=> 'second');frame('second').end();(event.register('start', ctx=>ctx.restart('opening')), ui('start').bind('start'));(event.register('pick', ctx=>{ctx.setUi('color',ctx.value);ctx.setUi('show',false);}), ui('picker').bind('pick'));ui('$game').bind('game.enter', ctx=>ctx.setUi('title',ctx.game.frameId));",
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
    // 256x320 (a legal non-compact rectangle): the ring top is the committed hue,
    // so this press focuses the picker without emitting a colour candidate.
    let point = egui::pos2(228.0, 136.0);
    let mut events = pointer(point, true);
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
    update(&mut app, pointer(point, false));
    // The author's color callback hides the picker while the arrow key is still
    // being handled. A key still held while the control disappears must not leak
    // into the story.
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

/// Real App → on.ui → host candidate validation/commit → next frame Data/render chain.
/// The author composes the swatch and HEX outside the control; the picker only edits.
struct AppHarness {
    ctx: egui::Context,
    app: kakera_avg::app::PlayerApp,
    _game: tempfile::TempDir,
    _saves: tempfile::TempDir,
    shapes: Vec<egui::epaint::ClippedShape>,
}

fn app_harness(script: &str) -> anyhow::Result<AppHarness> {
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
        r#"<story schema="8" id="picker-chain" content-version="1" entry="opening"><scripts><script src="story.js"/></scripts><interface src="ui.xml"/><assets><font id="font" src="font.otf"/></assets><characters/><visuals/><frames><frame id="opening"><text>第一句。</text></frame></frames><next end="true"/></story>"#,
    )?;
    std::fs::copy(
        "games/demo/assets/fonts/chinese.otf",
        game.path().join("font.otf"),
    )?;
    std::fs::write(game.path().join("story.js"), script)?;
    std::fs::write(
        game.path().join("ui.xml"),
        r##"<ui schema="3" initial="hud"><variables><string id="color" initial="#FF0000FF"/><string id="text" initial="#F4EAD8FF"/></variables><screen id="hud" modal="false" pause="false"><button id="start" x="10" y="10" width="80" height="30" text="开始" event="start"/><color-picker id="picker" x="100" y="100" width="256" height="282" value="ui.color" event="pick" alpha="true"/><color-picker id="plain" x="380" y="100" width="256" height="256" value="ui.text" event="pick-text" alpha="false"/><panel id="swatch" x="700" y="100" width="200" height="80" color-bind="ui.color"/><label id="hex" x="700" y="200" width="400" height="30" bind="ui.color" font-size="20"/><label id="hexText" x="700" y="240" width="400" height="30" bind="ui.text" font-size="20"/><label id="line" x="700" y="300" width="400" height="80" bind="game.text" font-size="20"/></screen></ui>"##,
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
    let app = PlayerApp::new_context(
        &ctx,
        b.loaded,
        b.assets,
        SaveStore::at(saves.path().to_owned()),
        true,
        b.logic,
        Renderer::cpu(),
    )?;
    Ok(AppHarness {
        ctx,
        app,
        _game: game,
        _saves: saves,
        shapes: Vec::new(),
    })
}

impl AppHarness {
    fn update(&mut self, events: Vec<egui::Event>) {
        let ctx = self.ctx.clone();
        let app = &mut self.app;
        let output = ctx.run(
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
        self.shapes = output.shapes;
    }
    fn settle(&mut self) {
        self.update(vec![]);
    }
    fn start(&mut self) {
        self.update(vec![]);
        self.update(pointer(egui::pos2(30.0, 20.0), true));
        self.update(pointer(egui::pos2(30.0, 20.0), false));
        self.settle();
        assert!(
            self.texts().iter().any(|text| text == "第一句。"),
            "story did not enter: {:?}",
            self.texts()
        );
    }
    fn squares(&self) -> Vec<egui::Rect> {
        self.shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.vertices.len() == 33 * 33 => Some(
                    egui::Rect::from_two_pos(mesh.vertices.first()?.pos, mesh.vertices.last()?.pos),
                ),
                _ => None,
            })
            .collect()
    }
    fn square(&self, index: usize) -> egui::Rect {
        *self
            .squares()
            .get(index)
            .unwrap_or_else(|| panic!("missing rendered grid {index}"))
    }
    fn square_point(&self, index: usize, saturation: f32, lightness: f32) -> egui::Pos2 {
        let square = self.square(index);
        egui::pos2(
            square.left() + square.width() * saturation,
            square.bottom() - square.height() * lightness,
        )
    }
    fn ring_outer(&self, index: usize) -> f32 {
        self.square(index).width() / (0.78 * std::f32::consts::SQRT_2)
    }
    fn ring_point(&self, index: usize, hue: f32) -> egui::Pos2 {
        let center = self.square(index).center();
        let (inner, outer) = (self.ring_outer(index) * 0.82, self.ring_outer(index));
        let point = kakera_avg::color::hue_point([center.x, center.y], (inner + outer) * 0.5, hue);
        egui::pos2(point[0], point[1])
    }
    fn alpha_point(&self, index: usize, alpha: f32) -> egui::Pos2 {
        let center = self.square(index).center();
        let edge = self.ring_outer(index) + 6.0;
        egui::pos2(
            center.x - edge + 8.0 + (edge * 2.0 - 16.0) * alpha,
            center.y + edge + 8.0 + 9.0,
        )
    }
    fn markers(&self) -> Vec<egui::Pos2> {
        self.shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Circle(circle)
                    if circle.fill == egui::Color32::WHITE && circle.radius < 6.0 =>
                {
                    Some(circle.center)
                }
                _ => None,
            })
            .collect()
    }
    fn texts(&self) -> Vec<String> {
        self.shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect()
    }
    fn hexes(&self) -> Vec<String> {
        self.texts()
            .into_iter()
            .filter(|text| text.starts_with('#') && text.len() == 9)
            .collect()
    }
    fn fills(&self) -> Vec<(egui::Rect, egui::Color32)> {
        self.shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill != egui::Color32::TRANSPARENT => {
                    Some((rect.rect, rect.fill))
                }
                _ => None,
            })
            .collect()
    }
    fn swatch(&self, color: Rgba) -> bool {
        let expected = egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a);
        self.fills().iter().any(|(rect, fill)| {
            *fill == expected && rect.contains(egui::pos2(800.0, 140.0)) && rect.width() >= 200.0
        })
    }
    fn assert_external(&self, color: Rgba) {
        assert!(
            self.hexes().contains(&color.format()),
            "external HEX missing {}: {:?}",
            color.format(),
            self.hexes()
        );
        assert!(self.swatch(color), "external swatch tint not committed");
    }
}

const ACCEPT: &str = "(event.register('start', ctx=>ctx.restart('opening')), ui('start').bind('start'));(event.register('pick', ctx=>ctx.setUi('color',ctx.value)), ui('picker').bind('pick'));(event.register('pick-text', ctx=>ctx.setUi('text',ctx.value)), ui('plain').bind('pick-text'));frame('opening').end();";

#[test]
fn app_accept_chain_keeps_release_binding_marker_preview_and_hex() -> anyhow::Result<()> {
    let mut h = app_harness(ACCEPT)?;
    h.start();
    let square = h.square(0);
    // Continuous drag: press, several move frames, then release on a new position.
    h.update(pointer(
        egui::pos2(
            egui::lerp(square.x_range(), 0.3),
            egui::lerp(square.y_range(), 0.3),
        ),
        true,
    ));
    for step in [0.4, 0.5, 0.6, 0.7] {
        h.update(vec![egui::Event::PointerMoved(egui::pos2(
            egui::lerp(square.x_range(), step),
            egui::lerp(square.y_range(), 0.3 + step * 0.5),
        ))]);
    }
    // The release position of this frame is the last candidate.
    let release = egui::pos2(
        egui::lerp(square.x_range(), 0.8),
        egui::lerp(square.y_range(), 0.7),
    );
    h.update(pointer(release, false));
    h.settle();
    let accepted = Hsla {
        h: 0.0,
        s: 0.8,
        l: 0.3,
        a: 1.0,
    }
    .to_rgba();
    h.assert_external(accepted);
    for frame in 0..10 {
        h.settle();
        h.assert_external(accepted);
        assert!(
            h.markers()
                .iter()
                .any(|marker| marker.distance(release) < 0.01),
            "marker left the release point on idle frame {frame}: {:?}",
            h.markers()
        );
    }
    // A later keyboard edit must continue from the accepted float sample.
    h.update(key(egui::Key::ArrowUp));
    h.settle();
    let lifted = Hsla {
        h: 0.0,
        s: 0.8,
        l: 0.31,
        a: 1.0,
    }
    .to_rgba();
    h.assert_external(lifted);
    // A different picker keeps its own cache and does not overwrite this draft.
    let plain = h.square(1);
    let plain_point = egui::pos2(
        egui::lerp(plain.x_range(), 0.5),
        egui::lerp(plain.y_range(), 0.5),
    );
    h.update(pointer(plain_point, true));
    h.settle();
    h.update(pointer(plain_point, false));
    h.settle();
    let hexes = h.hexes();
    assert_eq!(hexes.len(), 2, "{hexes:?}");
    assert!(hexes.contains(&lifted.format()), "{hexes:?}");
    assert!(
        hexes.iter().any(|text| text != &lifted.format()),
        "second picker did not commit its own draft: {hexes:?}"
    );
    h.assert_external(lifted);
    Ok(())
}

#[test]
fn app_ring_and_alpha_edits_only_change_their_axis() -> anyhow::Result<()> {
    let mut h = app_harness(ACCEPT)?;
    h.start();
    let hue = h.ring_point(0, 120.0);
    h.update(pointer(hue, true));
    h.update(pointer(hue, false));
    h.settle();
    let ring = Hsla {
        h: 120.0,
        s: 1.0,
        l: 0.5,
        a: 1.0,
    }
    .to_rgba();
    h.assert_external(ring);
    let bar = h.alpha_point(0, 0.5);
    h.update(pointer(bar, true));
    h.update(pointer(bar, false));
    h.settle();
    let translucent = Hsla {
        h: 120.0,
        s: 1.0,
        l: 0.5,
        a: 0.5,
    }
    .to_rgba();
    h.assert_external(translucent);
    assert!(translucent.a < 160 && translucent.a > 100);
    assert_eq!((translucent.r, translucent.g), (ring.r, ring.g));
    Ok(())
}

#[test]
fn app_rejected_candidate_returns_to_the_committed_value() -> anyhow::Result<()> {
    let mut h = app_harness(
        "(event.register('start', ctx=>ctx.restart('opening')), ui('start').bind('start'));(event.register('pick', ctx=>{}), ui('picker').bind('pick'));(event.register('pick-text', ctx=>ctx.setUi('text',ctx.value)), ui('plain').bind('pick-text'));frame('opening').end();",
    )?;
    h.start();
    let square = h.square(0);
    h.update(pointer(
        egui::pos2(
            egui::lerp(square.x_range(), 0.4),
            egui::lerp(square.y_range(), 0.6),
        ),
        true,
    ));
    h.settle();
    h.update(pointer(
        egui::pos2(
            egui::lerp(square.x_range(), 0.9),
            egui::lerp(square.y_range(), 0.2),
        ),
        false,
    ));
    h.settle();
    let committed = Rgba::parse("#FF0000FF").unwrap();
    h.assert_external(committed);
    // The marker follows the committed value, not the rejected release position.
    let red = egui::pos2(egui::lerp(square.x_range(), 1.0), square.center().y);
    assert!(
        h.markers().iter().any(|marker| marker.distance(red) < 0.01),
        "marker did not return to the committed colour: {:?}",
        h.markers()
    );
    Ok(())
}

#[test]
fn app_corrected_candidate_moves_marker_and_external_nodes() -> anyhow::Result<()> {
    let mut h = app_harness(
        "(event.register('start', ctx=>ctx.restart('opening')), ui('start').bind('start'));(event.register('pick', ctx=>ctx.setUi('color','#3366CCFF')), ui('picker').bind('pick'));(event.register('pick-text', ctx=>ctx.setUi('text',ctx.value)), ui('plain').bind('pick-text'));frame('opening').end();",
    )?;
    h.start();
    let square = h.square(0);
    h.update(pointer(
        egui::pos2(
            egui::lerp(square.x_range(), 0.4),
            egui::lerp(square.y_range(), 0.6),
        ),
        true,
    ));
    h.settle();
    h.update(pointer(
        egui::pos2(
            egui::lerp(square.x_range(), 0.4),
            egui::lerp(square.y_range(), 0.6),
        ),
        false,
    ));
    h.settle();
    let corrected = Rgba::parse("#3366CCFF").unwrap();
    h.assert_external(corrected);
    // S=0.6, L=0.5: the marker must follow the corrected value, not the candidate.
    let marker = h.square_point(0, 0.6, 0.5);
    assert!(
        h.markers()
            .iter()
            .any(|point| point.distance(marker) < 0.01),
        "marker did not follow the corrected value: {:?}",
        h.markers()
    );
    Ok(())
}

#[test]
fn both_package_formats_keep_compact_pickers_and_author_composition() -> anyhow::Result<()> {
    use kakera_avg::content::ContentSource;
    use kakera_avg::package::{self, Archive, Snapshot, StagedProject};
    let root = tempfile::tempdir()?;
    let out = tempfile::tempdir()?;
    std::fs::write(
        root.path().join("story.xml"),
        r#"<story schema="8" id="picker-pack" content-version="1" entry="opening"><scripts><script src="story.js"/></scripts><interface src="ui.xml"/><assets><font id="font" src="font.otf"/></assets><characters/><visuals/><frames><frame id="opening"><text>第一句。</text></frame></frames><next end="true"/></story>"#,
    )?;
    std::fs::write(
        root.path().join("story.js"),
        "frame('opening').end();(event.register('pick', ctx=>ctx.setUi('color',ctx.value)), ui('picker').bind('pick'));(event.register('pick-text', ctx=>ctx.setUi('text',ctx.value)), ui('plain').bind('pick-text'));",
    )?;
    std::fs::write(
        root.path().join("ui.xml"),
        r##"<ui schema="3" initial="colors"><variables><string id="color" initial="#FF0000FF"/><string id="text" initial="#F4EAD8FF"/></variables><include src="ui/colors.xml"/></ui>"##,
    )?;
    std::fs::create_dir_all(root.path().join("ui"))?;
    std::fs::write(
        root.path().join("ui/colors.xml"),
        r##"<ui schema="3"><screen id="colors" modal="false" pause="false"><color-picker id="picker" x="40" y="110" width="256" height="282" value="ui.color" event="pick" alpha="true"/><color-picker id="plain" x="330" y="110" width="256" height="256" value="ui.text" event="pick-text" alpha="false"/><panel id="swatch" x="640" y="110" width="450" height="220" color-bind="ui.color"/><label id="hex" x="640" y="345" width="440" height="26" bind="ui.color"/></screen></ui>"##,
    )?;
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/fonts/chinese.otf"),
        root.path().join("font.otf"),
    )?;
    let (snapshot, _) = Snapshot::directory(root.path())?.for_publication()?;
    let one = out.path().join("one.kpak");
    package::write(&one, &snapshot.encode(1)?, false)?;
    let mut stage = StagedProject::capture(root.path())?;
    stage.for_publication()?;
    let two = out.path().join("two.kpak");
    package::write_streaming(&stage, &two, 1, false, |source| {
        kakera_avg::loader::load_game(source)?;
        Ok(())
    })?;
    for path in [one, two] {
        let archive = Archive::open(&path)?;
        let (loaded, _) = kakera_avg::loader::load_game(&archive)?;
        let nodes = &loaded.interface.screens["colors"].nodes;
        let picker = nodes.iter().find(|node| node.id == "picker").unwrap();
        assert!(matches!(
            picker.kind,
            interface::Kind::ColorPicker { alpha: true }
        ));
        assert_eq!(picker.rect, [40.0, 110.0, 256.0, 282.0]);
        assert_eq!(picker.bind.as_deref(), Some("ui.color"));
        let plain = nodes.iter().find(|node| node.id == "plain").unwrap();
        assert!(matches!(
            plain.kind,
            interface::Kind::ColorPicker { alpha: false }
        ));
        assert_eq!(plain.rect, [330.0, 110.0, 256.0, 256.0]);
        let swatch = nodes.iter().find(|node| node.id == "swatch").unwrap();
        assert_eq!(swatch.color_bind.as_deref(), Some("ui.color"));
        let hex = nodes.iter().find(|node| node.id == "hex").unwrap();
        assert_eq!(hex.bind.as_deref(), Some("ui.color"));
        // The merged entry replaces the include; no fragment is shipped beside it.
        assert!(archive.read(Path::new("ui/colors.xml")).is_err());
    }
    Ok(())
}
