use super::{Motion, Player, blend, ease, progress};
use crate::{assets::AssetCache, model::effects::ImageEffect};
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};

pub struct Image<'a> {
    pub region: Option<crate::image_layout::Region>,
    pub id: &'a str,
    pub bounds: Rect,
    pub fit: &'a str,
    pub tint: Color32,
    pub effect: &'a ImageEffect,
    pub motion: Motion,
    pub opacity: f32,
}
pub fn image(painter: &egui::Painter, assets: &AssetCache, input: Image<'_>) {
    let Image {
        region,
        id,
        bounds,
        fit,
        tint,
        effect,
        motion,
        opacity,
    } = input;
    let Some(texture) = assets.effected(id, effect.filter) else {
        return;
    };
    let texture_size = texture.size_vec2();
    let uv = region
        .map(|r| r.uv([texture_size.x, texture_size.y]))
        .unwrap_or([0.0, 0.0, 1.0, 1.0]);
    let source = region
        .map(|r| Vec2::new(r.0[2] as f32, r.0[3] as f32))
        .unwrap_or(texture_size);
    let factor = if fit == "cover" {
        (bounds.width() / source.x).max(bounds.height() / source.y)
    } else {
        (bounds.width() / source.x).min(bounds.height() / source.y)
    };
    let mut size = if fit == "stretch" {
        bounds.size()
    } else {
        source * factor
    } * effect.scale
        * motion.scale;
    size.x *= motion.scale_x;
    let center = bounds.center() + Vec2::from(motion.offset);
    let rgba = tint.to_array();
    let mut channels = [0; 4];
    for i in 0..4 {
        channels[i] = (u16::from(rgba[i]) * u16::from(effect.tint[i]) / 255) as u8;
    }
    channels[3] =
        (channels[3] as f32 * effect.opacity * motion.opacity * opacity).clamp(0.0, 255.0) as u8;
    let color = Color32::from_rgba_unmultiplied(channels[0], channels[1], channels[2], channels[3]);
    let angle = (effect.rotation + motion.rotation).to_radians();
    let (sin, cos) = angle.sin_cos();
    let mut mesh = egui::epaint::Mesh::with_texture(texture.id());
    for [x, y] in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
        let delta = Vec2::new((x - 0.5) * size.x, (y - 0.5) * size.y);
        mesh.vertices.push(egui::epaint::Vertex {
            pos: center + Vec2::new(delta.x * cos - delta.y * sin, delta.x * sin + delta.y * cos),
            uv: Pos2::new(
                uv[0] + (uv[2] - uv[0]) * if effect.flip_x { 1.0 - x } else { x },
                uv[1] + (uv[3] - uv[1]) * if effect.flip_y { 1.0 - y } else { y },
            ),
            color,
        });
    }
    mesh.indices.extend([0, 1, 2, 0, 2, 3]);
    painter.add(egui::Shape::mesh(mesh));
    if motion.flash > 0.0 {
        painter.rect_filled(
            bounds,
            0.0,
            Color32::WHITE.gamma_multiply(motion.flash * opacity * effect.opacity),
        );
    }
}
pub fn scene(
    painter: &egui::Painter,
    bounds: Rect,
    frame: &crate::model::Frame,
    assets: &AssetCache,
    player: &Player,
    motion: Motion,
) {
    let size = bounds.size();
    let transformed = Rect::from_center_size(
        bounds.center() + Vec2::from(motion.offset),
        size * motion.scale,
    );
    let composition = blend(
        player.transition.kind,
        ease(
            progress(player.age, player.transition.duration),
            player.transition.easing,
        ),
    );
    for layer in composition.layers {
        let clip = Rect::from_min_max(
            bounds.min + size * Vec2::new(layer.clip[0], layer.clip[1]),
            bounds.min + size * Vec2::new(layer.clip[2], layer.clip[3]),
        );
        let painter = painter.with_clip_rect(painter.clip_rect().intersect(clip));
        let area = transformed.translate(size * Vec2::from(layer.offset));
        let selected = if layer.old {
            player.previous.as_ref()
        } else {
            Some(frame)
        };
        if let Some(frame) = selected {
            crate::view::draw_scene_at(
                &painter,
                area,
                frame,
                assets,
                if layer.old { 6.0 } else { player.age },
                layer.opacity * motion.opacity,
                Some((player, layer.old)),
            );
        } else {
            painter.rect_filled(area, 0.0, Color32::from_rgb(23, 30, 41));
        }
    }
    if let Some((color, alpha)) = composition.veil {
        painter.rect_filled(
            bounds,
            0.0,
            Color32::from_rgb(color[0], color[1], color[2]).gamma_multiply(alpha),
        );
    }
    if motion.flash > 0.0 {
        painter.rect_filled(bounds, 0.0, Color32::WHITE.gamma_multiply(motion.flash));
    }
    if frame.video.is_some() {
        painter.rect_filled(bounds, 0.0, Color32::BLACK);
        if let Some(texture) = assets.image("$video") {
            let size = texture.size_vec2();
            let scale = (bounds.width() / size.x).min(bounds.height() / size.y);
            painter
                .with_clip_rect(painter.clip_rect().intersect(bounds))
                .image(
                    texture.id(),
                    Rect::from_center_size(bounds.center(), size * scale),
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
        }
    }
}
