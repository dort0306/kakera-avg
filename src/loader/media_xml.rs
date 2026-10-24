use super::{check, elements, error, required, single};
use crate::model::{Asset, AssetKind, Live2DPose, VideoCue};
use anyhow::Result;
use roxmltree::Node;
use std::collections::BTreeMap;
fn reference(
    node: Node<'_, '_>,
    assets: &BTreeMap<String, Asset>,
    kind: AssetKind,
) -> Result<String> {
    let id = required("story.xml", node, "asset")?;
    if !assets.get(id).is_some_and(|a| a.kind == kind) {
        return Err(error("story.xml", node, "媒体资源不存在或类型错误"));
    }
    Ok(id.into())
}
fn boolean(node: Node<'_, '_>, key: &str, default: bool) -> Result<bool> {
    match node.attribute(key) {
        None => Ok(default),
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        _ => Err(error("story.xml", node, format!("{key} 需要 true/false"))),
    }
}
pub fn video(frame: Node<'_, '_>, assets: &BTreeMap<String, Asset>) -> Result<Option<VideoCue>> {
    if !elements(frame).any(|n| n.has_tag_name("video")) {
        return Ok(None);
    }
    let n = single("story.xml", frame, "video")?;
    check(
        "story.xml",
        n,
        "video",
        &["asset", "skippable", "advance"],
        &[],
        false,
    )?;
    if elements(frame).any(|n| n.has_tag_name("options")) {
        return Err(error("story.xml", n, "视频场景不能含选项"));
    }
    Ok(Some(VideoCue {
        asset: reference(n, assets, AssetKind::Video)?,
        skippable: boolean(n, "skippable", true)?,
        advance: boolean(n, "advance", true)?,
    }))
}
pub fn live2d(p: Node<'_, '_>, assets: &BTreeMap<String, Asset>) -> Result<Option<Live2DPose>> {
    if !elements(p).any(|n| n.has_tag_name("live2d")) {
        return Ok(None);
    }
    let n = single("story.xml", p, "live2d")?;
    check(
        "story.xml",
        n,
        "live2d",
        &["asset", "motion", "expression"],
        &["parameter"],
        false,
    )?;
    let mut parameters = BTreeMap::new();
    for param in elements(n) {
        check(
            "story.xml",
            param,
            "parameter",
            &["id", "value"],
            &[],
            false,
        )?;
        let id = required("story.xml", param, "id")?.to_owned();
        let value = required("story.xml", param, "value")?
            .parse::<f32>()
            .map_err(|_| error("story.xml", param, "参数需要数值"))?;
        if !value.is_finite()
            || value.abs() > 10000.0
            || parameters.insert(id, value).is_some()
            || parameters.len() > 128
        {
            return Err(error("story.xml", param, "参数重复、越界或超过 128 项"));
        }
    }
    Ok(Some(Live2DPose {
        model: reference(n, assets, AssetKind::Live2D)?,
        motion: n.attribute("motion").map(str::to_owned),
        expression: n.attribute("expression").map(str::to_owned),
        parameters,
    }))
}
