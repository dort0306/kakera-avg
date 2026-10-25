use crate::{
    assets::AssetCache,
    model::{Frame, Slot},
};
use eframe::egui::{self, Color32, FontId, Rect, Vec2};

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
                region: None,
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
                region: None,
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
