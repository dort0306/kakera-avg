//! QuickJS adapter. All persistent state and commit validation stay in Rust.
use crate::{
    content::{self, ContentSource},
    interface::{EventCategory, UiEventKey},
    model::*,
};
use anyhow::{Context as _, Result, ensure};
use rquickjs::{CatchResultExt, Context, Function, Runtime, context::EvalOptions};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    time::{Duration, Instant},
};

impl ScriptLogic {
    /// Author composition tags collected from the same isolated script snapshot.
    pub fn component_library(&self) -> Result<crate::interface::components::Library> {
        let manifest = self.evaluate(json!({"mode":"component-manifest"}))?;
        crate::interface::components::Library::parse(&manifest)
    }
    pub(crate) fn bind_interface(&mut self, definition: &crate::interface::Definition) {
        fn collect(nodes: &[crate::interface::Node], keys: &mut BTreeSet<UiEventKey>) {
            for node in nodes {
                if let Some(event) = &node.event {
                    keys.insert(UiEventKey::ui(&node.id, event));
                }
                collect(&node.children, keys);
            }
        }
        self.ui_keys = definition.lifecycle.values().cloned().collect();
        for (id, screen) in &definition.screens {
            collect(&screen.nodes, &mut self.ui_keys);
            for event in [&screen.background_click, &screen.background_right_click]
                .into_iter()
                .flatten()
            {
                self.ui_keys.insert(UiEventKey::ui(id, event));
            }
        }
        if definition.enter {
            self.ui_keys.insert(UiEventKey::game());
        }
        self.ui_keys
            .extend(definition.host_handlers.iter().map(UiEventKey::host));
    }
    /// Structured `frame(...)`/`ui(...)` registrations, each with its author call site.
    pub fn ui_registrations(&self) -> Result<Vec<crate::interface::Registration>> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Entry {
            category: String,
            target: String,
            event: String,
            location: String,
        }
        let entries: Vec<Entry> =
            serde_json::from_str(&self.evaluate(json!({"mode":"ui-manifest"}))?)?;
        entries
            .into_iter()
            .map(|entry| {
                let category = EventCategory::parse(&entry.category).with_context(|| {
                    crate::i18n::format("host-script-0019", &[("arg0", entry.category.to_string())])
                })?;
                Ok(crate::interface::Registration {
                    key: UiEventKey {
                        category,
                        target: entry.target,
                        event: entry.event,
                    },
                    location: entry.location,
                })
            })
            .collect()
    }
    pub fn ui_event(
        &self,
        event: &crate::interface::Event,
        session: &Session,
        ui: &Variables,
        snapshot: &Value,
    ) -> Result<Vec<crate::interface::host::Command>> {
        let key = &event.key;
        ensure!(
            self.ui_keys.contains(key),
            "{}",
            crate::i18n::format("host-interface-0079", &[("arg0", key.describe())])
        );
        let mut request = self.frame_request("ui", &session.frame_id, None, &session.variables);
        request["uiVariables"] = ScriptLogic::request("ui", "", None, ui)["variables"].clone();
        request["category"] = json!(key.category.id());
        request["target"] = json!(key.target);
        request["event"] = json!(key.event);
        request["item"] = event.item.clone();
        request["value"] = event.value.clone();
        request["snapshot"] = snapshot.clone();
        Ok(serde_json::from_str(&self.evaluate(request)?)?)
    }
}

pub struct ScriptLogic {
    files: Vec<(String, String)>,
    frames: BTreeMap<String, (String, String)>,
    ui_keys: BTreeSet<UiEventKey>,
}

