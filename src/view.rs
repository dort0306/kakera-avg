use crate::{
    assets::AssetCache,
    controller::Intent,
    model::{Choice, Frame, Route, ScriptRoute, Slot, Story},
};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Stroke, Vec2};

pub enum UiIntent {
    Custom(crate::interface::Event),
    Play(Intent),
    Save,
    Load,
    Restart,
    History,
    Settings,
}

pub struct RenderState<'a> {
    pub visible_text: &'a str,
    pub complete: bool,
    pub blocked: bool,
    pub options: &'a [&'a Choice],
}

/// Scene content only. XML UI decides where to place it and all controls around it.
pub fn draw_scene(painter: &egui::Painter, stage: Rect, frame: &Frame, assets: &AssetCache) {
    draw_scene_at(painter, stage, frame, assets, 6.0, 1.0, None);
}
pub fn draw_scene_at(
    painter: &egui::Painter,
    stage: Rect,
    frame: &Frame,
    assets: &AssetCache,
    age: f32,
    opacity: f32,
    player: Option<(&crate::effects::Player, bool)>,
) {
    let scale = (stage.width() / 1280.0).min(stage.height() / 720.0);
    let painter = painter.with_clip_rect(painter.clip_rect().intersect(stage));
    painter.rect_filled(
        stage,
        0.0,
        Color32::from_rgb(23, 30, 41).gamma_multiply(opacity),
    );
    if let Some(id) = &frame.visual.background {
        let effect = &frame.visual.background_effect;
        let mut motion = crate::effects::motion(effect.animation, age);
        motion.offset[0] *= scale;
        motion.offset[1] *= scale;
        crate::effects::render::image(
            &painter,
            assets,
            crate::effects::render::Image {
                id,
                bounds: stage,
                fit: "cover",
                tint: Color32::WHITE,
                effect,
                motion,
                opacity,
            },
        );
    }
    if let Some((player, false)) = player
        && !player.active()
    {
        for d in &player.departing {
            let portrait = runtime_portrait(&d.portrait, assets, true);
            paint_portrait(
                &painter,
                stage,
                assets,
                &portrait,
                crate::effects::departing_motion(d.portrait.pose, d.animation, d.age),
                opacity,
                6.0,
            );
        }
    }
    for portrait in &frame.visual.portraits {
        let actor_motion = player
            .map(|(p, old)| p.portrait_motion(portrait, old))
            .unwrap_or_else(|| crate::effects::pose_motion(portrait.pose));
        let portrait = runtime_portrait(portrait, assets, player.is_some_and(|(_, old)| old));
        paint_portrait(
            &painter,
            stage,
            assets,
            &portrait,
            actor_motion,
            opacity,
            age,
        );
    }
}
fn runtime_portrait<'a>(
    p: &'a crate::model::Portrait,
    assets: &AssetCache,
    old: bool,
) -> std::borrow::Cow<'a, crate::model::Portrait> {
    let id = if old {
        crate::live2d::old_texture_id(p.slot)
    } else {
        crate::live2d::texture_id(p.slot)
    };
    if p.live2d.is_some() && assets.has_stream(id) {
        let mut instance = p.clone();
        instance.image = id.into();
        std::borrow::Cow::Owned(instance)
    } else {
        std::borrow::Cow::Borrowed(p)
    }
}
fn paint_portrait(
    painter: &egui::Painter,
    stage: Rect,
    assets: &AssetCache,
    portrait: &crate::model::Portrait,
    actor_motion: crate::effects::Motion,
    opacity: f32,
    age: f32,
) {
    let scale = (stage.width() / 1280.0).min(stage.height() / 720.0);
    if let Some(texture) = assets.image(&portrait.image) {
        let size = texture.size_vec2();
        let factor = (540.0 * scale / size.y).min(430.0 * scale / size.x);
        let x = match portrait.slot {
            Slot::Left => 300.0,
            Slot::Center => 640.0,
            Slot::Right => 980.0,
        };
        let bottom = stage.min + Vec2::new(x, 685.0) * scale;
        let bounds = Rect::from_min_size(
            bottom - Vec2::new(size.x * factor / 2.0, size.y * factor),
            size * factor,
        );
        let mut motion =
            actor_motion.combine(crate::effects::motion(portrait.effect.animation, age));
        motion.offset[0] *= scale;
        motion.offset[1] *= scale;
        crate::effects::render::image(
            painter,
            assets,
            crate::effects::render::Image {
                id: &portrait.image,
                bounds,
                fit: "stretch",
                tint: Color32::WHITE,
                effect: &portrait.effect,
                motion,
                opacity,
            },
        );
    }
}

