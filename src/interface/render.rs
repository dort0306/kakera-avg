//! Read-only XML renderer. Emits one event; never evaluates JS or performs IO.
use super::{Data, Definition, Event, Kind, Node, host::State};
use crate::{
    assets::AssetCache,
    color::{Hsla, Rgba, hue_at, hue_point},
    model::Frame,
};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub struct Input<'a> {
    pub effects: &'a crate::effects::Player,
    pub definition: &'a Definition,
    pub state: &'a State,
    pub data: &'a Data,
    pub frame: &'a Frame,
    pub assets: &'a AssetCache,
    pub visible_text: &'a str,
    pub blocked: bool,
    pub pickers: &'a mut PickerState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerArea {
    Ring,
    Square,
    Alpha,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerFocus {
    pub node: String,
    pub area: PickerArea,
}
#[derive(Clone, Copy, Debug)]
struct PickerCache {
    hsla: Hsla,
    last_hue: f32,
    last_saturation: f32,
}
#[derive(Clone, Copy, Debug)]
struct PickerGesture {
    node: u64,
    area: PickerArea,
    point: Pos2,
}
/// Presentation-only picker state; never serialized into Model, saves or preferences.
#[derive(Default)]
pub struct PickerState {
    pub focus: Option<PickerFocus>,
    cache: BTreeMap<String, PickerCache>,
    gesture: Option<PickerGesture>,
    owns_input: bool,
    release_required: bool,
}
impl PickerState {
    pub fn owns_input(&self) -> bool {
        self.owns_input
    }
    pub fn take_release_request(&mut self) -> bool {
        std::mem::take(&mut self.release_required)
    }
}
fn picker_focus_id(node: &str, area: PickerArea) -> egui::Id {
    egui::Id::new(("color-picker-focus", node, area as u8))
}
fn color(value: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(value[0], value[1], value[2], value[3])
}
fn text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Bool(value) => if *value {
            crate::i18n::text("host-interface-render-0002")
        } else {
            crate::i18n::text("host-interface-render-0003")
        }
        .into(),
        Value::Null => String::new(),
        _ => value.to_string(),
    }
}
fn image(
    painter: &egui::Painter,
    assets: &AssetCache,
    id: &str,
    bounds: Rect,
    fit: &str,
    tint: Color32,
    region: Option<crate::image_layout::Region>,
) {
    if let Some(texture) = assets.image(id) {
        let texture_size = texture.size_vec2();
        let uv = region
            .map(|r| r.uv([texture_size.x, texture_size.y]))
            .unwrap_or([0.0, 0.0, 1.0, 1.0]);
        let size = region
            .map(|r| Vec2::new(r.0[2] as f32, r.0[3] as f32))
            .unwrap_or(texture_size);
        let factor = if fit == "cover" {
            (bounds.width() / size.x).max(bounds.height() / size.y)
        } else {
            (bounds.width() / size.x).min(bounds.height() / size.y)
        };
        let target = if fit == "stretch" {
            bounds
        } else {
            Rect::from_center_size(bounds.center(), size * factor)
        };
        painter
            .with_clip_rect(painter.clip_rect().intersect(bounds))
            .image(
                texture.id(),
                target,
                Rect::from_min_max(Pos2::new(uv[0], uv[1]), Pos2::new(uv[2], uv[3])),
                tint,
            );
    }
}
struct Renderer<'a> {
    input: Input<'a>,
    scale: f32,
    event: Option<Event>,
    controls: Vec<Rect>,
    active_pickers: Vec<String>,
}
struct PickerGeometry {
    center: Pos2,
    ring_inner: f32,
    ring_outer: f32,
    square: Rect,
    alpha_bar: Option<Rect>,
    scale: f32,
}
/// Logical height reserved below the colour disc: an 8 unit gap and the 18 unit bar.
const PICKER_ALPHA_ROW: f32 = 26.0;
const PICKER_ALPHA_GAP: f32 = 8.0;
const PICKER_ALPHA_HEIGHT: f32 = 18.0;
/// Smallest accepted colour disc diameter; the loader enforces the same number.
const PICKER_MIN_DIAMETER: f32 = 128.0;
impl PickerGeometry {
    /// Layout from the node's logical rectangle so loading and drawing share one
    /// definition. The whole content group is centred; only the ring, square and an
    /// optional alpha bar exist, and all of them scale with the stage exactly once.
    fn new(bounds: Rect, size: [f32; 2], alpha: bool, scale: f32) -> Option<Self> {
        let reserved = if alpha { PICKER_ALPHA_ROW } else { 0.0 };
        let diameter = size[0].min(size[1] - reserved);
        if diameter < PICKER_MIN_DIAMETER {
            return None;
        }
        let content = (diameter + reserved) * scale;
        let top = bounds.center().y - content * 0.5;
        let center = Pos2::new(bounds.center().x, top + diameter * 0.5 * scale);
        let radius = (diameter * 0.5 - 6.0) * scale;
        if radius <= 0.0 {
            return None;
        }
        Some(Self {
            center,
            ring_inner: radius * 0.82,
            ring_outer: radius,
            square: Rect::from_center_size(
                center,
                Vec2::splat(radius * 0.78 * std::f32::consts::SQRT_2),
            ),
            alpha_bar: alpha.then(|| {
                Rect::from_min_size(
                    Pos2::new(
                        bounds.min.x + PICKER_ALPHA_GAP * scale,
                        top + (diameter + PICKER_ALPHA_GAP) * scale,
                    ),
                    Vec2::new(
                        (size[0] - PICKER_ALPHA_GAP * 2.0) * scale,
                        PICKER_ALPHA_HEIGHT * scale,
                    ),
                )
            }),
            scale,
        })
    }
    fn hit(&self, point: Pos2) -> Option<PickerArea> {
        if self.square.contains(point) {
            return Some(PickerArea::Square);
        }
        let distance = Vec2::new(point.x - self.center.x, point.y - self.center.y).length();
        if (self.ring_inner..=self.ring_outer).contains(&distance) {
            return Some(PickerArea::Ring);
        }
        if self.alpha_bar.is_some_and(|bar| bar.contains(point)) {
            return Some(PickerArea::Alpha);
        }
        None
    }
    fn apply(&self, area: PickerArea, point: Pos2, cache: &mut PickerCache, hsla: &mut Hsla) {
        match area {
            PickerArea::Ring => {
                hsla.h = hue_at([self.center.x, self.center.y], [point.x, point.y]);
                cache.last_hue = hsla.h;
            }
            PickerArea::Square => {
                hsla.h = cache.last_hue;
                hsla.s = ((point.x - self.square.left()) / self.square.width()).clamp(0.0, 1.0);
                hsla.l =
                    (1.0 - (point.y - self.square.top()) / self.square.height()).clamp(0.0, 1.0);
                cache.last_saturation = hsla.s;
            }
            PickerArea::Alpha => {
                if let Some(bar) = self.alpha_bar {
                    hsla.a = ((point.x - bar.min.x) / bar.width().max(1.0)).clamp(0.0, 1.0);
                }
            }
        }
        cache.hsla = *hsla;
    }
    fn marker_for(&self, area: PickerArea, hsla: Hsla) -> Option<Pos2> {
        match area {
            PickerArea::Ring => {
                let point = hue_point(
                    [self.center.x, self.center.y],
                    (self.ring_inner + self.ring_outer) * 0.5,
                    hsla.h,
                );
                Some(Pos2::new(point[0], point[1]))
            }
            PickerArea::Square => Some(Pos2::new(
                self.square.left() + self.square.width() * hsla.s,
                self.square.bottom() - self.square.height() * hsla.l,
            )),
            PickerArea::Alpha => self
                .alpha_bar
                .map(|bar| Pos2::new(bar.min.x + bar.width() * hsla.a, bar.center().y)),
        }
    }
}
fn hue_color(hue: f32) -> Color32 {
    let (r, g, b) = crate::color::hsv_to_rgb(hue, 1.0, 1.0);
    Color32::from_rgb(r, g, b)
}
fn checkerboard(painter: &egui::Painter, rect: Rect, scale: f32) {
    let cell = (8.0 * scale).max(3.0);
    painter.rect_filled(rect, 0.0, Color32::from_gray(70));
    let mut y = rect.min.y;
    let mut row = 0;
    while y < rect.max.y {
        let mut x = rect.min.x + if row % 2 == 0 { 0.0 } else { cell };
        while x < rect.max.x {
            let end = (x + cell).min(rect.max.x);
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(x, y), Pos2::new(end, (y + cell).min(rect.max.y))),
                0.0,
                Color32::from_gray(95),
            );
            x += cell * 2.0;
        }
        y += cell;
        row += 1;
    }
}
#[cfg(test)]
mod geometry_tests {
    use super::*;
    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, height))
    }
    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.01, "{actual} != {expected}");
    }
    #[test]
    fn compact_and_wide_rectangles_center_the_same_ring_and_square() {
        for (size, alpha, scale) in [
            ([128.0, 128.0], false, 1.0),
            ([128.0, 128.0], false, 0.5),
            ([128.0, 128.0], false, 1.5),
            ([128.0, 128.0], false, 2.0),
            ([128.0, 154.0], true, 1.0),
            ([256.0, 256.0], false, 1.0),
            ([256.0, 282.0], true, 1.0),
            ([256.0, 320.0], true, 1.0),
            ([400.0, 282.0], true, 1.0),
            ([128.0, 400.0], false, 1.0),
            ([200.5, 300.25], true, 1.0),
        ] {
            let bounds = rect(10.0, 20.0, size[0] * scale, size[1] * scale);
            let layout = PickerGeometry::new(bounds, size, alpha, scale).expect("valid geometry");
            let diameter = size[0].min(size[1] - if alpha { 26.0 } else { 0.0 });
            let content = diameter + if alpha { 26.0 } else { 0.0 };
            // The whole content group is centred in the node rectangle.
            assert_close(layout.center.x, bounds.center().x);
            assert_close(
                layout.center.y,
                bounds.center().y - content * 0.5 * scale + diameter * 0.5 * scale,
            );
            // The ring keeps its proportions; neither axis stretches.
            assert_close(layout.ring_outer, (diameter * 0.5 - 6.0) * scale);
            assert_close(layout.ring_inner, layout.ring_outer * 0.82);
            assert_close(
                layout.square.width(),
                layout.ring_outer * 0.78 * std::f32::consts::SQRT_2,
            );
            assert_close(layout.square.width(), layout.square.height());
            assert_close(layout.square.center().x, layout.center.x);
            assert_close(layout.square.center().y, layout.center.y);
            // The square corners stay inside the ring.
            let corner = Vec2::new(layout.square.width(), layout.square.height()).length() * 0.5;
            assert!(corner < layout.ring_inner);
            if alpha {
                let bar = layout.alpha_bar.expect("alpha row");
                assert_close(bar.height(), 18.0 * scale);
                assert_close(bar.width(), (size[0] - 16.0) * scale);
                assert_close(bar.left(), bounds.left() + 8.0 * scale);
                assert_close(bar.right(), bounds.right() - 8.0 * scale);
                assert_close(bar.top(), layout.center.y + (diameter * 0.5 + 8.0) * scale);
                assert_close(
                    bar.bottom(),
                    bounds.bottom() - (size[1] - diameter - 26.0) * 0.5 * scale,
                );
            } else {
                assert!(layout.alpha_bar.is_none());
            }
        }
    }
    #[test]
    fn hit_areas_follow_the_layout_and_the_old_preview_rows_are_inert() {
        let bounds = rect(100.0, 100.0, 256.0, 320.0);
        let layout = PickerGeometry::new(bounds, [256.0, 320.0], true, 1.0).unwrap();
        assert_eq!(layout.hit(layout.center), Some(PickerArea::Square));
        let ring_point = Pos2::new(
            layout.center.x,
            layout.center.y - (layout.ring_inner + layout.ring_outer) * 0.5,
        );
        assert_eq!(layout.hit(ring_point), Some(PickerArea::Ring));
        let bar = layout.alpha_bar.unwrap();
        assert_eq!(layout.hit(bar.center()), Some(PickerArea::Alpha));
        // Blank corner space, the former preview row and the former lower alpha row are inert.
        assert_eq!(layout.hit(Pos2::new(120.0, 380.0)), None);
        assert_eq!(layout.hit(Pos2::new(120.0, 334.0)), None);
        assert_eq!(layout.hit(Pos2::new(200.0, 460.0)), None);
        // Without alpha there is no bar, no gap and no phantom row at all.
        let plain =
            PickerGeometry::new(rect(100.0, 100.0, 128.0, 128.0), [128.0, 128.0], false, 1.0)
                .unwrap();
        assert!(plain.alpha_bar.is_none());
        assert_eq!(plain.hit(Pos2::new(100.0 + 10.0, 100.0 + 120.0)), None);
        assert_eq!(
            plain.hit(Pos2::new(100.0 + 64.0, 100.0 + 64.0)),
            Some(PickerArea::Square)
        );
        assert_eq!(
            plain.hit(Pos2::new(
                100.0 + 64.0,
                100.0 - (plain.ring_inner + plain.ring_outer) * 0.5 + 64.0
            )),
            Some(PickerArea::Ring)
        );
    }
    #[test]
    fn sizes_below_the_contract_are_refused_by_the_geometry() {
        assert!(
            PickerGeometry::new(rect(0.0, 0.0, 127.0, 400.0), [127.0, 400.0], false, 1.0).is_none()
        );
        assert!(
            PickerGeometry::new(rect(0.0, 0.0, 128.0, 153.0), [128.0, 153.0], true, 1.0).is_none()
        );
        assert!(
            PickerGeometry::new(rect(0.0, 0.0, 128.0, 128.0), [128.0, 128.0], true, 1.0).is_none()
        );
    }
}
impl Renderer<'_> {
    fn tint(&self, node: &Node) -> Color32 {
        self.input
            .state
            .color_binds
            .get(&node.id)
            .map(|value| color(value.to_array()))
            .unwrap_or_else(|| color(node.color))
    }
    fn nodes(
        &mut self,
        ui: &mut egui::Ui,
        nodes: &[Node],
        origin: Pos2,
        item: &Value,
        active: bool,
        instance: &str,
    ) {
        for node in nodes {
            ui.scope(|ui| {
                self.node(ui, node, origin, item, active, instance);
            });
        }
    }
    fn node(
        &mut self,
        ui: &mut egui::Ui,
        node: &Node,
        origin: Pos2,
        item: &Value,
        active: bool,
        instance: &str,
    ) {
        let alpha = self
            .input
            .effects
            .ui_alpha
            .get(&node.id)
            .copied()
            .unwrap_or(1.);
        ui.multiply_opacity(alpha);
        let active = active && ui.opacity() > 0.;
        if !self.input.data.flag(&node.visible, item) {
            return;
        }
        let bounds = Rect::from_min_size(
            origin + Vec2::new(node.rect[0], node.rect[1]) * self.scale,
            Vec2::new(node.rect[2], node.rect[3]) * self.scale,
        );
        if !bounds.intersects(ui.clip_rect()) {
            return;
        }
        let enabled = active && self.input.data.flag(&node.enabled, item);
        if active
            && matches!(
                node.kind,
                Kind::Button
                    | Kind::Slider { .. }
                    | Kind::Toggle
                    | Kind::Repeat { .. }
                    | Kind::ColorPicker { .. }
            )
        {
            self.controls.push(bounds.intersect(ui.clip_rect()));
        }
        let painter = ui
            .painter()
            .with_clip_rect(ui.clip_rect().intersect(bounds));
        let id = format!("xml:{instance}:{}", node.id);
        let label = self
            .input
            .state
            .localized_text
            .get(&node.id)
            .cloned()
            .unwrap_or_else(|| {
                node.bind
                    .as_ref()
                    .filter(|_| !matches!(node.kind, Kind::Slider { .. } | Kind::Toggle))
                    .and_then(|path| self.input.data.get(path, item))
                    .map(text)
                    .unwrap_or_else(|| node.text.clone())
            });
        let image_id = node.image.as_deref().or_else(|| {
            node.image_bind
                .as_ref()
                .and_then(|path| self.input.data.get(path, item))
                .and_then(Value::as_str)
        });
        match &node.kind {
            Kind::NinePatch => {
                if let Some((patch, texture)) = image_id.and_then(|id| {
                    self.input
                        .assets
                        .nine_patch(id)
                        .zip(self.input.assets.image(id))
                }) {
                    let (patches, content) =
                        patch.layout([bounds.width(), bounds.height()], self.scale / node.density);
                    let mut mesh = egui::epaint::Mesh::with_texture(texture.id());
                    for patch_rect in patches {
                        let [l, t, r, b] = patch_rect.source;
                        let [x, y, u, v] = patch_rect.target;
                        // Piece edges share source coordinates; clipping hides the removed marker border.
                        mesh.add_rect_with_uv(
                            Rect::from_min_max(
                                bounds.min + Vec2::new(x, y),
                                bounds.min + Vec2::new(u, v),
                            ),
                            Rect::from_min_max(
                                Pos2::new(l / patch.size[0] as f32, t / patch.size[1] as f32),
                                Pos2::new(r / patch.size[0] as f32, b / patch.size[1] as f32),
                            ),
                            self.tint(node),
                        );
                    }
                    painter.add(egui::Shape::mesh(mesh));
                    let area = Rect::from_min_max(
                        bounds.min + Vec2::new(content[0], content[1]),
                        bounds.min + Vec2::new(content[2], content[3]),
                    );
                    ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                        ui.set_clip_rect(ui.clip_rect().intersect(area));
                        self.nodes(ui, &node.children, area.min, item, enabled, &id);
                    });
                }
            }
            Kind::Scene => {
                if self.input.data.values["game"]["frameId"] == "" {
                    return;
                }
                let mut motion = self
                    .input
                    .effects
                    .target_motion(&node.id, Default::default());
                motion.offset[0] *= self.scale;
                motion.offset[1] *= self.scale;
                crate::effects::render::scene(
                    &painter,
                    bounds,
                    self.input.frame,
                    self.input.assets,
                    self.input.effects,
                    motion,
                )
            }
            Kind::Panel => {
                if let Some(image_id) = image_id {
                    let mut motion = self
                        .input
                        .effects
                        .target_motion(&node.id, node.effect.animation);
                    motion.offset[0] *= self.scale;
                    motion.offset[1] *= self.scale;
                    crate::effects::render::image(
                        &painter,
                        self.input.assets,
                        crate::effects::render::Image {
                            region: self.input.state.regions.get(&node.id).copied(),
                            id: image_id,
                            bounds,
                            fit: &node.fit,
                            tint: self.tint(node),
                            effect: &node.effect,
                            motion,
                            opacity: 1.0,
                        },
                    );
                } else {
                    painter.rect_filled(bounds, node.radius * self.scale, self.tint(node));
                }
                ui.scope_builder(egui::UiBuilder::new().max_rect(bounds), |ui| {
                    ui.set_clip_rect(ui.clip_rect().intersect(bounds));
                    self.nodes(ui, &node.children, bounds.min, item, enabled, &id);
                });
            }
            Kind::Label => {
                let font = FontId::proportional(node.font * self.scale);
                let text_color = self.tint(node);
                let mut job = if node.bind.as_deref() == Some("game.text") {
                    crate::view::dialogue_job(&label, self.input.visible_text, font, bounds.width())
                } else {
                    egui::text::LayoutJob::simple(label.clone(), font, text_color, bounds.width())
                };
                // Preserve invisible suffix while honoring the XML text color.
                if node.bind.as_deref() == Some("game.text") {
                    for section in &mut job.sections {
                        if section.format.color != Color32::TRANSPARENT {
                            section.format.color = text_color;
                        }
                    }
                }
                let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                if galley.size().y > bounds.height() + 1.0 {
                    painter.text(
                        bounds.min,
                        egui::Align2::LEFT_TOP,
                        &self.input.state.overflow_messages[&node.id],
                        FontId::proportional(12.0 * self.scale),
                        Color32::LIGHT_RED,
                    );
                } else {
                    painter.galley(bounds.min, galley, Color32::WHITE);
                }
            }
            Kind::Button => {
                let response = ui.interact(
                    bounds,
                    ui.id().with(&id),
                    if enabled {
                        Sense::click()
                    } else {
                        Sense::hover()
                    },
                );
                let skin = if !enabled {
                    node.disabled_image.as_deref().or(image_id)
                } else if response.hovered() {
                    node.hover_image.as_deref().or(image_id)
                } else {
                    image_id
                };
                let tint = if enabled {
                    self.tint(node)
                } else {
                    self.tint(node).gamma_multiply(0.45)
                };
                if let Some(skin) = skin {
                    image(
                        &painter,
                        self.input.assets,
                        skin,
                        bounds,
                        "stretch",
                        tint,
                        self.input.state.regions.get(&node.id).copied(),
                    );
                } else {
                    painter.rect_filled(
                        bounds,
                        node.radius * self.scale,
                        if response.hovered() && enabled {
                            tint.gamma_multiply(1.25)
                        } else {
                            tint
                        },
                    );
                }
                let galley = ui.fonts_mut(|fonts| {
                    fonts.layout(
                        label,
                        FontId::proportional(node.font * self.scale),
                        Color32::WHITE,
                        bounds.width() - 8.0 * self.scale,
                    )
                });
                painter.galley(
                    bounds.center() - galley.size() / 2.0,
                    galley,
                    Color32::WHITE,
                );
                if enabled && response.clicked() {
                    self.emit(node, item, Value::Null);
                }
            }
            Kind::Slider { min, max } => {
                let mut value = node
                    .bind
                    .as_ref()
                    .and_then(|path| self.input.data.get(path, item))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0) as f32;
                let response = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(bounds), |ui| {
                        ui.add_enabled_ui(enabled, |ui| {
                            ui.spacing_mut().slider_width =
                                (bounds.width() - 70.0 * self.scale).max(20.0);
                            ui.add_sized(bounds.size(), egui::Slider::new(&mut value, *min..=*max))
                        })
                        .inner
                    })
                    .inner;
                if response.changed() && enabled {
                    self.emit(node, item, json!(value));
                }
            }
            Kind::Toggle => {
                let mut value = node
                    .bind
                    .as_ref()
                    .and_then(|path| self.input.data.get(path, item))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let response = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(bounds), |ui| {
                        ui.add_enabled_ui(enabled, |ui| {
                            ui.add_sized(
                                bounds.size(),
                                egui::Checkbox::new(
                                    &mut value,
                                    egui::RichText::new(label)
                                        .size(node.font * self.scale)
                                        .color(self.tint(node)),
                                ),
                            )
                        })
                        .inner
                    })
                    .inner;
                if response.changed() && enabled {
                    self.emit(node, item, json!(value));
                }
            }
            Kind::ColorPicker { alpha } => {
                self.color_picker(ui, node, bounds, item, enabled, *alpha);
            }
            Kind::Repeat {
                source,
                columns,
                cell,
                gap,
                pagination,
            } => {
                // Paginated lists are cached per repeat ID; plain repeats keep the
                // shared source list. The page is part of the scroll/instance identity.
                let page = pagination
                    .as_ref()
                    .and_then(|_| {
                        self.input
                            .data
                            .values
                            .get("repeat")?
                            .get(&node.id)?
                            .get("page")?
                            .as_i64()
                    })
                    .unwrap_or(1);
                let key = match pagination {
                    Some(_) => crate::interface::pagination::list_key(&node.id),
                    None => source.clone(),
                };
                let Some(items) = self.input.data.lists.get(&key) else {
                    return;
                };
                ui.scope_builder(egui::UiBuilder::new().max_rect(bounds), |ui| {
                    ui.set_clip_rect(ui.clip_rect().intersect(bounds));
                    egui::ScrollArea::vertical()
                        .id_salt(if pagination.is_some() {
                            format!("{id}:{page}")
                        } else {
                            id.clone()
                        })
                        .max_height(bounds.height())
                        .show(ui, |ui| {
                            let rows = items.len().div_ceil(*columns);
                            let size = Vec2::new(
                                (*columns as f32 * (cell[0] + gap) - gap).max(0.0),
                                (rows as f32 * (cell[1] + gap) - gap).max(0.0),
                            ) * self.scale;
                            let (content, _) = ui.allocate_exact_size(size, Sense::hover());
                            for (index, item) in items.iter().enumerate() {
                                let origin = content.min
                                    + Vec2::new(
                                        (index % columns) as f32 * (cell[0] + gap),
                                        (index / columns) as f32 * (cell[1] + gap),
                                    ) * self.scale;
                                self.nodes(
                                    ui,
                                    &node.children,
                                    origin,
                                    item,
                                    enabled,
                                    &format!("{id}:{page}:{index}"),
                                );
                            }
                        });
                });
            }
        }
    }
    fn color_picker(
        &mut self,
        ui: &mut egui::Ui,
        node: &Node,
        bounds: Rect,
        item: &Value,
        enabled: bool,
        alpha: bool,
    ) {
        let Some(path) = node.bind.as_deref() else {
            return;
        };
        let Some(committed) = self
            .input
            .data
            .get(path, item)
            .and_then(Value::as_str)
            .and_then(|text| Rgba::parse(text).ok())
        else {
            return;
        };
        let scale = self.scale;
        let Some(layout) = PickerGeometry::new(bounds, [node.rect[2], node.rect[3]], alpha, scale)
        else {
            return;
        };
        let node_key = egui::Id::new(("color-picker", &node.id)).value();
        let cache = self
            .input
            .pickers
            .cache
            .entry(node.id.clone())
            .or_insert_with(|| {
                let hsla = Hsla::from_rgba(committed, None, None);
                PickerCache {
                    hsla,
                    last_hue: hsla.h,
                    last_saturation: hsla.s,
                }
            });
        // Preserve accepted float samples and arbitrary S on the black/white edges.
        // A different committed RGBA is authoritative (callback correction or rejection).
        let mut hsla = if cache.hsla.to_rgba() == committed {
            cache.hsla
        } else {
            Hsla::from_rgba(committed, Some(cache.last_hue), Some(cache.last_saturation))
        };
        if hsla.s > 0.0 && hsla.l > 0.0 && hsla.l < 1.0 {
            cache.last_hue = hsla.h;
        }
        cache.last_saturation = hsla.s;
        cache.hsla = hsla;

        let clip = ui.clip_rect().intersect(bounds);
        // Register each editable sub-area in egui's normal Tab order. Buttons and
        // other controls use the same focus manager, including reverse traversal.
        let mut keyboard_area = None;
        if enabled {
            self.active_pickers.push(node.id.clone());
            for area in [PickerArea::Ring, PickerArea::Square, PickerArea::Alpha] {
                if area == PickerArea::Alpha && !alpha {
                    continue;
                }
                let response = ui.interact(
                    clip,
                    picker_focus_id(&node.id, area),
                    Sense::focusable_noninteractive(),
                );
                if response.has_focus() {
                    keyboard_area = Some(area);
                }
            }
        }
        if let Some(area) = keyboard_area {
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    picker_focus_id(&node.id, area),
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
            self.input.pickers.focus = Some(PickerFocus {
                node: node.id.clone(),
                area,
            });
            self.input.pickers.owns_input = true;
        } else if self
            .input
            .pickers
            .focus
            .as_ref()
            .is_some_and(|focus| focus.node == node.id)
        {
            if let Some(focus) = self.input.pickers.focus.as_ref() {
                ui.memory_mut(|memory| {
                    memory.surrender_focus(picker_focus_id(&focus.node, focus.area))
                });
            }
            self.input.pickers.focus = None;
            self.input.pickers.gesture = None;
            self.input.pickers.release_required = true;
        }
        // Read press/release positions from this frame's events. A synthetic or very fast
        // click can press and release between two frames, so `pointer.press_origin()` may
        // already be gone by the time the UI is built.
        let (pointer_down, latest, shift, press_origin, pointer_released) = ui.input(|input| {
            let press = input.events.iter().rev().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } => Some(*pos),
                _ => None,
            });
            let released = input.events.iter().rev().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    ..
                } => Some(*pos),
                _ => None,
            });
            (
                input.pointer.primary_down(),
                input.pointer.interact_pos(),
                input.modifiers.shift,
                press,
                released,
            )
        });
        let pointer_pressed = press_origin.is_some();
        if press_origin.is_some_and(|pos| clip.contains(pos)) {
            // Even a preview click, a disabled picker, or selecting the current
            // color consumes this frame without needing a color-change event.
            self.input.pickers.owns_input = true;
            self.input.pickers.release_required = true;
        }
        let mut changed = false;
        let mut focused_area = self
            .input
            .pickers
            .focus
            .as_ref()
            .filter(|focus| focus.node == node.id)
            .map(|focus| focus.area);
        if enabled {
            // Pressing outside clears the picker focus; only this control's rounded area starts a drag.
            if let Some(origin) = press_origin
                && pointer_pressed
            {
                if !clip.contains(origin) {
                    if let Some(area) = focused_area {
                        ui.memory_mut(|memory| {
                            memory.surrender_focus(picker_focus_id(&node.id, area))
                        });
                        self.input.pickers.focus = None;
                        self.input.pickers.release_required = true;
                        focused_area = None;
                    }
                } else if let Some(area) = layout.hit(origin) {
                    ui.memory_mut(|memory| memory.request_focus(picker_focus_id(&node.id, area)));
                    self.input.pickers.owns_input = true;
                    self.input.pickers.focus = Some(PickerFocus {
                        node: node.id.clone(),
                        area,
                    });
                    focused_area = Some(area);
                    self.input.pickers.gesture = Some(PickerGesture {
                        node: node_key,
                        area,
                        point: origin,
                    });
                    layout.apply(area, origin, cache, &mut hsla);
                    changed = true;
                }
            }
            if let Some(mut gesture) = self.input.pickers.gesture
                && gesture.node == node_key
                && (pointer_down || pointer_released.is_some())
                && let Some(point) = pointer_released.or(latest)
                && point != gesture.point
            {
                layout.apply(gesture.area, point, cache, &mut hsla);
                gesture.point = point;
                self.input.pickers.gesture = Some(gesture);
                changed = true;
            }
            if (pointer_released.is_some() || !pointer_down)
                && let Some(gesture) = self.input.pickers.gesture
                && gesture.node == node_key
            {
                self.input.pickers.gesture = None;
            }
            if self
                .input
                .pickers
                .gesture
                .is_some_and(|gesture| gesture.node == node_key)
            {
                hsla = cache.hsla;
            }

            // Keyboard editing only while this control owns focus.
            if let Some(area) = focused_area {
                let escape = ui.input(|input| input.key_pressed(egui::Key::Escape));
                if escape {
                    ui.memory_mut(|memory| memory.surrender_focus(picker_focus_id(&node.id, area)));
                    self.input.pickers.focus = None;
                    self.input.pickers.gesture = None;
                    self.input.pickers.release_required = true;
                    focused_area = None;
                } else {
                    let arrows = ui.input(|input| {
                        [
                            input.key_pressed(egui::Key::ArrowLeft),
                            input.key_pressed(egui::Key::ArrowRight),
                            input.key_pressed(egui::Key::ArrowUp),
                            input.key_pressed(egui::Key::ArrowDown),
                        ]
                    });
                    let step = if shift { 10.0 } else { 1.0 };
                    let fine = if shift { 0.1 } else { 0.01 };
                    let mut adjusted = false;
                    match area {
                        PickerArea::Ring => {
                            if arrows[0] {
                                hsla.h = (hsla.h - step).rem_euclid(360.0);
                                adjusted = true;
                            } else if arrows[1] {
                                hsla.h = (hsla.h + step).rem_euclid(360.0);
                                adjusted = true;
                            }
                            if adjusted {
                                cache.last_hue = hsla.h;
                            }
                        }
                        PickerArea::Square => {
                            if arrows[0] {
                                hsla.s = (hsla.s - fine).clamp(0.0, 1.0);
                                adjusted = true;
                            } else if arrows[1] {
                                hsla.s = (hsla.s + fine).clamp(0.0, 1.0);
                                adjusted = true;
                            } else if arrows[2] {
                                hsla.l = (hsla.l + fine).clamp(0.0, 1.0);
                                adjusted = true;
                            } else if arrows[3] {
                                hsla.l = (hsla.l - fine).clamp(0.0, 1.0);
                                adjusted = true;
                            }
                            if adjusted {
                                if hsla.l > 0.0 {
                                    cache.last_saturation = hsla.s;
                                }
                                if hsla.s > 0.0 {
                                    cache.last_hue = hsla.h;
                                }
                            }
                        }
                        PickerArea::Alpha => {
                            if arrows[0] {
                                hsla.a = (hsla.a - fine).clamp(0.0, 1.0);
                                adjusted = true;
                            } else if arrows[1] {
                                hsla.a = (hsla.a + fine).clamp(0.0, 1.0);
                                adjusted = true;
                            }
                        }
                    }
                    if adjusted {
                        cache.hsla = hsla;
                        changed = true;
                    }
                }
            }
        }

        let painter = ui.painter().with_clip_rect(clip);
        // Hue ring: fixed 192 segments, including all six hue boundaries.
        {
            let mut mesh = egui::epaint::Mesh::default();
            for index in 0..192 {
                let h0 = index as f32 * 360.0 / 192.0;
                let h1 = (index + 1) as f32 * 360.0 / 192.0;
                let a0 = h0.to_radians() - std::f32::consts::FRAC_PI_2;
                let a1 = h1.to_radians() - std::f32::consts::FRAC_PI_2;
                let (sin0, cos0) = (a0.sin(), a0.cos());
                let (sin1, cos1) = (a1.sin(), a1.cos());
                let inner = layout.ring_inner;
                let outer = layout.ring_outer;
                let base = mesh.vertices.len() as u32;
                let color0 = hue_color(h0);
                let color1 = hue_color(h1);
                for (radius, sin, cos, color) in [
                    (inner, sin0, cos0, color0),
                    (outer, sin0, cos0, color0),
                    (outer, sin1, cos1, color1),
                    (inner, sin1, cos1, color1),
                ] {
                    mesh.vertices.push(egui::epaint::Vertex {
                        pos: Pos2::new(
                            layout.center.x + cos * radius,
                            layout.center.y + sin * radius,
                        ),
                        uv: egui::epaint::WHITE_UV,
                        color,
                    });
                }
                mesh.indices
                    .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
            painter.add(egui::Shape::mesh(mesh));
        }
        // HSL square: S grows left-to-right, L bottom-to-top. An even grid includes
        // the pure-color midpoint row and approximates each bilinear half uniformly.
        const STEPS: u32 = 32;
        let mut mesh = egui::epaint::Mesh::default();
        for row in 0..=STEPS {
            let lightness = 1.0 - row as f32 / STEPS as f32;
            for column in 0..=STEPS {
                let saturation = column as f32 / STEPS as f32;
                let rgba = Hsla {
                    h: hsla.h,
                    s: saturation,
                    l: lightness,
                    a: 1.0,
                }
                .to_rgba();
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: Pos2::new(
                        layout.square.left() + layout.square.width() * saturation,
                        layout.square.bottom() - layout.square.height() * lightness,
                    ),
                    uv: egui::epaint::WHITE_UV,
                    color: color(rgba.to_array()),
                });
            }
        }
        for row in 0..STEPS {
            for column in 0..STEPS {
                let top = row * (STEPS + 1) + column;
                let bottom = top + STEPS + 1;
                mesh.indices
                    .extend([top, top + 1, bottom + 1, top, bottom + 1, bottom]);
            }
        }
        painter.add(egui::Shape::mesh(mesh));
        // The authoritative RGBA echo stays in the event value; the control itself only
        // paints the ring, the square and an optional alpha row. Author XML/JS composes
        // any swatch or HEX label from the bound draft instead.
        if let Some(bar) = layout.alpha_bar {
            checkerboard(&painter, bar, layout.scale);
            let rgba = hsla.to_rgba();
            let start = Color32::from_rgba_unmultiplied(rgba.r, rgba.g, rgba.b, 0);
            let end = Color32::from_rgba_unmultiplied(rgba.r, rgba.g, rgba.b, 255);
            let mut mesh = egui::epaint::Mesh::default();
            for (x, color) in [(bar.left(), start), (bar.right(), end)] {
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: Pos2::new(x, bar.top()),
                    uv: egui::epaint::WHITE_UV,
                    color,
                });
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: Pos2::new(x, bar.bottom()),
                    uv: egui::epaint::WHITE_UV,
                    color,
                });
            }
            mesh.indices.extend([0, 1, 2, 2, 1, 3]);
            painter.add(egui::Shape::mesh(mesh));
        }
        // Markers have black/white outlines so they stay visible on any hue.
        let marker = |point: Pos2| {
            painter.circle_filled(point, 7.0 * layout.scale, Color32::BLACK);
            painter.circle_filled(point, 5.0 * layout.scale, Color32::WHITE);
        };
        let area = focused_area.or_else(|| {
            self.input
                .pickers
                .gesture
                .filter(|gesture| gesture.node == node_key)
                .map(|gesture| gesture.area)
        });
        if let Some(area) = area {
            if let Some(point) = layout.marker_for(area, hsla) {
                marker(point);
            }
        } else {
            for area in [PickerArea::Ring, PickerArea::Square] {
                if let Some(point) = layout.marker_for(area, hsla) {
                    marker(point);
                }
            }
            if let Some(point) = layout.marker_for(PickerArea::Alpha, hsla) {
                marker(point);
            }
        }
        if self
            .input
            .pickers
            .focus
            .as_ref()
            .is_some_and(|focus| focus.node == node.id)
        {
            painter.rect_stroke(
                bounds,
                2.0,
                Stroke::new(2.0 * layout.scale, Color32::from_rgb(240, 202, 137)),
                egui::StrokeKind::Inside,
            );
        }
        if changed && hsla.to_rgba() != committed {
            self.emit(node, item, json!(hsla.to_rgba().format()));
        }
    }
    fn emit(&mut self, node: &Node, item: &Value, value: Value) {
        if self.event.is_none()
            && (node.event.is_some() || node.route.is_some() || node.sound.is_some())
        {
            self.event = Some(Event {
                sound: node.sound.clone(),
                route: node.route.clone(),
                key: crate::interface::UiEventKey::ui(
                    node.id.clone(),
                    node.event.clone().unwrap_or_default(),
                ),
                item: item.clone(),
                value,
            });
        }
    }
}
pub fn draw(ui: &mut egui::Ui, input: Input<'_>) -> Option<Event> {
    let area = ui.max_rect();
    let scale = (area.width() / input.definition.size[0])
        .min(area.height() / input.definition.size[1])
        .max(0.01);
    let stage = Rect::from_center_size(area.center(), Vec2::from(input.definition.size) * scale);
    let mut renderer = Renderer {
        input,
        scale,
        event: None,
        controls: Vec::new(),
        active_pickers: Vec::new(),
    };
    renderer.input.pickers.owns_input =
        renderer.input.pickers.focus.is_some() || renderer.input.pickers.gesture.is_some();
    ui.scope_builder(egui::UiBuilder::new().max_rect(stage), |ui| {
        ui.set_clip_rect(stage);
        for (index, id) in renderer.input.state.stack.iter().enumerate() {
            let screen = &renderer.input.definition.screens[id];
            let active = index + 1 == renderer.input.state.stack.len() && !renderer.input.blocked;
            renderer.nodes(ui, &screen.nodes, stage.min, &Value::Null, active, id);
        }
    });
    // A hidden parent, clipping, disabling, or a new top page can bypass the
    // node completely. Reconcile against the controls actually eligible now.
    if let Some(focus) = renderer.input.pickers.focus.as_ref()
        && !renderer.active_pickers.contains(&focus.node)
    {
        ui.memory_mut(|memory| memory.surrender_focus(picker_focus_id(&focus.node, focus.area)));
        renderer.input.pickers.focus = None;
        renderer.input.pickers.gesture = None;
        renderer.input.pickers.release_required = true;
    }
    renderer
        .input
        .pickers
        .cache
        .retain(|id, _| renderer.active_pickers.contains(id));
    let top = renderer
        .input
        .state
        .stack
        .last()
        .expect("validated UI stack");
    let screen = &renderer.input.definition.screens[top];
    let eligible = |p: Pos2| {
        stage.contains(p)
            && !renderer.controls.iter().any(|r| r.contains(p))
            && ui
                .ctx()
                .layer_id_at(p)
                .is_none_or(|layer| layer == ui.layer_id())
            && !renderer.input.blocked
            && ui.input(|i| i.focused)
    };
    let candidate = ui.input(|i| i.pointer.interact_pos()).is_some_and(eligible);
    let id = ui.id().with("xml-background-gesture");
    let mut gesture = ui
        .ctx()
        .data_mut(|d| d.get_temp::<BackgroundGesture>(id).unwrap_or_default());
    if gesture.screen != *top {
        gesture = BackgroundGesture {
            screen: top.clone(),
            ..Default::default()
        };
    }
    for (index, button) in [egui::PointerButton::Primary, egui::PointerButton::Secondary]
        .into_iter()
        .enumerate()
    {
        let press = ui.input(|i| {
            i.events.iter().rev().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: b,
                    pressed: true,
                    ..
                } if *b == button => Some(*pos),
                _ => None,
            })
        });
        if let Some(pos) = press {
            gesture.pressed[index] = eligible(pos);
        }
    }
    if renderer.event.is_none() {
        // Right click wins if both buttons are released together. Never dispatch two intents.
        for (index, button, event) in [
            (
                1,
                egui::PointerButton::Secondary,
                &screen.background_right_click,
            ),
            (0, egui::PointerButton::Primary, &screen.background_click),
        ] {
            if candidate
                && gesture.pressed[index]
                && ui.input(|i| i.pointer.button_clicked(button))
                && let Some(event) = event
            {
                renderer.event = Some(Event {
                    sound: None,
                    route: None,
                    key: crate::interface::UiEventKey::ui(top.clone(), event.clone()),
                    item: Value::Null,
                    value: Value::Null,
                });
                break;
            }
        }
    }
    for (index, button) in [egui::PointerButton::Primary, egui::PointerButton::Secondary]
        .into_iter()
        .enumerate()
    {
        if renderer.input.blocked
            || !ui.input(|i| i.focused)
            || !ui.input(|i| i.pointer.button_down(button))
        {
            gesture.pressed[index] = false;
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, gesture));
    renderer.event
}

#[derive(Clone, Default)]
struct BackgroundGesture {
    screen: String,
    pressed: [bool; 2],
}
