use super::{check, elements, error, required, single, unique};
use crate::model::*;
use anyhow::Result;
use roxmltree::Node;
use std::collections::BTreeMap;

pub fn literal(
    file: &str,
    node: Node<'_, '_>,
    kind: &VariableValue,
    text: &str,
) -> Result<VariableValue> {
    match kind {
        VariableValue::Int(_) => text
            .parse()
            .map(VariableValue::Int)
            .map_err(|_| error(file, node, format!("整数常量无效：{text}"))),
        VariableValue::Bool(_) => match text {
            "true" => Ok(VariableValue::Bool(true)),
            "false" => Ok(VariableValue::Bool(false)),
            _ => Err(error(file, node, "布尔常量仅接受 true/false")),
        },
        VariableValue::String(_) if text.len() <= MAX_STRING_BYTES => {
            Ok(VariableValue::String(text.to_owned()))
        }
        _ => Err(error(file, node, "字符串超过 4096 字节")),
    }
}

fn value_attribute<'a>(file: &str, node: Node<'a, '_>, name: &str) -> Result<&'a str> {
    node.attribute(name)
        .ok_or_else(|| error(file, node, format!("缺少属性 {name}")))
}

pub fn variables(root: Node<'_, '_>) -> Result<BTreeMap<String, VariableDecl>> {
    variables_in(root, "story.xml")
}
pub fn variables_in(root: Node<'_, '_>, file: &str) -> Result<BTreeMap<String, VariableDecl>> {
    let mut variables = BTreeMap::new();
    if !root.children().any(|node| node.has_tag_name("variables")) {
        return Ok(variables);
    }
    let parent = single(file, root, "variables")?;
    check(
        file,
        parent,
        "variables",
        &[],
        &["int", "bool", "string"],
        false,
    )?;
    for node in elements(parent) {
        let tag = node.tag_name().name();
        let attributes: &[&str] = if tag == "int" {
            &["id", "label", "initial", "min", "max"]
        } else {
            &["id", "label", "initial"]
        };
        check(file, node, tag, attributes, &[], false)?;
        let id = required(file, node, "id")?;
        if id.len() > 64
            || !id.bytes().enumerate().all(|(index, ch)| {
                ch == b'_' || ch.is_ascii_alphabetic() || (index > 0 && ch.is_ascii_digit())
            })
        {
            return Err(error(
                file,
                node,
                "变量 ID 必须为 1–64 个 ASCII 字母/数字/下划线，不能以数字开头",
            ));
        }
        let kind = match tag {
            "int" => VariableValue::Int(0),
            "bool" => VariableValue::Bool(false),
            "string" => VariableValue::String(String::new()),
            _ => unreachable!(),
        };
        let initial = literal(file, node, &kind, value_attribute(file, node, "initial")?)?;
        let bound = |name| -> Result<Option<i64>> {
            node.attribute(name)
                .map(|text| {
                    text.parse()
                        .map_err(|_| error(file, node, format!("{name} 必须为 i64 整数")))
                })
                .transpose()
        };
        let min = bound("min")?;
        let max = bound("max")?;
        if matches!((min,max), (Some(min),Some(max)) if min > max) {
            return Err(error(file, node, "min 不能大于 max"));
        }
        let label = node.attribute("label").unwrap_or(id).to_owned();
        if label.trim().is_empty() || label.len() > 256 {
            return Err(error(file, node, "变量 label 必须非空且最多 256 字节"));
        }
        let decl = VariableDecl {
            initial,
            min,
            max,
            label,
        };
        rules::validate_value(&decl, &decl.initial)
            .map_err(|message| error(file, node, message))?;
        unique(file, node, &mut variables, id, decl)?;
        if variables.len() > MAX_VARIABLES {
            return Err(error(file, node, "变量数量超过 128"));
        }
    }
    Ok(variables)
}
