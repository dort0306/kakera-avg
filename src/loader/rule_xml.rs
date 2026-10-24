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

fn condition(
    file: &str,
    node: Node<'_, '_>,
    variables: &BTreeMap<String, VariableDecl>,
    depth: usize,
    count: &mut usize,
) -> Result<Condition> {
    *count += 1;
    if depth > 16 || *count > 128 {
        return Err(error(file, node, "条件超过最大深度 16 或节点数 128"));
    }
    let tag = node.tag_name().name();
    match tag {
        "cmp" => {
            check(file, node, "cmp", &["var", "op", "value"], &[], false)?;
            let variable = required(file, node, "var")?;
            let decl = variables
                .get(variable)
                .ok_or_else(|| error(file, node, format!("未知变量：{variable}")))?;
            let op = match required(file, node, "op")? {
                "eq" => CompareOp::Eq,
                "ne" => CompareOp::Ne,
                "lt" => CompareOp::Lt,
                "le" => CompareOp::Le,
                "gt" => CompareOp::Gt,
                "ge" => CompareOp::Ge,
                text => return Err(error(file, node, format!("未知比较操作：{text}"))),
            };
            if !matches!(decl.initial, VariableValue::Int(_))
                && !matches!(op, CompareOp::Eq | CompareOp::Ne)
            {
                return Err(error(file, node, "bool/string 仅支持 eq/ne 比较"));
            }
            let value = literal(
                file,
                node,
                &decl.initial,
                value_attribute(file, node, "value")?,
            )?;
            Ok(Condition::Compare {
                variable: variable.to_owned(),
                op,
                value,
            })
        }
        "all" | "any" | "not" => {
            check(file, node, tag, &[], &["cmp", "all", "any", "not"], false)?;
            let children: Vec<_> = elements(node).collect();
            if children.is_empty() || (tag == "not" && children.len() != 1) {
                return Err(error(
                    file,
                    node,
                    "all/any 不能为空，not 必须恰好有一个条件",
                ));
            }
            let mut parsed = Vec::new();
            for child in children {
                parsed.push(condition(file, child, variables, depth + 1, count)?);
            }
            Ok(match tag {
                "all" => Condition::All(parsed),
                "any" => Condition::Any(parsed),
                _ => Condition::Not(Box::new(parsed.remove(0))),
            })
        }
        _ => Err(error(file, node, format!("未知条件元素：{tag}"))),
    }
}

#[derive(Clone, Copy)]
pub enum Guard {
    Forbidden,
    Optional,
    Required,
}

pub fn transition(
    file: &str,
    node: Node<'_, '_>,
    variables: &BTreeMap<String, VariableDecl>,
    guard: Guard,
) -> Result<TransitionRule> {
    let when_exists = node.children().any(|node| node.has_tag_name("when"));
    if matches!(guard, Guard::Forbidden) && when_exists {
        return Err(error(file, node, "此流转不能有 when"));
    }
    let when = if when_exists || matches!(guard, Guard::Required) {
        let when = single(file, node, "when")?;
        check(
            file,
            when,
            "when",
            &[],
            &["cmp", "all", "any", "not"],
            false,
        )?;
        let children: Vec<_> = elements(when).collect();
        if children.len() != 1 {
            return Err(error(file, when, "when 必须恰好包含一个条件"));
        }
        Some(condition(file, children[0], variables, 1, &mut 0)?)
    } else {
        None
    };
    let mut effects = Vec::new();
    if node.children().any(|node| node.has_tag_name("effects")) {
        let parent = single(file, node, "effects")?;
        check(file, parent, "effects", &[], &["set", "add"], false)?;
        for effect in elements(parent) {
            let tag = effect.tag_name().name();
            check(file, effect, tag, &["var", "value"], &[], false)?;
            let id = required(file, effect, "var")?;
            let decl = variables
                .get(id)
                .ok_or_else(|| error(file, effect, format!("未知变量：{id}")))?;
            let value = literal(
                file,
                effect,
                &decl.initial,
                value_attribute(file, effect, "value")?,
            )?;
            let parsed = if tag == "set" {
                rules::validate_value(decl, &value)
                    .map_err(|message| error(file, effect, message))?;
                Effect::Set {
                    variable: id.to_owned(),
                    value,
                }
            } else {
                let VariableValue::Int(amount) = value else {
                    return Err(error(file, effect, "add 仅支持 int 变量"));
                };
                Effect::Add {
                    variable: id.to_owned(),
                    amount,
                }
            };
            effects.push(parsed);
            if effects.len() > 128 {
                return Err(error(file, effect, "更新列表超过 128 项"));
            }
        }
    }
    Ok(TransitionRule {
        to: required(file, node, "to")?.to_owned(),
        when,
        effects,
    })
}
