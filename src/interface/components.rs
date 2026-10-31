//! Load-time author composition tags. Definitions only describe host nodes; the
//! expanded tree is parsed by the same strict XML contract as handwritten UI.
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_COMPONENTS: usize = 128;
pub const MAX_PROPS: usize = 64;
pub const MAX_NODES: usize = 1024;
pub const MAX_DESCRIPTOR_BYTES: usize = 256 * 1024;
pub const MAX_TOTAL_BYTES: usize = 1024 * 1024;
pub const MAX_INSTANCES: usize = 1024;
pub const MAX_NESTING: usize = 8;
pub const MAX_DEPTH: usize = 8;
pub const MAX_ATTRS: usize = 64;
pub const MAX_CHILDREN: usize = 256;
pub const MAX_TAG_BYTES: usize = 64;
/// Host layout attributes always come from the XML instance, never from props.
pub const GENERIC: [&str; 7] = ["id", "x", "y", "width", "height", "visible", "enabled"];
/// Host tags a component may produce (plus other registered component tags).
pub const HOST_TAGS: [&str; 10] = [
    "scene",
    "panel",
    "image",
    "nine-patch",
    "label",
    "button",
    "slider",
    "toggle",
    "repeat",
    "color-picker",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropKind {
    Text,
    Bool,
    Int,
    Number,
    Binding,
    Event,
}
impl PropKind {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "string" => Self::Text,
            "bool" => Self::Bool,
            "int" => Self::Int,
            "number" => Self::Number,
            "binding" => Self::Binding,
            "event" => Self::Event,
            _ => return None,
        })
    }
    fn id(self) -> &'static str {
        match self {
            Self::Text => "string",
            Self::Bool => "bool",
            Self::Int => "int",
            Self::Number => "number",
            Self::Binding => "binding",
            Self::Event => "event",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Prop {
    pub kind: PropKind,
    pub required: bool,
    pub default: Option<Value>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub max_length: Option<usize>,
    pub value_type: Option<String>,
}
impl Prop {
    fn convert(&self, name: &str, text: &str) -> Result<Value> {
        let invalid = || {
            anyhow::anyhow!(
                "{}",
                crate::i18n::format(
                    "host-interface-components-0001",
                    &[
                        ("arg0", name.to_string()),
                        ("arg1", self.kind.id().to_string())
                    ]
                )
            )
        };
        Ok(match self.kind {
            PropKind::Text | PropKind::Binding | PropKind::Event => {
                if let Some(limit) = self.max_length
                    && text.chars().count() > limit
                {
                    anyhow::bail!(
                        "{}",
                        crate::i18n::format(
                            "host-interface-components-0002",
                            &[("arg0", name.to_string()), ("arg1", limit.to_string())]
                        )
                    );
                }
                if (self.kind == PropKind::Binding || self.kind == PropKind::Event)
                    && (text.len() > 128 || text.chars().any(char::is_control))
                {
                    return Err(invalid());
                }
                if let Some(path) = self.value_type.as_deref()
                    && !binding_value_type(text, path)
                {
                    return Err(invalid());
                }
                Value::String(text.into())
            }
            PropKind::Bool => match text {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => return Err(invalid()),
            },
            PropKind::Int => {
                let value: i64 = text.parse().map_err(|_| invalid())?;
                let number = value as f64;
                ensure!(
                    number.abs() <= 9_007_199_254_740_992.0
                        && self.min.is_none_or(|min| number >= min)
                        && self.max.is_none_or(|max| number <= max),
                    "{}",
                    invalid()
                );
                json!(value)
            }
            PropKind::Number => {
                let value: f64 = text.parse().map_err(|_| invalid())?;
                ensure!(
                    value.is_finite()
                        && self.min.is_none_or(|min| value >= min)
                        && self.max.is_none_or(|max| value <= max),
                    "{}",
                    invalid()
                );
                json!(value)
            }
        })
    }
}
/// Only the declared binding channel is accepted; the final node validation still
/// checks the referenced UI variable type at its actual use site.
fn binding_value_type(path: &str, expected: &str) -> bool {
    let path = path.strip_prefix("not:").unwrap_or(path);
    let actual = if path == "true" || path == "false" {
        Some("bool")
    } else if path.starts_with("ui.") {
        None
    } else if let Some(id) = path.strip_prefix("repeat.") {
        match id.rsplit_once('.') {
            Some((_, "page" | "pages" | "pageSize" | "total" | "start" | "end")) => Some("number"),
            Some((_, "canPrev" | "canNext")) => Some("bool"),
            Some((_, "pageText")) => Some("string"),
            _ => Some("other"),
        }
    } else {
        Some("other")
    };
    match actual {
        // A UI path is checked against the variable declaration later.
        None => true,
        Some(actual) => {
            actual == expected
                || (expected == "number" && actual == "number")
                || (expected == "string" && actual == "text")
        }
    }
}

#[derive(Clone, Debug)]
pub struct Component {
    pub name: String,
    pub props: BTreeMap<String, Prop>,
    pub location: String,
}
/// Long-lived author registrations collected before the UI is expanded.
#[derive(Clone, Debug, Default)]
pub struct Library {
    components: BTreeMap<String, Component>,
}
impl Library {
    pub fn parse(manifest: &str) -> Result<Self> {
        let value: Value = serde_json::from_str(manifest)
            .context(crate::i18n::text("host-interface-components-0003"))?;
        let entries = value.as_array().ok_or_else(|| {
            anyhow::anyhow!("{}", crate::i18n::text("host-interface-components-0003"))
        })?;
        ensure!(
            entries.len() <= MAX_COMPONENTS,
            "{}",
            crate::i18n::format(
                "host-interface-components-0004",
                &[("arg0", MAX_COMPONENTS.to_string())]
            )
        );
        let mut components = BTreeMap::new();
        for entry in entries {
            let object = entry.as_object().ok_or_else(|| {
                anyhow::anyhow!("{}", crate::i18n::text("host-interface-components-0003"))
            })?;
            ensure!(
                object
                    .keys()
                    .all(|key| matches!(key.as_str(), "name" | "props" | "expand" | "location")),
                "{}",
                crate::i18n::text("host-interface-components-0005")
            );
            let name = object.get("name").and_then(Value::as_str).ok_or_else(|| {
                anyhow::anyhow!("{}", crate::i18n::text("host-interface-components-0006"))
            })?;
            let location = object
                .get("location")
                .and_then(Value::as_str)
                .unwrap_or("<script>");
            ensure!(
                object.get("expand").and_then(Value::as_bool) == Some(true),
                "{}",
                crate::i18n::format(
                    "host-interface-components-0007",
                    &[("arg0", name.to_string()), ("arg1", location.to_string())]
                )
            );
            if !valid_tag(name) {
                return Err(anyhow::anyhow!(
                    "{}",
                    crate::i18n::format(
                        "host-interface-components-0008",
                        &[("arg0", name.to_string()), ("arg1", location.to_string())]
                    )
                ));
            }
            let props = object
                .get("props")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "{}",
                        crate::i18n::format(
                            "host-interface-components-0009",
                            &[("arg0", name.to_string())]
                        )
                    )
                })?;
            ensure!(
                props.len() <= MAX_PROPS,
                "{}",
                crate::i18n::format(
                    "host-interface-components-0010",
                    &[("arg0", name.to_string()), ("arg1", MAX_PROPS.to_string())]
                )
            );
            let mut parsed = BTreeMap::new();
            for (prop_name, spec) in props {
                let prop = parse_prop(name, prop_name, spec, location)?;
                parsed.insert(prop_name.clone(), prop);
            }
            ensure!(
                components
                    .insert(
                        name.into(),
                        Component {
                            name: name.into(),
                            props: parsed,
                            location: location.into(),
                        }
                    )
                    .is_none(),
                "{}",
                crate::i18n::format(
                    "host-interface-components-0011",
                    &[("arg0", name.to_string()), ("arg1", location.to_string())]
                )
            );
        }
        Ok(Self { components })
    }
    pub fn contains(&self, tag: &str) -> bool {
        self.components.contains_key(tag)
    }
    pub fn get(&self, tag: &str) -> Option<&Component> {
        self.components.get(tag)
    }
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }
    /// Registered tags, sorted, for strict child-name validation.
    pub fn tags(&self) -> impl Iterator<Item = &str> {
        self.components.keys().map(String::as_str)
    }
    /// Convert one XML instance into owned props and a bounded host XML fragment.
    /// `attributes` carries every instance attribute (including generic layout ones).
    pub fn expand(
        &self,
        expander: &dyn Expander,
        tag: &str,
        attributes: &[(String, String)],
        children: usize,
    ) -> Result<(String, String)> {
        let component = self.components.get(tag).ok_or_else(|| {
            anyhow::anyhow!(
                "{}",
                crate::i18n::format(
                    "host-interface-components-0012",
                    &[("arg0", tag.to_string())]
                )
            )
        })?;
        let mut values = Map::new();
        let mut seen = BTreeSet::new();
        let mut layout = BTreeMap::new();
        for (name, value) in attributes {
            if name == "id" {
                // The generated root id stays `$root` and is mapped to the resolved
                // instance id by the parser; other generic layout attributes override.
                continue;
            }
            if GENERIC.contains(&name.as_str()) {
                seen.insert(name.clone());
                layout.insert(name.clone(), value.clone());
                continue;
            }
            let Some(prop) = component.props.get(name) else {
                return Err(anyhow::anyhow!(
                    "{}",
                    crate::i18n::format(
                        "host-interface-components-0013",
                        &[("arg0", name.to_string()), ("arg1", tag.to_string())]
                    )
                ));
            };
            ensure!(
                seen.insert(name.clone()),
                "{}",
                crate::i18n::format(
                    "host-interface-components-0014",
                    &[("arg0", name.to_string())]
                )
            );
            values.insert(name.clone(), prop.convert(name, value)?);
        }
        for (name, prop) in &component.props {
            if values.contains_key(name) {
                continue;
            }
            match &prop.default {
                Some(default) => {
                    values.insert(name.clone(), default.clone());
                }
                None => anyhow::bail!(
                    "{}",
                    crate::i18n::format(
                        "host-interface-components-0015",
                        &[("arg0", tag.to_string()), ("arg1", name.to_string())]
                    )
                ),
            }
        }
        let descriptor = expander.expand(component, &Value::Object(values), children)?;
        ensure!(
            descriptor.len() <= MAX_DESCRIPTOR_BYTES,
            "{}",
            crate::i18n::format(
                "host-interface-components-0016",
                &[
                    ("arg0", tag.to_string()),
                    ("arg1", component.location.to_string())
                ]
            )
        );
        let descriptor: Value = serde_json::from_str(&descriptor).with_context(|| {
            crate::i18n::format(
                "host-interface-components-0017",
                &[
                    ("arg0", tag.to_string()),
                    ("arg1", component.location.to_string()),
                ],
            )
        })?;
        let xml = serialize(tag, &descriptor, &layout, children, self).with_context(|| {
            crate::i18n::format(
                "host-interface-components-0018",
                &[
                    ("arg0", tag.to_string()),
                    ("arg1", component.location.to_string()),
                ],
            )
        })?;
        Ok((xml, component.location.clone()))
    }
}
fn parse_prop(component: &str, name: &str, spec: &Value, location: &str) -> Result<Prop> {
    let invalid = |reason: &str| {
        anyhow::anyhow!(
            "{}",
            crate::i18n::format(
                "host-interface-components-0019",
                &[
                    ("arg0", component.to_string()),
                    ("arg1", name.to_string()),
                    ("arg2", reason.to_string()),
                    ("arg3", location.to_string())
                ]
            )
        )
    };
    if !valid_attribute(name) || GENERIC.contains(&name) {
        return Err(invalid(crate::i18n::text("host-interface-components-0020")));
    }
    let spec = spec
        .as_object()
        .ok_or_else(|| invalid(crate::i18n::text("host-interface-components-0021")))?;
    let allowed = [
        "type",
        "required",
        "default",
        "min",
        "max",
        "maxLength",
        "valueType",
    ];
    if spec.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid(crate::i18n::text("host-interface-components-0022")));
    }
    let kind = spec
        .get("type")
        .and_then(Value::as_str)
        .and_then(PropKind::parse)
        .ok_or_else(|| invalid(crate::i18n::text("host-interface-components-0023")))?;
    let required = match spec.get("required") {
        None => false,
        Some(Value::Bool(value)) => *value,
        Some(_) => {
            return Err(invalid(crate::i18n::text("host-interface-components-0024")));
        }
    };
    let default = spec.get("default").cloned();
    if required == default.is_some() {
        return Err(invalid(crate::i18n::text("host-interface-components-0025")));
    }
    let number = |key: &str| -> Result<Option<f64>> {
        match spec.get(key) {
            None => Ok(None),
            Some(value) => value
                .as_f64()
                .filter(|value| value.is_finite())
                .map(Some)
                .ok_or_else(|| invalid(crate::i18n::text("host-interface-components-0026"))),
        }
    };
    let (min, max) = (number("min")?, number("max")?);
    if matches!((min, max), (Some(min), Some(max)) if min > max) {
        return Err(invalid(crate::i18n::text("host-interface-components-0027")));
    }
    if (min.is_some() || max.is_some()) && !matches!(kind, PropKind::Int | PropKind::Number) {
        return Err(invalid(crate::i18n::text("host-interface-components-0028")));
    }
    let max_length = match spec.get("maxLength") {
        None => None,
        Some(value) => Some(
            value
                .as_u64()
                .filter(|value| (1..=32768).contains(value))
                .ok_or_else(|| invalid(crate::i18n::text("host-interface-components-0029")))?
                as usize,
        ),
    };
    if max_length.is_some() && kind != PropKind::Text {
        return Err(invalid(crate::i18n::text("host-interface-components-0030")));
    }
    let value_type = match spec.get("valueType") {
        None => None,
        Some(value) => Some(
            value
                .as_str()
                .filter(|value| matches!(*value, "bool" | "int" | "number" | "string" | "text"))
                .ok_or_else(|| invalid(crate::i18n::text("host-interface-components-0031")))?
                .to_string(),
        ),
    };
    if value_type.is_some() != (kind == PropKind::Binding) {
        return Err(invalid(crate::i18n::text("host-interface-components-0032")));
    }
    let prop = Prop {
        kind,
        required,
        default,
        min,
        max,
        max_length,
        value_type,
    };
    if let Some(default) = &prop.default {
        let text = match default {
            Value::String(value) => value.clone(),
            Value::Bool(value) => value.to_string(),
            Value::Number(value) => value.to_string(),
            _ => {
                return Err(invalid(crate::i18n::text("host-interface-components-0033")));
            }
        };
        prop.convert(name, &text)
            .map_err(|error| invalid(&format!("{error:#}")))?;
    }
    Ok(prop)
}
fn valid_tag(tag: &str) -> bool {
    if tag.len() > MAX_TAG_BYTES || !tag.starts_with("x-") {
        return false;
    }
    let rest = &tag[2..];
    !rest.is_empty()
        && !rest.starts_with('-')
        && !rest.ends_with('-')
        && !rest.contains("--")
        && rest
            .bytes()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == b'-')
}
fn valid_attribute(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.bytes().enumerate().all(|(index, ch)| {
            if index == 0 {
                ch.is_ascii_lowercase()
            } else {
                ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == b'-'
            }
        })
}
/// One bounded host node description serialized as XML for the strict parser.
fn serialize(
    instance_tag: &str,
    root: &Value,
    layout: &BTreeMap<String, String>,
    children: usize,
    library: &Library,
) -> Result<String> {
    let mut output = String::from("<component-fragment>");
    let mut tokens = Vec::new();
    let mut count = 0usize;
    write_node(
        &mut output,
        root,
        1,
        true,
        Some(layout),
        instance_tag,
        library,
        &mut tokens,
        &mut count,
    )?;
    output.push_str("</component-fragment>");
    ensure!(
        tokens == (0..children).collect::<Vec<_>>(),
        "{}",
        crate::i18n::format(
            "host-interface-components-0034",
            &[("arg0", instance_tag.to_string())]
        )
    );
    Ok(output)
}
#[allow(clippy::too_many_arguments)]
fn write_node(
    output: &mut String,
    node: &Value,
    depth: usize,
    root: bool,
    layout: Option<&BTreeMap<String, String>>,
    instance_tag: &str,
    library: &Library,
    tokens: &mut Vec<usize>,
    count: &mut usize,
) -> Result<()> {
    ensure!(
        depth <= MAX_DEPTH,
        "{}",
        crate::i18n::format(
            "host-interface-components-0035",
            &[("arg0", instance_tag.to_string())]
        )
    );
    let object = node.as_object().ok_or_else(|| {
        anyhow::anyhow!("{}", crate::i18n::text("host-interface-components-0036"))
    })?;
    ensure!(
        object
            .keys()
            .all(|key| matches!(key.as_str(), "tag" | "attrs" | "children")),
        "{}",
        crate::i18n::format(
            "host-interface-components-0037",
            &[("arg0", instance_tag.to_string())]
        )
    );
    let tag = object.get("tag").and_then(Value::as_str).ok_or_else(|| {
        anyhow::anyhow!("{}", crate::i18n::text("host-interface-components-0036"))
    })?;
    if tag == "$author" {
        ensure!(
            !root,
            "{}",
            crate::i18n::format(
                "host-interface-components-0038",
                &[("arg0", instance_tag.to_string())]
            )
        );
        ensure!(
            object.get("children").is_none(),
            "{}",
            crate::i18n::format(
                "host-interface-components-0039",
                &[("arg0", instance_tag.to_string())]
            )
        );
        let attrs = object
            .get("attrs")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                anyhow::anyhow!("{}", crate::i18n::text("host-interface-components-0040"))
            })?;
        ensure!(
            attrs.len() == 1,
            "{}",
            crate::i18n::format(
                "host-interface-components-0040",
                &[("arg0", instance_tag.to_string())]
            )
        );
        let index = attrs
            .get("index")
            .and_then(Value::as_u64)
            .filter(|index| *index <= u32::MAX as u64)
            .ok_or_else(|| {
                anyhow::anyhow!("{}", crate::i18n::text("host-interface-components-0040"))
            })?;
        tokens.push(index as usize);
        output.push_str(&format!("<author index=\"{index}\"/>"));
        return Ok(());
    }
    ensure!(
        HOST_TAGS.contains(&tag) || valid_tag(tag),
        "{}",
        crate::i18n::format(
            "host-interface-components-0041",
            &[
                ("arg0", tag.to_string()),
                ("arg1", instance_tag.to_string())
            ]
        )
    );
    if tag.starts_with("x-") {
        ensure!(
            library.contains(tag),
            "{}",
            crate::i18n::format(
                "host-interface-components-0043",
                &[
                    ("arg0", tag.to_string()),
                    ("arg1", instance_tag.to_string())
                ]
            )
        );
    }
    if root {
        ensure!(
            matches!(tag, "panel" | "nine-patch"),
            "{}",
            crate::i18n::format(
                "host-interface-components-0044",
                &[
                    ("arg0", tag.to_string()),
                    ("arg1", instance_tag.to_string())
                ]
            )
        );
        ensure!(
            object
                .get("attrs")
                .and_then(Value::as_object)
                .and_then(|attrs| attrs.get("id"))
                .and_then(Value::as_str)
                == Some("$root"),
            "{}",
            crate::i18n::format(
                "host-interface-components-0045",
                &[("arg0", instance_tag.to_string())]
            )
        );
    }
    *count = count.saturating_add(1);
    ensure!(
        *count <= MAX_NODES,
        "{}",
        crate::i18n::format(
            "host-interface-components-0046",
            &[("arg0", instance_tag.to_string())]
        )
    );
    let mut attrs: BTreeMap<String, String> = BTreeMap::new();
    if let Some(values) = object.get("attrs") {
        let values = values.as_object().ok_or_else(|| {
            anyhow::anyhow!("{}", crate::i18n::text("host-interface-components-0047"))
        })?;
        ensure!(
            values.len() <= MAX_ATTRS,
            "{}",
            crate::i18n::format(
                "host-interface-components-0048",
                &[("arg0", instance_tag.to_string())]
            )
        );
        for (name, value) in values {
            ensure!(
                valid_attribute(name) && name != "$root",
                "{}",
                crate::i18n::format(
                    "host-interface-components-0049",
                    &[
                        ("arg0", name.to_string()),
                        ("arg1", instance_tag.to_string())
                    ]
                )
            );
            let text = match value {
                Value::String(value) => value.clone(),
                Value::Bool(value) => value.to_string(),
                Value::Number(value) => value.to_string(),
                _ => {
                    return Err(anyhow::anyhow!(
                        "{}",
                        crate::i18n::format(
                            "host-interface-components-0050",
                            &[
                                ("arg0", name.to_string()),
                                ("arg1", instance_tag.to_string())
                            ]
                        )
                    ));
                }
            };
            ensure!(
                text.len() <= 64 * 1024,
                "{}",
                crate::i18n::format(
                    "host-interface-components-0051",
                    &[("arg0", name.to_string())]
                )
            );
            attrs.insert(name.clone(), text);
        }
    }
    if root && let Some(layout) = layout {
        for (name, value) in layout {
            attrs.insert(name.clone(), value.clone());
        }
    }
    output.push('<');
    output.push_str(tag);
    for (name, value) in attrs {
        output.push(' ');
        output.push_str(&name);
        output.push_str("=\"");
        escape(output, &value);
        output.push('"');
    }
    match object.get("children") {
        None => output.push_str("/>"),
        Some(children) => {
            let children = children.as_array().ok_or_else(|| {
                anyhow::anyhow!("{}", crate::i18n::text("host-interface-components-0052"))
            })?;
            ensure!(
                children.len() <= MAX_CHILDREN,
                "{}",
                crate::i18n::format(
                    "host-interface-components-0053",
                    &[("arg0", instance_tag.to_string())]
                )
            );
            output.push('>');
            for child in children {
                write_node(
                    output,
                    child,
                    depth + 1,
                    false,
                    None,
                    instance_tag,
                    library,
                    tokens,
                    count,
                )?;
            }
            output.push_str("</");
            output.push_str(tag);
            output.push('>');
        }
    }
    Ok(())
}
fn escape(output: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\r' => output.push_str("&#13;"),
            '\n' => output.push_str("&#10;"),
            '\t' => output.push_str("&#9;"),
            ch => output.push(ch),
        }
    }
}
/// Isolated expansion executor implemented by the QuickJS adapter.
pub trait Expander {
    /// Runs one registered `expand` function with already-converted props.
    fn expand(&self, component: &Component, props: &Value, children: usize) -> Result<String>;
}
