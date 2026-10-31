//! Strict `<particles>` authoring contract. Parsed once at load time; the drawing
//! path only reads the resulting emitter and its prepared particles.
use crate::{loader::error, model::Story, particles};
use anyhow::Result;
use roxmltree::Node;

/// Attributes owned by the emitter node beyond the shared id/rect/tint ones.
pub const ATTRIBUTES: [&str; 18] = [
    "image",
    "rate",
    "burst",
    "max",
    "life",
    "life-max",
    "size",
    "size-max",
    "vx",
    "vx-max",
    "vy",
    "vy-max",
    "accel-x",
    "accel-y",
    "spin",
    "spin-max",
    "fade",
    "autostart",
];
/// Continuous rate ceiling in particles per second.
const MAX_RATE: f32 = 200.0;

fn invalid(file: &str, node: Node<'_, '_>, key: &str, min: f32, max: f32) -> anyhow::Error {
    error(
        file,
        node,
        crate::i18n::format(
            "host-interface-particles-0002",
            &[("arg0", key.to_string()), ("arg1", format!("{min}–{max}"))],
        ),
    )
}
fn count(file: &str, node: Node<'_, '_>, key: &str, default: u32, max: u32) -> Result<u32> {
    let Some(text) = node.attribute(key) else {
        return Ok(default);
    };
    let value = text.parse::<u32>().map_err(|_| {
        error(
            file,
            node,
            crate::i18n::format(
                "host-interface-particles-0001",
                &[("arg0", key.to_string()), ("arg1", max.to_string())],
            ),
        )
    })?;
    if value > max {
        return Err(error(
            file,
            node,
            crate::i18n::format(
                "host-interface-particles-0001",
                &[("arg0", key.to_string()), ("arg1", max.to_string())],
            ),
        ));
    }
    Ok(value)
}
fn scalar(
    file: &str,
    node: Node<'_, '_>,
    key: &str,
    default: f32,
    min: f32,
    max: f32,
) -> Result<f32> {
    let value = super::number(file, node, key, Some(default))?;
    if !(min..=max).contains(&value) {
        return Err(invalid(file, node, key, min, max));
    }
    Ok(value)
}
fn twin(
    file: &str,
    node: Node<'_, '_>,
    low: &str,
    high: &str,
    default: [f32; 2],
    min: f32,
    max: f32,
) -> Result<[f32; 2]> {
    let first = scalar(file, node, low, default[0], min, max)?;
    let second = scalar(file, node, high, default[1], min, max)?;
    if second < first {
        return Err(error(
            file,
            node,
            crate::i18n::format(
                "host-interface-particles-0006",
                &[("arg0", high.to_string()), ("arg1", low.to_string())],
            ),
        ));
    }
    Ok([first, second])
}

/// Parses one emitter. The caller already validated the shared rect and tint, and
/// recorded the node id in the UI namespace.
pub fn parse(
    file: &str,
    node: Node<'_, '_>,
    id: &str,
    rect: [f32; 4],
    tint: [u8; 4],
    story: &Story,
) -> Result<particles::Emitter> {
    let image = super::required(file, node, "image")?.to_owned();
    super::image(file, node, &image, story)?;
    let rate = scalar(file, node, "rate", 0.0, 0.0, MAX_RATE)?;
    let burst = count(file, node, "burst", 0, particles::MAX_BURST)?;
    if rate == 0.0 && burst == 0 {
        return Err(error(
            file,
            node,
            crate::i18n::text("host-interface-particles-0004"),
        ));
    }
    let max = count(file, node, "max", 128, particles::MAX_PER_EMITTER as u32)? as usize;
    if max == 0 {
        return Err(error(
            file,
            node,
            crate::i18n::text("host-interface-particles-0005"),
        ));
    }
    let life = scalar(
        file,
        node,
        "life",
        2.0,
        particles::MIN_LIFE,
        particles::MAX_LIFE,
    )?;
    let life = twin(
        file,
        node,
        "life",
        "life-max",
        [life, life],
        particles::MIN_LIFE,
        particles::MAX_LIFE,
    )?;
    let size = scalar(
        file,
        node,
        "size",
        16.0,
        particles::MIN_SIZE,
        particles::MAX_SIZE,
    )?;
    let size = twin(
        file,
        node,
        "size",
        "size-max",
        [size, size],
        particles::MIN_SIZE,
        particles::MAX_SIZE,
    )?;
    let velocity = [
        twin(
            file,
            node,
            "vx",
            "vx-max",
            [0.0, 0.0],
            -particles::MAX_SPEED,
            particles::MAX_SPEED,
        )?,
        twin(
            file,
            node,
            "vy",
            "vy-max",
            [0.0, 0.0],
            -particles::MAX_SPEED,
            particles::MAX_SPEED,
        )?,
    ];
    let accel = [
        scalar(
            file,
            node,
            "accel-x",
            0.0,
            -particles::MAX_ACCEL,
            particles::MAX_ACCEL,
        )?,
        scalar(
            file,
            node,
            "accel-y",
            0.0,
            -particles::MAX_ACCEL,
            particles::MAX_ACCEL,
        )?,
    ];
    let spin = twin(
        file,
        node,
        "spin",
        "spin-max",
        [0.0, 0.0],
        -particles::MAX_SPIN,
        particles::MAX_SPIN,
    )?;
    Ok(particles::Emitter {
        id: id.to_owned(),
        screen: String::new(),
        image,
        rect,
        rate,
        burst,
        max,
        life,
        size,
        velocity,
        accel,
        spin,
        color: tint,
        fade: super::boolean(file, node, "fade", true)?,
        autostart: super::boolean(file, node, "autostart", false)?,
    })
}
