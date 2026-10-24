//! QuickJS adapter. All persistent state and commit validation stay in Rust.
use crate::{
    content::{self, ContentSource},
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

pub enum LogicEngine {
    Declarative,
    Script(ScriptLogic),
}
impl LogicEngine {
    pub fn ui_handlers(&self) -> Result<BTreeSet<String>> {
        match self {
            Self::Declarative => Ok(BTreeSet::new()),
            Self::Script(logic) => Ok(serde_json::from_str(
                &logic.evaluate(json!({"mode":"ui-manifest"}))?,
            )?),
        }
    }
    pub fn ui_event(
        &self,
        event: &crate::interface::Event,
        session: &Session,
        ui: &Variables,
        snapshot: &Value,
    ) -> Result<Vec<crate::interface::host::Command>> {
        let Self::Script(logic) = self else {
            anyhow::bail!("UI 事件需要 QuickJS")
        };
        let mut request = ScriptLogic::request("ui", &session.frame_id, None, &session.variables);
        request["uiVariables"] = ScriptLogic::request("ui", "", None, ui)["variables"].clone();
        request["event"] = json!(event.name);
        request["item"] = event.item.clone();
        request["value"] = event.value.clone();
        request["snapshot"] = snapshot.clone();
        Ok(serde_json::from_str(&logic.evaluate(request)?)?)
    }
}
impl Logic for LogicEngine {
    fn resolve(
        &self,
        story: &Story,
        frame: &str,
        choice: Option<&str>,
        vars: &Variables,
    ) -> Result<Option<TransitionRule>, String> {
        match self {
            Self::Declarative => DeclarativeLogic.resolve(story, frame, choice, vars),
            Self::Script(logic) => logic.resolve(story, frame, choice, vars),
        }
    }
    fn options(&self, story: &Story, session: &Session) -> Result<Vec<String>, String> {
        match self {
            Self::Declarative => DeclarativeLogic.options(story, session),
            Self::Script(logic) => logic.options(story, session),
        }
    }
}

pub struct ScriptLogic {
    files: Vec<(String, String)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Registration {
    frame: String,
    kind: String,
    options: Vec<String>,
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
            ensure!(total <= 1024 * 1024, "JS 源码总量超过 1MiB");
            files.push((path.to_string_lossy().into_owned(), code));
        }
        let logic = Self { files };
        let manifest: Vec<Registration> =
            serde_json::from_str(&logic.evaluate(json!({"mode":"manifest"}))?)?;
        let mut routes = BTreeMap::new();
        for item in manifest {
            let frame = story
                .frames
                .get(&item.frame)
                .with_context(|| format!("{}：未知触发器场景 {}", item.location, item.frame))?;
            let kind = match item.kind.as_str() {
                "advance" => ScriptRoute::Advance,
                "choice" => ScriptRoute::Choice,
                "end" => ScriptRoute::End,
                _ => anyhow::bail!("未知触发器类型"),
            };
            let expected: BTreeSet<_> = frame.options.iter().map(|item| &item.id).collect();
            let actual: BTreeSet<_> = item.options.iter().collect();
            ensure!(
                if kind == ScriptRoute::Choice {
                    !expected.is_empty() && expected == actual
                } else {
                    expected.is_empty() && actual.is_empty()
                },
                "{}：场景 {} 的触发器与 XML 选项不一致",
                item.location,
                item.frame
            );
            ensure!(
                routes
                    .insert(item.frame.clone(), Route::Script(kind))
                    .is_none(),
                "重复触发器：{}",
                item.frame
            );
        }
        for frame in story.frames.keys() {
            ensure!(routes.contains_key(frame), "JS 缺少场景触发器：{frame}");
        }
        story.routes = routes;
        Ok(logic)
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
                .map_err(|error| anyhow::anyhow!("QuickJS 初始化：{error}"))?;
            for (path, code) in &self.files {
                let mut options = EvalOptions::default();
                options.filename = Some(path.clone());
                options.strict = true;
                ctx.eval_with_options::<(), _>(code.as_bytes(), options)
                    .catch(&ctx)
                    .map_err(|error| anyhow::anyhow!("{path}：QuickJS：{error}"))?;
            }
            bridge
                .call((request.to_string(),))
                .catch(&ctx)
                .map_err(|error| {
                    anyhow::anyhow!(
                        "JS 回调（{}）：{error}",
                        self.files
                            .iter()
                            .map(|(name, _)| name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
        })?;
        ensure!(
            !runtime.is_job_pending(),
            "JS 不支持异步任务或 Promise 回调"
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
}

impl Logic for ScriptLogic {
    fn resolve(
        &self,
        story: &Story,
        frame: &str,
        choice: Option<&str>,
        vars: &Variables,
    ) -> Result<Option<TransitionRule>, String> {
        (|| -> Result<_> {
            if choice.is_none()
                && matches!(
                    story.routes.get(frame),
                    Some(Route::Script(ScriptRoute::Choice | ScriptRoute::End))
                )
            {
                return Ok(None);
            }
            let output = self.evaluate(Self::request("resolve", frame, choice, vars))?;
            let Some(output): Option<Output> = serde_json::from_str(&output)? else {
                return Ok(None);
            };
            ensure!(
                story.frames.contains_key(&output.to),
                "JS 目标场景不存在：{}",
                output.to
            );
            ensure!(output.effects.len() <= 128, "JS 更新超过 128 项");
            let effects = output
                .effects
                .into_iter()
                .map(|item| -> Result<Effect> {
                    Ok(match item {
                        Update::Add { variable, amount } => Effect::Add {
                            variable,
                            amount: amount.parse().context("JS add 超出 i64")?,
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
                                        .context("JS int 格式错误")?
                                        .parse()
                                        .context("JS int 超出 i64")?,
                                ),
                                "bool" => VariableValue::Bool(
                                    value.as_bool().context("JS bool 格式错误")?,
                                ),
                                "string" => VariableValue::String(
                                    value.as_str().context("JS string 格式错误")?.into(),
                                ),
                                _ => anyhow::bail!("JS 返回未知变量类型"),
                            },
                        },
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            // The adapter validates too, so history replay and controller share the same checks.
            rules::apply_effects(story, vars, &effects).map_err(anyhow::Error::msg)?;
            Ok(Some(TransitionRule {
                to: output.to,
                when: None,
                effects,
            }))
        })()
        .map_err(|error| format!("{error:#}"))
    }
    fn options(&self, story: &Story, session: &Session) -> Result<Vec<String>, String> {
        (|| -> Result<_> {
            let frame = story.frame(session).context("当前场景不存在")?;
            let mut names = Vec::new();
            // Each guard sees the same fresh globals as its actual choice dispatch.
            // One guard cannot affect another guard through a captured JS variable.
            for option in &frame.options {
                let allowed: bool = serde_json::from_str(&self.evaluate(Self::request(
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
        .map_err(|error| format!("{error:#}"))
    }
}