pub fn dialogue_job(
    text: &str,
    visible_text: &str,
    font_id: FontId,
    width: f32,
) -> egui::text::LayoutJob {
    debug_assert!(text.starts_with(visible_text));
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = width;
    let format = egui::TextFormat {
        font_id,
        color: Color32::from_rgb(241, 243, 247),
        ..Default::default()
    };
    job.append(visible_text, 0.0, format.clone());
    job.append(
        &text[visible_text.len()..],
        0.0,
        egui::TextFormat {
            color: Color32::TRANSPARENT,
            ..format
        },
    );
    job
}

fn rect(origin: Pos2, scale: f32, x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect::from_min_size(
        origin + Vec2::new(x, y) * scale,
        Vec2::new(width, height) * scale,
    )
}

/// Read-only presentation: returns at most one intent, never changes the session.
pub fn draw(
    ui: &mut egui::Ui,
    story: &Story,
    frame: &Frame,
    assets: &AssetCache,
    render: RenderState<'_>,
) -> Option<UiIntent> {
    let available = ui.max_rect();
    let scale = (available.width() / 1280.0)
        .min(available.height() / 720.0)
        .max(0.01);
    let stage = Rect::from_center_size(available.center(), Vec2::new(1280.0, 720.0) * scale);
    let origin = stage.min;
    let painter = ui.painter().with_clip_rect(stage);
    painter.rect_filled(stage, 0.0, Color32::from_rgb(23, 30, 41));
    if let Some(bg) = frame
        .visual
        .background
        .as_ref()
        .and_then(|id| assets.image(id))
    {
        let size = bg.size_vec2();
        let cover = (stage.width() / size.x).max(stage.height() / size.y);
        let target = Rect::from_center_size(stage.center(), size * cover);
        painter.image(
            bg.id(),
            target,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    for portrait in &frame.visual.portraits {
        if let Some(image) = assets.image(&portrait.image) {
            let size = image.size_vec2();
            let factor = (540.0 * scale / size.y).min(430.0 * scale / size.x);
            let x = match portrait.slot {
                Slot::Left => 300.0,
                Slot::Center => 640.0,
                Slot::Right => 980.0,
            };
            let bottom = origin + Vec2::new(x, 685.0) * scale;
            let target = Rect::from_min_size(
                bottom - Vec2::new(size.x * factor / 2.0, size.y * factor),
                size * factor,
            );
            painter.image(
                image.id(),
                target,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }
    painter.text(
        origin + Vec2::new(44.0, 40.0) * scale,
        Align2::LEFT_CENTER,
        "K A K E R A  /  DEMO",
        FontId::proportional(17.0 * scale),
        Color32::from_rgb(238, 221, 184),
    );
    let dialogue = rect(origin, scale, 36.0, 505.0, 1208.0, 180.0);
    painter.rect_filled(
        dialogue,
        10.0 * scale,
        Color32::from_rgba_unmultiplied(12, 20, 34, 238),
    );
    painter.rect_stroke(
        dialogue,
        10.0 * scale,
        Stroke::new(scale, Color32::from_rgb(129, 149, 163)),
        egui::StrokeKind::Inside,
    );
    if let Some(name) = frame
        .speaker
        .as_ref()
        .and_then(|id| story.characters.get(id))
    {
        painter.text(
            origin + Vec2::new(64.0, 533.0) * scale,
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(22.0 * scale),
            Color32::from_rgb(240, 202, 137),
        );
    }
    // Lay out the full sentence with an invisible suffix, preserving final line breaks.
    let job = dialogue_job(
        &frame.text,
        render.visible_text,
        FontId::proportional(26.0 * scale),
        1146.0 * scale,
    );
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job.clone()));
    let overflow = galley.size().y > 78.0 * scale;
    if overflow {
        painter.text(
            origin + Vec2::new(350.0, 533.0) * scale,
            Align2::LEFT_CENTER,
            "台词超出对话框，请拆句（可滚动查看）",
            FontId::proportional(18.0 * scale),
            Color32::LIGHT_RED,
        );
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(rect(origin, scale, 64.0, 554.0, 1146.0, 78.0)),
            |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("dialogue")
                    .show(ui, |ui| {
                        ui.label(job);
                    });
            },
        );
    } else {
        painter.galley(
            origin + Vec2::new(64.0, 554.0) * scale,
            galley,
            Color32::WHITE,
        );
    }
    let mut intent = None;
    let mut button = |ui: &mut egui::Ui, bounds: Rect, text: &str, action: UiIntent| {
        let widget = egui::Button::new(RichText::new(text).size(18.0 * scale));
        if ui.put(bounds, widget).clicked() && !render.blocked {
            intent = Some(action);
        }
    };
    button(
        ui,
        rect(origin, scale, 896.0, 20.0, 88.0, 38.0),
        "保存",
        UiIntent::Save,
    );
    button(
        ui,
        rect(origin, scale, 994.0, 20.0, 88.0, 38.0),
        "读档",
        UiIntent::Load,
    );
    button(
        ui,
        rect(origin, scale, 1092.0, 20.0, 152.0, 38.0),
        "重新开始",
        UiIntent::Restart,
    );
    button(
        ui,
        rect(origin, scale, 700.0, 20.0, 88.0, 38.0),
        "历史",
        UiIntent::History,
    );
    button(
        ui,
        rect(origin, scale, 798.0, 20.0, 88.0, 38.0),
        "设置",
        UiIntent::Settings,
    );
    if !render.complete {
        button(
            ui,
            rect(origin, scale, 1094.0, 637.0, 120.0, 34.0),
            "补全",
            UiIntent::Play(Intent::Advance),
        );
    } else {
        match &story.routes[&frame.id] {
            Route::Next(_) | Route::Branch { .. } | Route::Script(ScriptRoute::Advance) => {
                button(
                    ui,
                    rect(origin, scale, 1094.0, 637.0, 120.0, 34.0),
                    "继续  >",
                    UiIntent::Play(Intent::Advance),
                );
            }
            Route::End | Route::Script(ScriptRoute::End) => {
                painter.text(
                    origin + Vec2::new(1210.0, 656.0) * scale,
                    Align2::RIGHT_CENTER,
                    "已结束",
                    FontId::proportional(19.0 * scale),
                    Color32::from_rgb(240, 202, 137),
                );
            }
            Route::Choice(_) | Route::Script(ScriptRoute::Choice) => {
                painter.text(
                    origin + Vec2::new(1210.0, 656.0) * scale,
                    Align2::RIGHT_CENTER,
                    "请选择",
                    FontId::proportional(19.0 * scale),
                    Color32::from_rgb(240, 202, 137),
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(rect(origin, scale, 804.0, 160.0, 416.0, 315.0)),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("choices")
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 12.0 * scale;
                                if render.options.is_empty() {
                                    ui.colored_label(
                                        Color32::YELLOW,
                                        "当前没有可用选项，请读档或重新开始",
                                    );
                                }
                                for option in render.options {
                                    let response = ui.add_sized(
                                        [396.0 * scale, 48.0 * scale],
                                        egui::Button::new(
                                            RichText::new(&option.text).size(22.0 * scale),
                                        )
                                        .wrap(),
                                    );
                                    if response.clicked() && !render.blocked {
                                        intent =
                                            Some(UiIntent::Play(Intent::Choose(option.id.clone())));
                                    }
                                }
                            });
                    },
                );
            }
        }
    }
    // Do not combine Space with a click in the same update. Choices never consume Advance.
    if intent.is_none()
        && !render.blocked
        && (!render.complete
            || matches!(
                story.routes[&frame.id],
                Route::Next(_) | Route::Branch { .. } | Route::Script(ScriptRoute::Advance)
            ))
        && ui.input(|i| i.key_pressed(egui::Key::Space))
    {
        intent = Some(UiIntent::Play(Intent::Advance));
    }
    intent
}
