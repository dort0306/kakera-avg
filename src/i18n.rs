//! Bounded Fluent resources. No IO, scripts, window or domain state.
use anyhow::{Result, bail, ensure};
use fluent_bundle::{FluentArgs, FluentResource, FluentValue, concurrent::FluentBundle};
use fluent_syntax::ast::{self, Entry, Expression, InlineExpression, PatternElement};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};

pub const MAX_OUTPUT: usize = 64 * 1024;
pub const EMERGENCY: &str = "提示信息无法显示，请查看日志。";
#[derive(Clone, Debug, PartialEq)]
pub enum Argument {
    Text(String),
    Number(i64),
}
impl Argument {
    pub fn from_value(value: &crate::model::VariableValue) -> Result<Self> {
        Ok(match value {
            crate::model::VariableValue::String(s) => Self::Text(s.clone()),
            crate::model::VariableValue::Bool(b) => Self::Text(b.to_string()),
            crate::model::VariableValue::Int(n) => {
                ensure!(
                    n.unsigned_abs() < (1u64 << 53),
                    "{}",
                    crate::model::Diagnostic::new("host-i18n-detail-0001", &[])
                );
                Self::Number(*n)
            }
        })
    }
}
#[derive(Clone, Debug, Default)]
struct Contract {
    parameters: BTreeSet<String>,
    occurrences: BTreeMap<String, usize>,
    references: Vec<String>,
    bound: usize,
    literals: String,
}
#[derive(Clone)]
pub struct Messages {
    bundle: Arc<FluentBundle<FluentResource>>,
    contracts: BTreeMap<String, Contract>,
    static_values: BTreeMap<String, String>,
    reference_edges: usize,
}
impl std::fmt::Debug for Messages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Messages")
            .field("entries", &self.contracts.len())
            .finish()
    }
}
fn inline(value: &InlineExpression<&str>, info: &mut Contract) -> Result<()> {
    match value {
        InlineExpression::StringLiteral { value } | InlineExpression::NumberLiteral { value } => {
            info.bound = info.bound.saturating_add(value.len());
            info.literals
                .push_str(&fluent_syntax::unicode::unescape_unicode_to_string(value));
        }
        InlineExpression::VariableReference { id } => {
            info.parameters.insert(id.name.into());
            *info.occurrences.entry(id.name.into()).or_default() += 1;
        }
        InlineExpression::MessageReference { id, attribute } => {
            ensure!(
                attribute.is_none(),
                "{}",
                crate::model::Diagnostic::new("host-i18n-detail-0002", &[])
            );
            info.references.push(id.name.into());
        }
        InlineExpression::TermReference {
            id,
            attribute,
            arguments,
        } => {
            ensure!(
                attribute.is_none() && arguments.is_none(),
                "{}",
                crate::model::Diagnostic::new("host-i18n-detail-0003", &[])
            );
            info.references.push(format!("-{}", id.name));
        }
        InlineExpression::Placeable { expression } => expression_info(expression, info)?,
        InlineExpression::FunctionReference { .. } => bail!(
            "{}",
            crate::model::Diagnostic::new("host-i18n-detail-0004", &[])
        ),
    }
    Ok(())
}
fn expression_info(value: &Expression<&str>, info: &mut Contract) -> Result<()> {
    match value {
        Expression::Inline(value) => inline(value, info)?,
        Expression::Select { selector, variants } => {
            inline(selector, info)?;
            for variant in variants {
                pattern_info(&variant.value, info)?;
            }
        }
    }
    Ok(())
}
fn pattern_info(pattern: &ast::Pattern<&str>, info: &mut Contract) -> Result<()> {
    for element in &pattern.elements {
        match element {
            PatternElement::TextElement { value } => {
                info.bound = info.bound.saturating_add(value.len());
                info.literals.push_str(value);
            }
            PatternElement::Placeable { expression } => {
                info.bound = info.bound.saturating_add(8);
                expression_info(expression, info)?;
            }
        }
    }
    ensure!(
        info.bound <= MAX_OUTPUT && info.references.len() <= 100000,
        "{}",
        crate::model::Diagnostic::new("host-i18n-detail-0005", &[])
    );
    Ok(())
}
fn expand(
    id: &str,
    raw: &BTreeMap<String, Contract>,
    done: &mut BTreeMap<String, Contract>,
    stack: &mut Vec<String>,
    cache_bytes: &mut usize,
) -> Result<Contract> {
    ensure!(
        stack.len() < 32 && !stack.iter().any(|s| s == id),
        "{}",
        crate::model::Diagnostic::new("host-i18n-detail-0006", &[("arg0", id.to_string())])
    );
    if let Some(info) = done.get(id) {
        return Ok(info.clone());
    }
    let mut info = raw
        .get(id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{}",
                crate::model::Diagnostic::new("host-i18n-detail-0007", &[("arg0", id.to_string())])
            )
        })?
        .clone();
    stack.push(id.into());
    for reference in std::mem::take(&mut info.references) {
        let other = expand(&reference, raw, done, stack, cache_bytes)?;
        info.bound = info.bound.saturating_add(other.bound).saturating_add(8);
        ensure!(
            info.bound <= MAX_OUTPUT,
            "{}",
            crate::model::Diagnostic::new("host-i18n-detail-0008", &[("arg0", id.to_string())])
        );
        info.parameters.extend(other.parameters);
        for (name, count) in other.occurrences {
            let total = info.occurrences.entry(name).or_default();
            *total = total.saturating_add(count);
        }
        info.literals.push_str(&other.literals);
    }
    stack.pop();
    *cache_bytes = cache_bytes.saturating_add(info.literals.len());
    ensure!(
        *cache_bytes <= 64 * 1024 * 1024 && info.parameters.len() <= 16,
        "{}",
        crate::model::Diagnostic::new("host-i18n-detail-0009", &[])
    );
    done.insert(id.into(), info.clone());
    Ok(info)
}
impl Messages {
    pub fn new(locale: &str, sources: &[(String, String)]) -> Result<Self> {
        let mut bundle = FluentBundle::new_concurrent(vec![locale.parse()?]);
        let mut raw = BTreeMap::new();
        let mut edges = 0usize;
        for (file, source) in sources {
            // Bound parser recursion before constructing an AST. Quoted literals and
            // comments do not contribute to Fluent placeable nesting.
            let mut depth = 0usize;
            let mut quoted = false;
            let mut escaped = false;
            for line in source.lines().filter(|line| !line.starts_with('#')) {
                for ch in line.chars() {
                    if escaped {
                        escaped = false;
                        continue;
                    }
                    if quoted && ch == '\\' {
                        escaped = true;
                        continue;
                    }
                    if ch == '"' && depth > 0 {
                        quoted = !quoted;
                        continue;
                    }
                    if !quoted {
                        if ch == '{' {
                            depth += 1;
                        }
                        if ch == '}' {
                            depth = depth.saturating_sub(1);
                        }
                        ensure!(
                            depth <= 32,
                            "{}",
                            crate::model::Diagnostic::new(
                                "host-ftl-depth",
                                &[("file", file.clone())]
                            )
                        );
                    }
                }
            }
            let resource = FluentResource::try_new(source.clone()).map_err(|(_, errors)| {
                let offset = errors.first().map_or(0, |error| error.pos.start);
                let prefix = source.get(..offset).unwrap_or("");
                let row = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
                let col = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
                anyhow::anyhow!(
                    "{file}:{row}:{col}: {}",
                    crate::model::Diagnostic::new(
                        "host-ftl-parse",
                        &[(
                            "reason",
                            format!(
                                "{:?} (total: {})",
                                errors.iter().take(8).collect::<Vec<_>>(),
                                errors.len()
                            )
                        )]
                    )
                )
            })?;
            for entry in resource.entries() {
                let (id, pattern, attributes) = match entry {
                    Entry::Message(m) => (m.id.name.to_string(), m.value.as_ref(), &m.attributes),
                    Entry::Term(t) => (format!("-{}", t.id.name), Some(&t.value), &t.attributes),
                    _ => continue,
                };
                ensure!(
                    id.len() <= 128 && attributes.is_empty(),
                    "{}",
                    crate::model::Diagnostic::new(
                        "host-i18n-detail-0010",
                        &[("arg0", file.to_string()), ("arg1", id.to_string())]
                    )
                );
                let mut info = Contract::default();
                pattern_info(
                    pattern.ok_or_else(|| {
                        anyhow::anyhow!(
                            "{}",
                            crate::model::Diagnostic::new(
                                "host-i18n-detail-0011",
                                &[("arg0", file.to_string()), ("arg1", id.to_string())]
                            )
                        )
                    })?,
                    &mut info,
                )?;
                edges = edges.saturating_add(info.references.len());
                ensure!(
                    edges <= 100000 && raw.len() < 50000,
                    "{}",
                    crate::model::Diagnostic::new(
                        "host-i18n-detail-0012",
                        &[("arg0", file.to_string())]
                    )
                );
                ensure!(
                    raw.insert(id.clone(), info).is_none(),
                    "{}",
                    crate::model::Diagnostic::new(
                        "host-i18n-detail-0013",
                        &[("arg0", file.to_string()), ("arg1", id.to_string())]
                    )
                );
            }
            bundle
                .add_resource(resource)
                .map_err(|e| anyhow::anyhow!("{file}: {e:?}"))?;
        }
        let mut contracts = BTreeMap::new();
        let mut cache_bytes = 0usize;
        for id in raw.keys() {
            expand(id, &raw, &mut contracts, &mut Vec::new(), &mut cache_bytes)?;
        }
        let mut messages = Self {
            bundle: Arc::new(bundle),
            contracts,
            static_values: BTreeMap::new(),
            reference_edges: edges,
        };
        let mut bytes = 0usize;
        for (id, contract) in &messages.contracts {
            if !id.starts_with('-') && contract.parameters.is_empty() {
                let value = messages.format(id, &BTreeMap::new())?;
                bytes = bytes.saturating_add(value.len());
                ensure!(
                    bytes <= 64 * 1024 * 1024,
                    "{}",
                    crate::model::Diagnostic::new("host-i18n-detail-0014", &[])
                );
                messages.static_values.insert(id.clone(), value);
            }
        }
        Ok(messages)
    }
    pub fn entry_count(&self) -> usize {
        self.contracts.len()
    }
    pub fn reference_count(&self) -> usize {
        self.reference_edges
    }
    pub fn cache_bytes(&self) -> usize {
        self.static_values
            .values()
            .map(String::len)
            .sum::<usize>()
            .saturating_add(self.contracts.values().map(|c| c.literals.len()).sum())
    }
    pub fn contains(&self, id: &str) -> bool {
        self.contracts.contains_key(id) && !id.starts_with('-')
    }
    pub fn parameters(&self, id: &str) -> Result<&BTreeSet<String>> {
        Ok(&self
            .contracts
            .get(id)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "{}",
                    crate::model::Diagnostic::new(
                        "host-i18n-detail-0015",
                        &[("arg0", id.to_string())]
                    )
                )
            })?
            .parameters)
    }
    pub fn literals(&self) -> String {
        self.contracts
            .values()
            .map(|c| c.literals.as_str())
            .collect()
    }
    pub fn format(&self, id: &str, args: &BTreeMap<String, Argument>) -> Result<String> {
        ensure!(
            self.contains(id),
            "{}",
            crate::model::Diagnostic::new("host-i18n-detail-0016", &[("arg0", id.to_string())])
        );
        ensure!(
            args.len() <= 16
                && args.keys().collect::<BTreeSet<_>>() == self.parameters(id)?.iter().collect(),
            "{}",
            crate::model::Diagnostic::new("host-i18n-detail-0017", &[("arg0", id.to_string())])
        );
        if let Some(value) = self.static_values.get(id) {
            return Ok(value.clone());
        }
        let contract = &self.contracts[id];
        let mut bound = contract.bound;
        for (name, count) in &contract.occurrences {
            let bytes = match &args[name] {
                Argument::Text(s) => s.len(),
                Argument::Number(_) => 64,
            };
            bound = bound.saturating_add(count.saturating_mul(bytes));
        }
        ensure!(
            bound <= MAX_OUTPUT,
            "{}",
            crate::model::Diagnostic::new("host-i18n-detail-0005", &[])
        );
        let mut fluent = FluentArgs::new();
        for (key, value) in args {
            let value = match value {
                Argument::Text(s) => {
                    ensure!(
                        s.len() <= 4096,
                        "{}",
                        crate::model::Diagnostic::new("host-i18n-detail-0018", &[])
                    );
                    FluentValue::from(s.as_str())
                }
                Argument::Number(n) => {
                    ensure!(
                        n.unsigned_abs() < (1u64 << 53),
                        "{}",
                        crate::model::Diagnostic::new("host-i18n-detail-0019", &[])
                    );
                    FluentValue::from(*n)
                }
            };
            fluent.set(key, value);
        }
        let message = self.bundle.get_message(id).ok_or_else(|| {
            anyhow::anyhow!(
                "{}",
                crate::model::Diagnostic::new("host-i18n-detail-0020", &[("arg0", id.to_string())])
            )
        })?;
        let mut errors = Vec::new();
        let output = self
            .bundle
            .format_pattern(
                message.value().expect("validated value"),
                Some(&fluent),
                &mut errors,
            )
            .into_owned();
        ensure!(
            errors.is_empty() && output.len() <= MAX_OUTPUT,
            "{}",
            crate::model::Diagnostic::new(
                "host-i18n-detail-0021",
                &[("arg0", id.to_string()), ("arg1", format!("{:?}", errors))]
            )
        );
        Ok(output)
    }
}

