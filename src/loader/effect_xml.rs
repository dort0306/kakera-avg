use super::{check, elements, error, required, single};
use crate::model::effects::*;
use anyhow::Result;
use roxmltree::Node;
use std::collections::BTreeMap;

fn number(
    file: &str,
    node: Node<'_, '_>,
    key: &str,
    default: f32,
    min: f32,
    max: f32,
) -> Result<f32> {
    let value = node
        .attribute(key)
        .map(str::parse::<f32>)
        .transpose()
        .map_err(|_| error(file, node, format!("{key} 必须为数值")))?
        .unwrap_or(default);
    if !value.is_finite() || !(min..=max).contains(&value) {
        return Err(error(file, node, format!("{key} 超出 {min}–{max}")));
    }
    Ok(value)
}
fn integer(
    file: &str,
    node: Node<'_, '_>,
    key: &str,
    default: u32,
    min: u32,
    max: u32,
) -> Result<u32> {
    let value = number(file, node, key, default as f32, min as f32, max as f32)?;
    if value.fract() != 0.0 {
        return Err(error(file, node, format!("{key} 需要整数")));
    }
    Ok(value as u32)
}
fn enum_value<T: serde::de::DeserializeOwned>(
    file: &str,
    node: Node<'_, '_>,
    key: &str,
    default: &str,
) -> Result<T> {
    serde_json::from_value(serde_json::json!(node.attribute(key).unwrap_or(default)))
        .map_err(|_| error(file, node, format!("未知 {key}")))
}
fn boolean(file: &str, node: Node<'_, '_>, key: &str) -> Result<bool> {
    match node.attribute(key) {
        None | Some("false") => Ok(false),
        Some("true") => Ok(true),
        _ => Err(error(file, node, format!("{key} 需要 true/false"))),
    }
}
pub fn portrait(
    file: &str,
    node: Node<'_, '_>,
) -> Result<(PortraitPose, PortraitAnimation, PortraitAnimation)> {
    let pose = enum_value(file, node, "pose", "normal")?;
    let enter = PortraitAnimation {
        kind: enum_value(file, node, "enter", "none")?,
        duration: integer(file, node, "duration", 600, 0, 5000)?,
        strength: number(file, node, "strength", 32.0, 0.0, 128.0)?,
        easing: enum_value(file, node, "easing", "ease-in-out")?,
    };
    let exit = PortraitAnimation {
        kind: enum_value(file, node, "exit", "none")?,
        ..enter
    };
    if !enter.kind.accepts(pose) {
        return Err(error(file, node, "立绘 enter 与最终 pose 不兼容"));
    }
    if exit.kind != PortraitActionKind::None && !exit.kind.is_exit() {
        return Err(error(
            file,
            node,
            "exit 仅允许 none/fade-out/exit-left/right/up/down",
        ));
    }
    Ok((pose, enter, exit))
}
pub fn transition(file: &str, node: Node<'_, '_>) -> Result<Transition> {
    check(
        file,
        node,
        "transition",
        &["kind", "duration", "easing"],
        &[],
        false,
    )?;
    required(file, node, "kind")?;
    Ok(Transition {
        kind: enum_value(file, node, "kind", "none")?,
        duration: integer(file, node, "duration", 600, 0, 5000)?,
        easing: enum_value(file, node, "easing", "ease-in-out")?,
    })
}
pub fn presets(root: Node<'_, '_>) -> Result<BTreeMap<String, ImageEffect>> {
    let mut result = BTreeMap::new();
    if !elements(root).any(|n| n.has_tag_name("effects")) {
        return Ok(result);
    }
    let group = single("story.xml", root, "effects")?;
    check("story.xml", group, "effects", &[], &["image-effect"], false)?;
    for node in elements(group) {
        let file = "story.xml";
        check(
            file,
            node,
            "image-effect",
            &[
                "id",
                "filter",
                "blur",
                "pixel-size",
                "opacity",
                "tint",
                "scale",
                "rotation",
                "flip-x",
                "flip-y",
                "animation",
                "duration",
                "strength",
                "easing",
            ],
            &[],
            false,
        )?;
        let id = required(file, node, "id")?.to_owned();
        let filter = match node.attribute("filter").unwrap_or("none") {
            "none" => Filter::None,
            "grayscale" => Filter::Grayscale,
            "sepia" => Filter::Sepia,
            "blur" => Filter::Blur(integer(file, node, "blur", 4, 1, 12)? as u8),
            "pixelate" => Filter::Pixelate(integer(file, node, "pixel-size", 16, 1, 64)? as u8),
            _ => return Err(error(file, node, "未知图片 filter")),
        };
        if node.attribute("blur").is_some() && !matches!(filter, Filter::Blur(_))
            || node.attribute("pixel-size").is_some() && !matches!(filter, Filter::Pixelate(_))
        {
            return Err(error(file, node, "blur/pixel-size 仅可用于对应 filter"));
        }
        let mut tint = [255; 4];
        if let Some(text) = node.attribute("tint") {
            let hex = text
                .strip_prefix('#')
                .filter(|s| s.is_ascii() && [6, 8].contains(&s.len()))
                .ok_or_else(|| error(file, node, "tint 需要 #RRGGBB/#RRGGBBAA"))?;
            for (i, v) in tint.iter_mut().enumerate().take(hex.len() / 2) {
                *v = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
                    .map_err(|_| error(file, node, "tint 格式错误"))?;
            }
        }
        let effect = ImageEffect {
            filter,
            tint,
            opacity: number(file, node, "opacity", 1.0, 0.0, 1.0)?,
            scale: number(file, node, "scale", 1.0, 0.25, 4.0)?,
            rotation: number(file, node, "rotation", 0.0, -360.0, 360.0)?,
            flip_x: boolean(file, node, "flip-x")?,
            flip_y: boolean(file, node, "flip-y")?,
            animation: Animation {
                kind: enum_value(file, node, "animation", "none")?,
                duration: integer(file, node, "duration", 400, 0, 5000)?,
                strength: number(file, node, "strength", 12.0, 0.0, 128.0)?,
                easing: enum_value(file, node, "easing", "ease-in-out")?,
            },
        };
        if result.insert(id.clone(), effect).is_some() || result.len() > 64 {
            return Err(error(file, node, format!("重复效果或超过 64 项：{id}")));
        }
    }
    Ok(result)
}
pub fn reference(
    file: &str,
    node: Node<'_, '_>,
    key: &str,
    presets: &BTreeMap<String, ImageEffect>,
) -> Result<ImageEffect> {
    node.attribute(key)
        .map(|id| {
            presets
                .get(id)
                .cloned()
                .ok_or_else(|| error(file, node, format!("未知图片效果：{id}")))
        })
        .unwrap_or_else(|| Ok(ImageEffect::default()))
}