impl crate::interface::components::Expander for ScriptLogic {
    fn expand(
        &self,
        component: &crate::interface::components::Component,
        props: &Value,
        children: usize,
    ) -> Result<String> {
        self.evaluate(json!({
            "mode": "component-expand",
            "tag": component.name,
            "props": props,
            "children": children
        }))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Registration {
    frame: String,
    kind: String,
    options: Vec<String>,
    location: String,
    routes: Vec<RouteEntry>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RouteEntry {
    #[serde(deserialize_with = "Option::deserialize")]
    option: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    target: Option<String>,
    location: String,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "lowercase", deny_unknown_fields)]
enum Update {
    Set {
        variable: String,
        #[serde(rename = "type")]
        kind: String,
        value: Value,
    },
    Add {
        variable: String,
        amount: String,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Output {
    to: String,
    effects: Vec<Update>,
}

impl ScriptLogic {
    pub fn load(story: &mut Story, source: &dyn ContentSource, paths: &[PathBuf]) -> Result<Self> {
        let mut files = Vec::new();
        let mut total = 0;
        for path in paths {
            let code = content::text(source, path)?;
            total += code.len();
            ensure!(
                total <= 1024 * 1024,
                "{}",
                crate::i18n::format("host-script-0001", &[])
            );
            files.push((path.to_string_lossy().into_owned(), code));
        }
        let mut logic = Self {
            files,
            frames: BTreeMap::new(),
            ui_keys: BTreeSet::new(),
        };
        let manifest: Vec<Registration> =
            serde_json::from_str(&logic.evaluate(json!({"mode":"manifest"}))?)?;
        let mut routes = BTreeMap::new();
        for item in manifest {
            let actual_frame = story.resolve_frame(&item.frame).with_context(|| {
                crate::i18n::format(
                    "host-script-0002",
                    &[
                        ("arg0", item.location.to_string()),
                        ("arg1", item.frame.to_string()),
                    ],
                )
            })?;
            let frame = &story.frames[actual_frame];
            let kind = match item.kind.as_str() {
                "advance" => Route::Advance,
                "choice" => Route::Choice,
                "end" => Route::End,
                _ => anyhow::bail!("{}", crate::i18n::format("host-script-0003", &[])),
            };
            let expected: BTreeSet<_> = frame.options.iter().map(|item| &item.id).collect();
            let actual: BTreeSet<_> = item.options.iter().collect();
            ensure!(
                if kind == Route::Choice {
                    !expected.is_empty() && expected == actual
                } else {
                    expected.is_empty() && actual.is_empty()
                },
                "{}",
                crate::i18n::format(
                    "host-script-0004",
                    &[
                        ("arg0", item.location.to_string()),
                        ("arg1", item.frame.to_string())
                    ]
                )
            );
            Self::validate_routes(&frame.id, kind, &item.options, &item.routes)?;
            for entry in &item.routes {
                if let Some(target) = &entry.target {
                    story.resolve_frame(target).with_context(|| {
                        crate::i18n::format(
                            "host-script-0022",
                            &[
                                ("arg0", entry.location.to_string()),
                                ("arg1", target.to_string()),
                                ("arg2", item.frame.to_string()),
                            ],
                        )
                    })?;
                }
            }
            ensure!(
                routes.insert(actual_frame.to_owned(), kind).is_none(),
                "{}",
                crate::i18n::format("host-script-0005", &[("arg0", item.frame.to_string())])
            );
            logic.frames.insert(
                actual_frame.to_owned(),
                (item.frame, frame.reference().to_owned()),
            );
        }
        for frame in story.frames.keys() {
            ensure!(
                routes.contains_key(frame),
                "{}",
                crate::i18n::format("host-script-0006", &[("arg0", frame.to_string())])
            );
        }
        story.routes = routes;
        Ok(logic)
    }

    /// The bridge already rejects mixed routes; this keeps the trusted manifest honest and
    /// pairs every declared target with its own author call site.
    fn validate_routes(
        frame: &str,
        kind: Route,
        options: &[String],
        routes: &[RouteEntry],
    ) -> Result<()> {
        let expected: Vec<&str> = match kind {
            Route::Advance => vec![""],
            Route::Choice => options.iter().map(String::as_str).collect(),
            Route::End => Vec::new(),
        };
        ensure!(
            routes.len() == expected.len(),
            "{}",
            crate::i18n::format("host-script-0020", &[("arg0", frame.to_string())])
        );
        for (entry, want) in routes.iter().zip(expected) {
            let actual = entry.option.as_deref().unwrap_or_default();
            ensure!(
                actual == want,
                "{}",
                crate::i18n::format(
                    "host-script-0021",
                    &[("arg0", frame.to_string()), ("arg1", actual.to_string())]
                )
            );
        }
        Ok(())
    }

    fn evaluate(&self, request: Value) -> Result<String> {
        let runtime = Runtime::new()?;
        runtime.set_memory_limit(16 * 1024 * 1024);
        runtime.set_max_stack_size(256 * 1024);
        let deadline = Instant::now() + Duration::from_millis(200);
        let mut interrupts = 0_u32;
        runtime.set_interrupt_handler(Some(Box::new(move || {
            interrupts += 1;
            interrupts > 10_000 || Instant::now() > deadline
        })));
        let context = Context::full(&runtime)?;
        let result = context.with(|ctx| -> Result<String> {
            let mut engine_options = EvalOptions::default();
            engine_options.filename = Some("<engine-bridge>".into());
            let bridge: Function = ctx
                .eval_with_options(include_str!("script/bridge.js"), engine_options)
                .catch(&ctx)
                .map_err(|error| {
                    anyhow::anyhow!(
                        "{}",
                        crate::i18n::format("host-script-0007", &[("arg0", error.to_string())])
                    )
                })?;
            for (path, code) in &self.files {
                let mut options = EvalOptions::default();
                options.filename = Some(path.clone());
                options.strict = true;
                ctx.eval_with_options::<(), _>(code.as_bytes(), options)
                    .catch(&ctx)
                    .map_err(|error| {
                        anyhow::anyhow!(
                            "{path}：QuickJS：{}",
                            crate::i18n::diagnostics(&error.to_string())
                        )
                    })?;
            }
            bridge
                .call((request.to_string(),))
                .catch(&ctx)
                .map_err(|error| {
                    anyhow::anyhow!(
                        "{}",
                        crate::i18n::format(
                            "host-script-0008",
                            &[
                                (
                                    "arg0",
                                    self.files
                                        .iter()
                                        .map(|(name, _)| name.as_str())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                        .to_string()
                                ),
                                ("arg1", error.to_string())
                            ]
                        )
                    )
                })
        })?;
        ensure!(
            !runtime.is_job_pending(),
            "{}",
            crate::i18n::format("host-script-0009", &[])
        );
        Ok(result)
    }

    fn request(mode: &str, frame: &str, choice: Option<&str>, vars: &Variables) -> Value {
        // i64 is transported as decimal text, never through an IEEE-754 Number.
        let variables: BTreeMap<_, _> = vars
            .iter()
            .map(|(name, value)| {
                (
                    name,
                    match value {
                        VariableValue::Int(value) => {
                            json!({"type":"int","value":value.to_string()})
                        }
                        VariableValue::Bool(value) => json!({"type":"bool","value":value}),
                        VariableValue::String(value) => json!({"type":"string","value":value}),
                    },
                )
            })
            .collect();
        json!({"mode":mode,"frame":frame,"choice":choice,"variables":variables})
    }
    fn frame_request(
        &self,
        mode: &str,
        frame: &str,
        choice: Option<&str>,
        vars: &Variables,
    ) -> Value {
        let (key, name) = self
            .frames
            .get(frame)
            .map(|(key, name)| (key.as_str(), name.as_str()))
            .unwrap_or((frame, frame));
        let mut request = Self::request(mode, key, choice, vars);
        request["frameName"] = json!(name);
        request
    }
}

impl Logic for ScriptLogic {
    fn resolve(
        &self,
        story: &Story,
        frame: &str,
        choice: Option<&str>,
        vars: &Variables,
    ) -> Result<Option<TransitionRule>, crate::model::Diagnostic> {
        (|| -> Result<_> {
            if choice.is_none()
                && matches!(story.routes.get(frame), Some(Route::Choice | Route::End))
            {
                return Ok(None);
            }
            let output = self.evaluate(self.frame_request("resolve", frame, choice, vars))?;
            let Some(output): Option<Output> = serde_json::from_str(&output)? else {
                return Ok(None);
            };
            let to = story
                .resolve_frame(&output.to)
                .with_context(|| {
                    crate::i18n::format("host-script-0010", &[("arg0", output.to.to_string())])
                })?
                .to_owned();
            ensure!(
                output.effects.len() <= 128,
                "{}",
                crate::i18n::format("host-script-0011", &[])
            );
            let effects = output
                .effects
                .into_iter()
                .map(|item| -> Result<Effect> {
                    Ok(match item {
                        Update::Add { variable, amount } => Effect::Add {
                            variable,
                            amount: amount
                                .parse()
                                .context(crate::i18n::text("host-script-0013"))?,
                        },
                        Update::Set {
                            variable,
                            kind,
                            value,
                        } => Effect::Set {
                            variable,
                            value: match kind.as_str() {
                                "int" => VariableValue::Int(
                                    value
                                        .as_str()
                                        .context(crate::i18n::text("host-script-0014"))?
                                        .parse()
                                        .context(crate::i18n::text("host-script-0015"))?,
                                ),
                                "bool" => VariableValue::Bool(
                                    value
                                        .as_bool()
                                        .context(crate::i18n::text("host-script-0016"))?,
                                ),
                                "string" => VariableValue::String(
                                    value
                                        .as_str()
                                        .context(crate::i18n::text("host-script-0017"))?
                                        .into(),
                                ),
                                _ => anyhow::bail!(
                                    "{}",
                                    crate::i18n::format("host-script-0012", &[])
                                ),
                            },
                        },
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            // The adapter validates too, so history replay and controller share the same checks.
            rules::apply_effects(story, vars, &effects).map_err(anyhow::Error::msg)?;
            Ok(Some(TransitionRule { to, effects }))
        })()
        .map_err(|error| crate::model::Diagnostic::from(format!("{error:#}")))
    }
    fn options(
        &self,
        story: &Story,
        session: &Session,
    ) -> Result<Vec<String>, crate::model::Diagnostic> {
        (|| -> Result<_> {
            let frame = story
                .frame(session)
                .context(crate::i18n::text("host-script-0018"))?;
            let mut names = Vec::new();
            // Each guard sees the same fresh globals as its actual choice dispatch.
            // One guard cannot affect another guard through a captured JS variable.
            for option in &frame.options {
                let allowed: bool = serde_json::from_str(&self.evaluate(self.frame_request(
                    "guard",
                    &session.frame_id,
                    Some(&option.id),
                    &session.variables,
                ))?)?;
                if allowed {
                    names.push(option.id.clone());
                }
            }
            Ok(names)
        })()
        .map_err(|error| crate::model::Diagnostic::from(format!("{error:#}")))
    }
}