fn host() -> &'static Messages {
    static HOST: OnceLock<Messages> = OnceLock::new();
    HOST.get_or_init(|| {
        Messages::new(
            "zh-CN",
            &[(
                "engine/zh-CN.ftl".into(),
                include_str!("../locales/engine/zh-CN.ftl").into(),
            )],
        )
        .expect("embedded engine Fluent catalog")
    })
}
/// All immutable host labels are formatted once, outside drawing.
pub fn text(id: &str) -> &'static str {
    static TEXT: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    TEXT.get_or_init(|| {
        host()
            .contracts
            .iter()
            .filter(|(id, c)| !id.starts_with('-') && c.parameters.is_empty())
            .map(|(id, _)| {
                (
                    id.clone(),
                    host().format(id, &BTreeMap::new()).expect("embedded label"),
                )
            })
            .collect()
    })
    .get(id)
    .map(String::as_str)
    .unwrap_or(EMERGENCY)
}
pub fn format(id: &str, args: &[(&str, String)]) -> String {
    format_host(id, args, true)
}
/// Author text is data, even when it resembles a serialized diagnostic.
pub fn format_literal(id: &str, args: &[(&str, String)]) -> String {
    format_host(id, args, false)
}
fn format_host(id: &str, args: &[(&str, String)], causes: bool) -> String {
    let arguments = args
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                Argument::Text(if causes { diagnostics(v) } else { v.clone() }),
            )
        })
        .collect();
    host().format(id, &arguments).unwrap_or_else(|error| {
        eprintln!("I18N_FORMAT {id}: {error:#}");
        EMERGENCY.into()
    })
}
pub fn font_seed() -> String {
    host().literals()
}
pub fn diagnostics(text: &str) -> String {
    fn resolve(text: &str, depth: usize) -> String {
        if depth >= 32 || text.len() > MAX_OUTPUT {
            return EMERGENCY.into();
        }
        let mut output = String::new();
        let mut rest = text;
        while let Some(at) = rest.find("KDIAG{") {
            output.push_str(&rest[..at]);
            let json = &rest[at + 5..];
            let mut stream =
                serde_json::Deserializer::from_str(json).into_iter::<crate::model::Diagnostic>();
            if let Some(Ok(message)) = stream.next() {
                let length = stream.byte_offset();
                let args = message
                    .parameters
                    .into_iter()
                    .map(|(k, v)| (k, Argument::Text(resolve(&v, depth + 1))))
                    .collect();
                output.push_str(
                    &host().format(&message.code, &args).unwrap_or_else(|error| {
                        eprintln!("I18N_DIAGNOSTIC {}: {error:#}", message.code);
                        EMERGENCY.into()
                    }),
                );
                rest = &json[length..];
            } else {
                output.push_str("KDIAG");
                rest = &rest[at + 5..];
            }
        }
        output.push_str(rest);
        output
    }
    resolve(text, 0)
}
pub fn validate_host() -> Result<()> {
    for (id, c) in &host().contracts {
        if !id.starts_with('-') {
            host().format(
                id,
                &c.parameters
                    .iter()
                    .map(|p| (p.clone(), Argument::Text("测试".into())))
                    .collect(),
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn messages(text: &str) -> Result<Messages> {
        Messages::new("zh-CN", &[("test.ftl".into(), text.into())])
    }
    #[test]
    fn embedded_host_catalog_and_structured_diagnostics_are_valid() {
        validate_host().unwrap();
        let diagnostic = crate::model::Diagnostic::new("host-model-0005", &[]);
        assert_eq!(diagnostics(&diagnostic.to_string()), "起始幕不存在");
        assert_eq!(text("host-dialog-close"), "关闭");
        assert!(
            messages(r#"escaped = { "\u4E00" }"#)
                .unwrap()
                .literals()
                .contains('一')
        );
        let literal = diagnostic.to_string();
        assert!(
            format_literal("host-history-choice", &[("text", literal.clone())]).contains(&literal)
        );
    }
    #[test]
    fn catalogs_reject_partial_duplicate_missing_cyclic_and_exponential_resources() {
        for text in [
            "a = ok\nb = {",
            "a = one\na = two",
            "a = { missing }",
            "a = { b }\nb = { a }",
            "a = { READFILE() }",
        ] {
            assert!(messages(text).is_err(), "{text}");
        }
        let mut source = "a0 = x\n".to_string();
        for n in 1..32 {
            source.push_str(&format!("a{n} = {{ a{} }}{{ a{} }}\n", n - 1, n - 1));
        }
        assert!(messages(&source).is_err());
        let mut empty = "a0 = { \"\" }\n".to_string();
        for n in 1..32 {
            empty.push_str(&format!("a{n} = {{ a{} }}{{ a{} }}\n", n - 1, n - 1));
        }
        assert!(
            messages(&empty).is_err(),
            "empty references still have isolation/expansion cost"
        );
        assert!(messages(&format!("a = raw quotation \" {}", "{".repeat(40))).is_err());
        assert!(messages(&format!("a = {}", "{".repeat(40))).is_err());
    }
    #[test]
    fn formatting_enforces_arguments_and_preserves_isolation() {
        let catalog = messages("message = 你好 { $name }\n").unwrap();
        assert!(catalog.format("message", &BTreeMap::new()).is_err());
        let args = [("name".into(), Argument::Text("甲".into()))]
            .into_iter()
            .collect();
        assert_eq!(
            catalog.format("message", &args).unwrap(),
            "你好 \u{2068}甲\u{2069}"
        );
        let source = format!(
            "many = {}",
            (0..16).map(|n| format!("{{ $p{n} }}")).collect::<String>()
        );
        let messages = messages(&source).unwrap();
        let args = (0..16)
            .map(|n| (format!("p{n}"), Argument::Text("x".into())))
            .collect();
        assert!(messages.format("many", &args).is_ok());
        let args = (0..16)
            .map(|n| (format!("p{n}"), Argument::Text("x".repeat(4096))))
            .collect();
        assert!(messages.format("many", &args).is_err());
    }
}
