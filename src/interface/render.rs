//! Read-only XML renderer. Emits one event; never evaluates JS or performs IO.
use super::{Data, Definition, Event, Kind, Node, host::State};
use crate::{assets::AssetCache, model::Frame};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Sense, Vec2};
use serde_json::{Value, json};

pub struct Input<'a> {
    pub effects: &'a crate::effects::Player,
    pub definition: &'a Definition,
    pub state: &'a State,
    pub data: &'a Data,
    pub frame: &'a Frame,
    pub assets: &'a AssetCache,
    pub visible_text: &'a str,
    pub blocked: bool,
}
fn color(value: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(value[0], value[1], value[2], value[3])
}
fn text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Bool(value) => if *value { "是" } else { "否" }.into(),
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
}
impl Renderer<'_> {
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
            if !self.input.data.flag(&node.visible, item) {
                continue;
            }
            let bounds = Rect::from_min_size(
                origin + Vec2::new(node.rect[0], node.rect[1]) * self.scale,
                Vec2::new(node.rect[2], node.rect[3]) * self.scale,
            );
            if !bounds.intersects(ui.clip_rect()) {
                continue;
            }
            let enabled = active && self.input.data.flag(&node.enabled, item);
            if active
                && matches!(
                    node.kind,
                    Kind::Button | Kind::Slider { .. } | Kind::Toggle | Kind::Repeat { .. }
                )
            {
                self.controls.push(bounds.intersect(ui.clip_rect()));
            }
            let painter = ui
                .painter()
                .with_clip_rect(ui.clip_rect().intersect(bounds));
            let id = format!("xml:{instance}:{}", node.id);
            let label = node
                .bind
                .as_ref()
                .filter(|_| !matches!(node.kind, Kind::Slider { .. } | Kind::Toggle))
                .and_then(|path| self.input.data.get(path, item))
                .map(text)
                .unwrap_or_else(|| node.text.clone());
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
                        let (patches, content) = patch
                            .layout([bounds.width(), bounds.height()], self.scale / node.density);
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
                                color(node.color),
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
                Kind::Panel | Kind::Image => {
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
                                tint: color(node.color),
                                effect: &node.effect,
                                motion,
                                opacity: 1.0,
                            },
                        );
                    } else {
                        painter.rect_filled(bounds, node.radius * self.scale, color(node.color));
                    }
                    ui.scope_builder(egui::UiBuilder::new().max_rect(bounds), |ui| {
                        ui.set_clip_rect(ui.clip_rect().intersect(bounds));
                        self.nodes(ui, &node.children, bounds.min, item, enabled, &id);
                    });
                }
                Kind::Label => {
                    let font = FontId::proportional(node.font * self.scale);
                    let mut job = if node.bind.as_deref() == Some("game.text") {
                        crate::view::dialogue_job(
                            &label,
                            self.input.visible_text,
                            font,
                            bounds.width(),
                        )
                    } else {
                        egui::text::LayoutJob::simple(
                            label.clone(),
                            font,
                            color(node.color),
                            bounds.width(),
                        )
                    };
                    // Preserve invisible suffix while honoring the XML text color.
                    if node.bind.as_deref() == Some("game.text") {
                        for section in &mut job.sections {
                            if section.format.color != Color32::TRANSPARENT {
                                section.format.color = color(node.color);
                            }
                        }
                    }
                    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                    if galley.size().y > bounds.height() + 1.0 {
                        painter.text(
                            bounds.min,
                            egui::Align2::LEFT_TOP,
                            format!("UI 文本溢出：{}", node.id),
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
                        color(node.color)
                    } else {
                        color(node.color).gamma_multiply(0.45)
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
                                ui.add_sized(
                                    bounds.size(),
                                    egui::Slider::new(&mut value, *min..=*max),
                                )
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
                                            .color(color(node.color)),
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
                Kind::Repeat {
                    source,
                    columns,
                    cell,
                    gap,
                } => {
                    let Some(items) = self.input.data.lists.get(source) else {
                        continue;
                    };
                    ui.scope_builder(egui::UiBuilder::new().max_rect(bounds), |ui| {
                        ui.set_clip_rect(ui.clip_rect().intersect(bounds));
                        egui::ScrollArea::vertical()
                            .id_salt(&id)
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
                                        &format!("{id}:{index}"),
                                    );
                                }
                            });
                    });
                }
            }
        }
    }
    fn emit(&mut self, node: &Node, item: &Value, value: Value) {
        if self.event.is_none()
            && let Some(name) = &node.event
        {
            self.event = Some(Event {
                name: name.clone(),
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
    };
    ui.scope_builder(egui::UiBuilder::new().max_rect(stage), |ui| {
        ui.set_clip_rect(stage);
        for (index, id) in renderer.input.state.stack.iter().enumerate() {
            let screen = &renderer.input.definition.screens[id];
            let active = index + 1 == renderer.input.state.stack.len() && !renderer.input.blocked;
            renderer.nodes(ui, &screen.nodes, stage.min, &Value::Null, active, id);
        }
    });
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
                    name: event.clone(),
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
