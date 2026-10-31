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
        VariableValue::Int(_) => text.parse().map(VariableValue::Int).map_err(|_| {
            error(
                file,
                node,
                crate::i18n::format("host-loader-variables-0001", &[("arg0", text.to_string())]),
            )
        }),
        VariableValue::Bool(_) => match text {
            "true" => Ok(VariableValue::Bool(true)),
            "false" => Ok(VariableValue::Bool(false)),
            _ => Err(error(
                file,
                node,
                crate::i18n::text("host-loader-variables-0004"),
            )),
        },
        VariableValue::String(_) if text.len() <= MAX_STRING_BYTES => {
            Ok(VariableValue::String(text.to_owned()))
        }
        _ => Err(error(
            file,
            node,
            crate::i18n::text("host-loader-variables-0005"),
        )),
    }
}

fn value_attribute<'a>(file: &str, node: Node<'a, '_>, name: &str) -> Result<&'a str> {
    node.attribute(name).ok_or_else(|| {
        error(
            file,
            node,
            crate::i18n::format("host-loader-variables-0002", &[("arg0", name.to_string())]),
        )
    })
}

pub fn variables(
    root: Node<'_, '_>,
    catalog: &crate::localization::Catalog,
) -> Result<BTreeMap<String, VariableDecl>> {
    parse_variables(root, "story.xml", Some(catalog))
}
pub fn variables_in(root: Node<'_, '_>, file: &str) -> Result<BTreeMap<String, VariableDecl>> {
    parse_variables(root, file, None)
}
fn parse_variables(
    root: Node<'_, '_>,
    file: &str,
    catalog: Option<&crate::localization::Catalog>,
) -> Result<BTreeMap<String, VariableDecl>> {
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
        let mut attributes = attributes.to_vec();
        if catalog.is_some() {
            attributes.push("label-key");
        }
        check(file, node, tag, &attributes, &[], false)?;
        let id = required(file, node, "id")?;
        if id.len() > 64
            || !id.bytes().enumerate().all(|(index, ch)| {
                ch == b'_' || ch.is_ascii_alphabetic() || (index > 0 && ch.is_ascii_digit())
            })
        {
            return Err(error(
                file,
                node,
                crate::i18n::text("host-loader-variables-0006"),
            ));
        }
        let kind = match tag {
            "int" => VariableValue::Int(0),
            "bool" => VariableValue::Bool(false),
            "string" => VariableValue::String(String::new()),
            _ => unreachable!(),
        };
        let initial = literal(file, node, &kind, value_attribute(file, node, "initial")?)?;
        let bound = |name: &str| -> Result<Option<i64>> {
            node.attribute(name)
                .map(|text| {
                    text.parse().map_err(|_| {
                        error(
                            file,
                            node,
                            crate::i18n::format(
                                "host-loader-variables-0003",
                                &[("arg0", name.to_string())],
                            ),
                        )
                    })
                })
                .transpose()
        };
        let min = bound("min")?;
        let max = bound("max")?;
        if matches!((min,max), (Some(min),Some(max)) if min > max) {
            return Err(error(
                file,
                node,
                crate::i18n::text("host-loader-variables-0007"),
            ));
        }
        let label = if node.attribute("label-key").is_some() {
            super::localized_attribute(
                file,
                node,
                "label",
                "label-key",
                catalog.expect("story catalog"),
            )?
        } else {
            node.attribute("label").unwrap_or(id).to_owned()
        };
        if label.trim().is_empty() || label.len() > 256 {
            return Err(error(
                file,
                node,
                crate::i18n::text("host-loader-variables-0008"),
            ));
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
            return Err(error(
                file,
                node,
                crate::i18n::text("host-loader-variables-0009"),
            ));
        }
    }
    Ok(variables)
}
