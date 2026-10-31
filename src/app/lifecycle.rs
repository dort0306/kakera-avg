use super::*;
use crate::interface::{
    self,
    host::{self, Command, Operation, Plan, State},
    presentation::Action,
};
use crate::save::Snapshot;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Checkpoint {
    schema: u32,
    active: bool,
    exited: bool,
    entered: bool,
    source: Snapshot,
    continuation: Snapshot,
    stack: Vec<String>,
    origin: Vec<String>,
    variables: crate::model::Variables,
    #[serde(deserialize_with = "Option::deserialize")]
    sequence: Option<String>,
}
impl Checkpoint {
    pub(super) fn terminal_exit(&self) -> bool {
        self.exited
            && self.source.frame_id == self.continuation.frame_id
            && self.source.history == self.continuation.history
            && self.source.variables == self.continuation.variables
            && self.source.history_base_variables == self.continuation.history_base_variables
            && self.source.generation == self.continuation.generation
    }
}
impl PlayerApp {
    pub(super) fn exit_terminal(&mut self, base: Option<Plan>) -> Result<()> {
        let mut plan = base.unwrap_or(self.empty_ui_plan()?);
        plan.operation = Operation::None;
        if self.terminal_exit_done
            || !matches!(
                self.story.routes[&self.session.frame_id],
                crate::model::Route::End
            )
        {
            return self.commit_page_plan(plan);
        }
        ensure!(
            self.handoff.is_none(),
            "{}",
            crate::i18n::format("host-app-lifecycle-0001", &[])
        );
        plan = self.lifecycle_hook(plan, &self.session, "frame.exit", &self.session.frame_id)?;
        plan = self.page_lifecycle(plan, &self.session, None)?;
        if plan.state.stack != self.custom.state.stack
            || matches!(plan.operation, Operation::Present)
        {
            ensure!(
                matches!(plan.operation, Operation::None | Operation::Present),
                "{}",
                crate::i18n::format("host-app-lifecycle-0002", &[])
            );
            let prepared = self.prepare_ui_media(&plan)?;
            let mut checkpoint = self.checkpoint(&self.session, &plan)?;
            checkpoint.exited = true;
            checkpoint.entered = false;
            crate::save::atomic_json(
                &self.saves.directory().join("flow-checkpoint.json"),
                &checkpoint,
            )?;
            self.handoff = Some(checkpoint);
            self.pending_auto = None;
            self.auto_play.set(false);
            self.commit_ui(plan, true);
            self.apply_ui_media(prepared);
        } else {
            self.commit_page_plan(plan)?;
        }
        self.terminal_exit_done = true;
        Ok(())
    }
    pub(super) fn ui_video_position(&self) -> f64 {
        self.ui_media.position()
    }
    pub(super) fn empty_ui_plan(&self) -> Result<Plan> {
        host::prepare(
            &self.custom.definition,
            &self.custom.state,
            &self.settings,
            &self.custom.profile,
            vec![],
            false,
        )
    }
    fn merge_lifecycle(&self, mut base: Plan, extra: Plan) -> Result<Plan> {
        let combined = if let (Operation::Resume(a), Operation::Resume(b)) =
            (&base.operation, &extra.operation)
        {
            ensure!(
                a.is_none() || b.is_none() || a == b,
                "{}",
                crate::i18n::format("host-app-lifecycle-0003", &[])
            );
            Some(Operation::Resume(a.clone().or(b.clone())))
        } else {
            None
        };
        ensure!(
            combined.is_some()
                || matches!(base.operation, Operation::None)
                || matches!(extra.operation, Operation::None),
            "{}",
            crate::i18n::format("host-app-lifecycle-0004", &[])
        );
        base.command_count += extra.command_count;
        ensure!(
            base.command_count <= 32,
            "{}",
            crate::i18n::format("host-app-lifecycle-0005", &[])
        );
        base.state = extra.state;
        base.settings = extra.settings;
        base.profile = extra.profile;
        if let Some(operation) = combined {
            base.operation = operation;
        } else if !matches!(extra.operation, Operation::None) {
            base.operation = extra.operation;
        }
        base.message = extra.message.or(base.message);
        base.auto_play = extra.auto_play.or(base.auto_play);
        base.transition = extra.transition.or(base.transition);
        base.animations.extend(extra.animations);
        base.portraits.extend(extra.portraits);
        base.live2d.extend(extra.live2d);
        base.ui_actions.extend(extra.ui_actions);
        ensure!(
            base.ui_actions
                .iter()
                .filter(|a| matches!(a, Action::Sequence(_)))
                .count()
                <= 1,
            "{}",
            crate::i18n::format("host-app-lifecycle-0006", &[])
        );
        Ok(base)
    }
    pub(super) fn lifecycle_hook(
        &self,
        plan: Plan,
        session: &Session,
        phase: &str,
        id: &str,
    ) -> Result<Plan> {
        let Some(key) = self
            .custom
            .definition
            .lifecycle
            .get(&format!("{phase}:{id}"))
        else {
            return Ok(plan);
        };
        let available = self
            .logic
            .options(&self.story, session)
            .map_err(anyhow::Error::msg)?;
        let snapshot = interface::Data::build(interface::data::Input {
            definition: &self.custom.definition,
            story: &self.story,
            session,
            available: &available,
            state: &plan.state,
            profile: &plan.profile,
            slots: &self.custom.slots,
            settings: &plan.settings,
            complete: true,
        });
        let commands = self.logic.ui_event(
            &interface::Event {
                sound: None,
                route: None,
                key: key.clone(),
                item: serde_json::Value::Null,
                value: serde_json::Value::Null,
            },
            session,
            &plan.state.variables,
            &snapshot.values,
        )?;
        for command in &commands {
            ensure!(
                matches!(
                    command,
                    Command::Open { .. }
                        | Command::Close
                        | Command::SetUi { .. }
                        | Command::Message { .. }
                        | Command::PlaySound { .. }
                        | Command::Sequence { .. }
                        | Command::Present { .. }
                ) || (phase == "screen.close" && matches!(command, Command::Resume { .. })),
                "{}",
                crate::i18n::format("host-app-lifecycle-0007", &[("arg0", phase.to_string())])
            );
        }
        let extra = host::prepare(
            &self.custom.definition,
            &plan.state,
            &plan.settings,
            &plan.profile,
            commands,
            false,
        )?;
        self.merge_lifecycle(plan, extra)
    }
    pub(super) fn page_lifecycle(
        &self,
        mut plan: Plan,
        session: &Session,
        initial: Option<Vec<String>>,
    ) -> Result<Plan> {
        let mut current = initial.unwrap_or_else(|| self.custom.state.stack.clone());
        let mut budget = 0;
        while current != plan.state.stack {
            budget += 1;
            ensure!(
                budget <= 16,
                "{}",
                crate::i18n::format("host-app-lifecycle-0008", &[])
            );
            let common = current
                .iter()
                .zip(&plan.state.stack)
                .take_while(|(a, b)| a == b)
                .count();
            if current.len() > common {
                let page = current.pop().unwrap();
                plan = self.lifecycle_hook(plan, session, "screen.close", &page)?;
            } else {
                let page = plan.state.stack[current.len()].clone();
                current.push(page.clone());
                plan = self.lifecycle_hook(plan, session, "screen.open", &page)?;
            }
        }
        Ok(plan)
    }
    fn checkpoint(&self, continuation: &Session, plan: &Plan) -> Result<Checkpoint> {
        Ok(Checkpoint {
            schema: 1,
            active: true,
            exited: continuation != &self.session,
            entered: continuation == &self.session && self.effects.has_entered,
            source: Snapshot::capture_committed(&self.story, &self.session)?,
            continuation: Snapshot::capture_committed(&self.story, continuation)?,
            stack: plan.state.stack.clone(),
            origin: self.custom.state.stack.clone(),
            variables: plan.state.variables.clone(),
            sequence: plan.ui_actions.iter().find_map(|a| {
                if let Action::Sequence(id) = a {
                    Some(id.clone())
                } else {
                    None
                }
            }),
        })
    }
    pub(super) fn begin_handoff(
        &mut self,
        continuation: &Session,
        plan: Plan,
        origin: Vec<String>,
    ) -> Result<()> {
        ensure!(
            self.handoff.is_none(),
            "{}",
            crate::i18n::format("host-app-lifecycle-0009", &[])
        );
        let mut plan = plan;
        ensure!(
            matches!(plan.operation, Operation::None | Operation::Present),
            "{}",
            crate::i18n::format("host-app-lifecycle-0010", &[])
        );
        plan.operation = Operation::None;
        let prepared = self.prepare_ui_media(&plan)?;
        let mut checkpoint = self.checkpoint(continuation, &plan)?;
        checkpoint.origin = origin;
        crate::save::atomic_json(
            &self.saves.directory().join("flow-checkpoint.json"),
            &checkpoint,
        )?;
        self.handoff = Some(checkpoint);
        self.pending_auto = None;
        self.auto_play.set(false);
        self.commit_ui(plan, true);
        self.apply_ui_media(prepared);
        self.lifecycle_ready = true;
        Ok(())
    }

    pub(super) fn commit_handoff_file(&mut self, checkpoint: Checkpoint) -> Result<()> {
        crate::save::atomic_json(
            &self.saves.directory().join("flow-checkpoint.json"),
            &checkpoint,
        )?;
        self.handoff = checkpoint.active.then_some(checkpoint);
        self.lifecycle_ready = true;
        Ok(())
    }
    pub(super) fn begin_enter_handoff(
        &mut self,
        session: Session,
        available: Vec<String>,
        mut plan: Plan,
        origin: Vec<String>,
    ) -> Result<()> {
        ensure!(
            self.handoff.is_none()
                && matches!(plan.operation, Operation::None | Operation::Present),
            "{}",
            crate::i18n::format("host-app-lifecycle-0011", &[])
        );
        plan.operation = Operation::None;
        let mut checkpoint = self.checkpoint(&session, &plan)?;
        checkpoint.origin = origin;
        checkpoint.source = checkpoint.continuation.clone();
        checkpoint.exited = false;
        checkpoint.entered = true;
        let ui_media = self.prepare_ui_media(&plan)?;
        let live2d = crate::live2d::Live2DPlayer::prepare_with(
            &self.story.frames[&session.frame_id],
            &self.prepared_live2d,
            self.renderer.clone(),
        )?;
        let entry = PreparedEntry {
            session,
            available,
            restored: false,
            force_audio: false,
            plan,
            ui_media,
            live2d,
            handoff_commit: Some(checkpoint),
        };
        if let Some(entry) = self.debug_break(entry) {
            self.commit_entry(entry)?;
        }
        Ok(())
    }
    pub(super) fn persist_page_plan(&mut self, plan: &Plan) -> Result<()> {
        let Some(mut checkpoint) = self.handoff.clone() else {
            return Ok(());
        };
        ensure!(
            matches!(plan.operation, Operation::None),
            "{}",
            crate::i18n::format("host-app-lifecycle-0012", &[])
        );
        checkpoint.stack = plan.state.stack.clone();
        checkpoint.variables = plan.state.variables.clone();
        if let Some(id) = plan.ui_actions.iter().find_map(|a| {
            if let Action::Sequence(id) = a {
                Some(id.clone())
            } else {
                None
            }
        }) {
            checkpoint.sequence = Some(id);
        } else if !self.ui_media.active() {
            checkpoint.sequence = None;
        }
        if checkpoint.stack == checkpoint.origin {
            checkpoint.active = false;
        }
        crate::save::atomic_json(
            &self.saves.directory().join("flow-checkpoint.json"),
            &checkpoint,
        )?;
        self.handoff = checkpoint.active.then_some(checkpoint);
        Ok(())
    }
    pub(super) fn commit_page_plan(&mut self, mut plan: Plan) -> Result<()> {
        match plan.operation.clone() {
            Operation::Present if !self.effects.has_entered => plan.operation = Operation::None,
            Operation::Present => {
                return self.begin_handoff(
                    &self.session.clone(),
                    plan,
                    self.custom.state.stack.clone(),
                );
            }
            Operation::Resume(target) => return self.resume_handoff(plan, target),
            _ => {}
        }
        ensure!(
            matches!(plan.operation, Operation::None),
            "{}",
            crate::i18n::format("host-app-lifecycle-0013", &[])
        );
        let video = plan.ui_actions.iter().any(|a| {
            if let Action::Sequence(id) = a {
                self.custom.definition.media.sequences[id]
                    .steps
                    .iter()
                    .any(|s| matches!(s, interface::presentation::Step::Video(_)))
            } else {
                false
            }
        });
        if video && self.effects.has_entered && self.handoff.is_none() {
            return self.begin_handoff(
                &self.session.clone(),
                plan,
                self.custom.state.stack.clone(),
            );
        }
        plan.operation = Operation::None;
        let prepared = self.prepare_ui_media(&plan)?;
        self.persist_page_plan(&plan)?;
        let refresh = plan.state.stack != self.custom.state.stack
            || plan.state.slot_page != self.custom.state.slot_page;
        self.commit_ui(plan, refresh);
        self.apply_ui_media(prepared);
        Ok(())
    }
    fn resume_handoff(&mut self, mut plan: Plan, target: Option<String>) -> Result<()> {
        ensure!(
            plan.ui_actions.is_empty(),
            "{}",
            crate::i18n::format("host-app-lifecycle-0014", &[])
        );
        if !self.effects.has_entered {
            let target = target.context(crate::i18n::text("host-app-lifecycle-0033"))?;
            let session = self.story.start_at(&target).map_err(anyhow::Error::msg)?;
            plan.operation = Operation::None;
            return self.enter_planned(session, false, true, Some(plan));
        }
        let mut checkpoint = if let Some(c) = &self.handoff {
            c.clone()
        } else {
            self.checkpoint(&self.session, &plan)?
        };
        // Read the durable slot again: a damaged file must never be replaced by an unchecked jump.
        if self.handoff.is_some() {
            checkpoint = serde_json::from_slice(&crate::save::read_json_bytes(
                &self.saves.directory().join("flow-checkpoint.json"),
            )?)?;
            ensure!(
                checkpoint.schema == 1 && checkpoint.active,
                "{}",
                crate::i18n::format("host-app-lifecycle-0015", &[])
            );
            let (source, _, _) = self.validated_checkpoint(&checkpoint)?;
            ensure!(
                source == self.session,
                "{}",
                crate::i18n::format("host-app-lifecycle-0016", &[])
            );
        } else {
            let path = self.saves.directory().join("flow-checkpoint.json");
            if path.exists() {
                checkpoint = serde_json::from_slice(&crate::save::read_json_bytes(&path)?)?;
                self.validated_checkpoint(&checkpoint)?;
            } else {
                ensure!(
                    target.is_some(),
                    "{}",
                    crate::i18n::format("host-app-lifecycle-0017", &[])
                );
            }
        }
        let mut target = target;
        if plan.state.stack == self.custom.state.stack && plan.state.stack != checkpoint.origin {
            plan.state.stack = checkpoint.origin.clone();
            plan = self.page_lifecycle(plan, &self.session, None)?;
            if let Operation::Resume(frame) = &plan.operation {
                if let (Some(original), Some(closing)) = (&target, frame) {
                    ensure!(
                        self.story.resolve_frame(original) == self.story.resolve_frame(closing),
                        "{}",
                        crate::i18n::format("host-app-lifecycle-0018", &[])
                    );
                }
                target = target.or_else(|| frame.clone());
            }
        }
        let mut session = checkpoint
            .continuation
            .restore_with(&self.story, &self.logic)?;
        if let Some(target) = target {
            let actual = self
                .story
                .resolve_frame(&target)
                .context(crate::i18n::text("host-app-lifecycle-0034"))?;
            if actual != session.frame_id {
                session = controller::transition_with(
                    &self.story,
                    &session,
                    controller::Intent::Resume(actual.into()),
                    &self.logic,
                )?
                .session;
            }
        }
        let terminal_exit = checkpoint.terminal_exit();
        let already_entered = (checkpoint.entered || terminal_exit)
            && session.frame_id == checkpoint.continuation.frame_id;
        plan.operation = Operation::None;
        let return_stack = plan.state.stack.clone();
        let available = self
            .logic
            .options(&self.story, &session)
            .map_err(anyhow::Error::msg)?;
        if !checkpoint.exited && session != self.session {
            plan =
                self.lifecycle_hook(plan, &self.session, "frame.exit", &self.session.frame_id)?;
        }
        let mut entry_plan = if already_entered {
            plan
        } else {
            self.prepare_ui_entry(&session, &available, false, Some(plan))?
        };
        if !already_entered {
            entry_plan =
                self.lifecycle_hook(entry_plan, &session, "frame.enter", &session.frame_id)?;
        }
        ensure!(
            matches!(entry_plan.operation, Operation::None | Operation::Present),
            "{}",
            crate::i18n::format("host-app-lifecycle-0019", &[])
        );
        let next_handoff = entry_plan.state.stack != return_stack
            || entry_plan
                .ui_actions
                .iter()
                .any(|a| matches!(a, Action::Sequence(_)))
            || matches!(entry_plan.operation, Operation::Present);
        if next_handoff {
            entry_plan = self.page_lifecycle(entry_plan, &session, Some(return_stack.clone()))?;
        }
        ensure!(
            matches!(entry_plan.operation, Operation::None | Operation::Present),
            "{}",
            crate::i18n::format("host-app-lifecycle-0020", &[])
        );
        entry_plan.operation = Operation::None;
        let prepared_ui = self.prepare_ui_media(&entry_plan)?;
        let live2d = crate::live2d::Live2DPlayer::prepare_with(
            &self.story.frames[&session.frame_id],
            &self.prepared_live2d,
            self.renderer.clone(),
        )?;
        checkpoint.active = next_handoff;
        checkpoint.continuation = Snapshot::capture_with(&self.story, &session, &self.logic)?;
        checkpoint.source = checkpoint.continuation.clone();
        checkpoint.origin = return_stack;
        checkpoint.stack = entry_plan.state.stack.clone();
        checkpoint.variables = entry_plan.state.variables.clone();
        checkpoint.exited = terminal_exit && already_entered;
        checkpoint.entered = !checkpoint.exited;
        checkpoint.sequence = entry_plan.ui_actions.iter().find_map(|a| {
            if let Action::Sequence(id) = a {
                Some(id.clone())
            } else {
                None
            }
        });
        let entry = PreparedEntry {
            ui_media: prepared_ui,
            handoff_commit: Some(checkpoint),
            session,
            restored: false,
            force_audio: true,
            available,
            plan: entry_plan,
            live2d,
        };
        if let Some(entry) = self.debug_break(entry) {
            self.commit_entry(entry)?;
            self.terminal_exit_done = terminal_exit && already_entered;
        }
        Ok(())
    }

    fn validated_checkpoint(&self, c: &Checkpoint) -> Result<(Session, Session, State)> {
        ensure!(
            c.schema == 1,
            "{}",
            crate::i18n::format("host-app-lifecycle-0021", &[])
        );
        let source = c.source.restore_with(&self.story, &self.logic)?;
        let continuation = c.continuation.restore_with(&self.story, &self.logic)?;
        ensure!(
            !c.entered || (source == continuation && !c.exited),
            "{}",
            crate::i18n::format("host-app-lifecycle-0022", &[])
        );
        ensure!(
            c.exited == (source != continuation)
                || (c.exited
                    && !c.entered
                    && source == continuation
                    && matches!(
                        self.story.routes[&source.frame_id],
                        crate::model::Route::End
                    )),
            "{}",
            crate::i18n::format("host-app-lifecycle-0023", &[])
        );
        if source != continuation {
            let previous = continuation
                .history
                .iter()
                .rev()
                .nth(1)
                .context(crate::i18n::text("host-app-lifecycle-0035"))?;
            ensure!(
                previous.frame_id == source.frame_id && previous.resume_to.is_none(),
                "{}",
                crate::i18n::format("host-app-lifecycle-0024", &[])
            );
            let intent = previous
                .choice_id
                .as_ref()
                .map(|id| controller::Intent::Choose(id.clone()))
                .unwrap_or(controller::Intent::Advance);
            let expected = controller::transition_with(&self.story, &source, intent, &self.logic)?;
            ensure!(
                expected.entered && expected.session == continuation,
                "{}",
                crate::i18n::format("host-app-lifecycle-0025", &[])
            );
        }
        let d = &self.custom.definition;
        ensure!(
            c.variables.len() == d.variables.len(),
            "{}",
            crate::i18n::format("host-app-lifecycle-0026", &[])
        );
        for (id, decl) in &d.variables {
            crate::model::rules::validate_value(
                decl,
                c.variables
                    .get(id)
                    .context(crate::i18n::text("host-app-lifecycle-0036"))?,
            )
            .map_err(anyhow::Error::msg)?;
        }
        for stack in [&c.stack, &c.origin] {
            ensure!(
                !stack.is_empty() && stack.len() <= 8,
                "{}",
                crate::i18n::format("host-app-lifecycle-0027", &[])
            );
            let mut seen = std::collections::BTreeSet::new();
            for (i, id) in stack.iter().enumerate() {
                let page = d
                    .screens
                    .get(id)
                    .context(crate::i18n::text("host-app-lifecycle-0037"))?;
                ensure!(
                    seen.insert(id) && page.modal == (i > 0),
                    "{}",
                    crate::i18n::format("host-app-lifecycle-0028", &[])
                );
            }
        }
        ensure!(
            c.sequence
                .as_ref()
                .is_none_or(|id| d.media.sequences.contains_key(id)),
            "{}",
            crate::i18n::format("host-app-lifecycle-0029", &[])
        );
        let mut state = State::new(d);
        state.stack = c.stack.clone();
        state.variables = c.variables.clone();
        let checked = host::prepare(
            d,
            &state,
            &self.settings,
            &self.custom.profile,
            vec![],
            false,
        )?;
        Ok((source, continuation, checked.state))
    }
    pub(super) fn restore_handoff(&mut self) -> Result<bool> {
        let path = self.saves.directory().join("flow-checkpoint.json");
        if !path.exists() {
            return Ok(false);
        }
        let result = (|| -> Result<bool> {
            let checkpoint: Checkpoint =
                serde_json::from_slice(&crate::save::read_json_bytes(&path)?)?;
            ensure!(
                checkpoint.schema == 1,
                "{}",
                crate::i18n::format("host-app-lifecycle-0030", &[])
            );
            if !checkpoint.active {
                return Ok(false);
            }
            let (session, _, state) = self.validated_checkpoint(&checkpoint)?;
            let d = &self.custom.definition;
            let mut plan = host::prepare(
                d,
                &state,
                &self.settings,
                &self.custom.profile,
                vec![],
                false,
            )?;
            plan = self.page_lifecycle(plan, &session, Some(vec![]))?;
            if !plan
                .ui_actions
                .iter()
                .any(|a| matches!(a, Action::Sequence(_)))
                && let Some(id) = &checkpoint.sequence
            {
                let extra = host::prepare(
                    d,
                    &plan.state,
                    &plan.settings,
                    &plan.profile,
                    vec![Command::Sequence { id: id.clone() }],
                    false,
                )?;
                plan = self.merge_lifecycle(plan, extra)?;
            }
            ensure!(
                matches!(plan.operation, Operation::None),
                "{}",
                crate::i18n::format("host-app-lifecycle-0031", &[])
            );
            let available = self
                .logic
                .options(&self.story, &session)
                .map_err(anyhow::Error::msg)?;

            let live2d = crate::live2d::Live2DPlayer::prepare_with(
                &self.story.frames[&session.frame_id],
                &self.prepared_live2d,
                self.renderer.clone(),
            )?;
            let prepared_ui = self.prepare_ui_media(&plan)?;
            self.commit_entry(PreparedEntry {
                ui_media: prepared_ui,
                handoff_commit: None,
                session,
                restored: true,
                force_audio: true,
                available,
                plan,
                live2d,
            })?;
            self.pending_auto = None;
            self.terminal_exit_done = checkpoint.terminal_exit();
            self.handoff = Some(checkpoint);
            self.lifecycle_ready = true;
            Ok(true)
        })();
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                self.lifecycle_ready = true;
                let text = crate::i18n::format(
                    "host-app-lifecycle-0032",
                    &[("arg0", format!("{:#}", error))],
                );
                self.message = Some(
                    self.message
                        .take()
                        .map(|s| format!("{s}\n{text}"))
                        .unwrap_or(text),
                );
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    /// Generates the XML controls a test script registers, so every named target exists.
    fn probes(script: &str) -> String {
        let mut out = String::new();
        let mut rest = script;
        while let Some(at) = rest.find("ui('") {
            rest = &rest[at + 4..];
            let Some(end) = rest.find("')") else { break };
            let target = &rest[..end];
            rest = &rest[end + 2..];
            let Some(bound) = rest.strip_prefix(".bind('") else {
                continue;
            };
            let Some(eend) = bound.find('\'') else { break };
            let event = &bound[..eend];
            rest = &bound[eend + 1..];
            if target.starts_with('$') || matches!(event, "screen.open" | "screen.close") {
                continue;
            }
            out.push_str(&format!(
                r#"<button id="{target}" width="10" height="10" text="按钮" event="{event}"/>"#
            ));
        }
        out
    }
    fn write_game(root: &Path, extra: &str) -> Result<()> {
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/fonts/chinese.otf"),
            root.join("font.otf"),
        )?;
        std::fs::write(
            root.join("story.xml"),
            r#"<story schema="8" id="lifecycle" content-version="9" entry="a"><scripts><script src="story.js"/></scripts><interface src="ui.xml"/><assets><font id="font" src="font.otf"/></assets><variables><int id="n" initial="0" min="0" max="100"/></variables><characters/><visuals/><frames><frame id="a"><text>甲</text></frame><frame id="b"><text>乙</text></frame><frame id="c"><text>丙</text></frame></frames><next end="true"/></story>"#,
        )?;
        std::fs::write(
            root.join("ui.xml"),
            format!(
                r#"<ui schema="3" initial="hud"><variables><string id="trace" initial=""/></variables><ui-media><sequence id="cut" finish="close"><fade target="caption" duration="1000"/></sequence></ui-media><screen id="hud" modal="false" pause="false"><scene id="scene" width="1280" height="720"/><button id="advance" width="10" height="10" text="按钮" event="advance"/>{}</screen><screen id="movie"><label id="caption" width="400" height="50" text="视频演出"/></screen></ui>"#,
                probes(extra)
            ),
        )?;
        std::fs::write(
            root.join("story.js"),
            format!(
                "frame('a').advance(ctx=>{{ctx.add('n',1);return 'b';}});frame('b').end();frame('c').end();frame('a').enter(ctx=>ctx.setUi('trace',ctx.ui.trace+'A'));frame('b').enter(ctx=>ctx.setUi('trace',ctx.ui.trace+'B'));(event.register('advance', ctx=>ctx.advance()), ui('advance').bind('advance'));{extra}"
            ),
        )?;
        Ok(())
    }
    fn load(root: &Path, saves: &Path) -> Result<PlayerApp> {
        let mut app = load_ui(root, saves)?;
        if !app.effects.has_entered {
            app.enter(app.story.start(), false, true)?;
        }
        app.pending_auto = None;
        app.lifecycle_ready = true;
        app.typewriter.finish();
        app.effects.finish();
        Ok(app)
    }
    fn load_ui(root: &Path, saves: &Path) -> Result<PlayerApp> {
        let b = crate::runtime::Bundle::load(
            &crate::package::Snapshot::directory(root)?,
            &crate::runtime::LoadOptions {
                core: None,
                no_live2d: true,
            },
        )?;
        let app = PlayerApp::new_context(
            &egui::Context::default(),
            b.loaded,
            b.assets,
            SaveStore::at(saves.to_owned()),
            true,
            b.logic,
            crate::render::Renderer::cpu(),
        )?;
        Ok(app)
    }
    fn standard(target: &str) -> String {
        format!(
            "frame('a').exit(ctx=>{{ctx.setUi('trace',ctx.ui.trace+'X');ctx.open('movie');}});ui('movie').bind('screen.open', ctx=>{{ctx.setUi('trace',ctx.ui.trace+'O');ctx.sequence('cut');}});ui('movie').bind('screen.close', ctx=>{{ctx.setUi('trace',ctx.ui.trace+'C');ctx.resume({target});}});"
        )
    }
    fn advance(app: &mut PlayerApp) -> Result<()> {
        app.custom_event(interface::Event::ui("advance", "advance"))
    }
    fn skip(app: &mut PlayerApp) -> Result<bool> {
        let ctx = app.context.clone();
        let mut result = Ok(false);
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Space,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
                ..Default::default()
            },
            |ctx| {
                result = app.tick_ui_media(ctx, true, true);
            },
        );
        result
    }
    #[test]
    fn screen_sequence_naturally_resumes_after_focus_and_debug_pause() -> Result<()> {
        for target in ["", "'c'", "page"] {
            let root = tempfile::tempdir()?;
            let saves = tempfile::tempdir()?;
            let close = if target == "page" {
                "ctx.open('second')".to_owned()
            } else {
                format!("ctx.resume({target})")
            };
            write_game(
                root.path(),
                &format!(
                    "frame('a').exit(ctx=>ctx.open('movie'));ui('movie').bind('screen.close', ctx=>{{{close};}});ui('second').bind('screen.close', ctx=>ctx.resume('c'));"
                ),
            )?;
            let path = root.path().join("ui.xml");
            let xml = std::fs::read_to_string(&path)?
                .replace("<screen id=\"movie\">", "<screen id=\"movie\" sequence=\"cut\">")
                .replace("</ui-media>", "<sequence id=\"second.cut\" finish=\"close\"><fade target=\"second.caption\" duration=\"200\"/></sequence></ui-media>")
                .replace("</ui>", "<screen id=\"second\" sequence=\"second.cut\"><label id=\"second.caption\" width=\"200\" height=\"40\" text=\"第二页\"/></screen></ui>");
            std::fs::write(path, xml)?;
            let mut app = load(root.path(), saves.path())?;
            app.attach_debug(crate::debug::Server::bind(
                "127.0.0.1:0".parse()?,
                "0123456789abcdef0123456789abcdef".into(),
            )?);
            app.debug_command(
                crate::debug::Command::Advance,
                app.debug_snapshot()["state_revision"].as_u64(),
            )?;
            let before = app.session.clone();
            let ctx = app.context.clone();
            assert!(app.tick_ui_media(&ctx, true, false)?);
            assert!(app.ui_media.active());
            assert_eq!(app.custom.state.stack, vec!["hud", "movie"]);
            // A paused debugger and a background window must both preserve the handoff.
            for focused in [true, false] {
                app.ui_media.elapse_for_test(20);
                app.tick_ui_media(&ctx, focused, false)?;
                assert_eq!(app.session, before);
                assert!(app.handoff.is_some());
            }
            app.debug_command(
                crate::debug::Command::Resume,
                app.debug_snapshot()["state_revision"].as_u64(),
            )?;
            app.ui_media.elapse_for_test(20);
            app.tick_ui_media(&ctx, false, false)?;
            assert_eq!(app.session, before);
            // Regaining focus discards the paused interval rather than catching up.
            app.tick_ui_media(&ctx, true, false)?;
            assert_eq!(app.session, before);
            app.ui_media.elapse_for_test(2);
            app.tick_ui_media(&ctx, true, false)?;
            if target == "page" {
                assert_eq!(app.custom.state.stack, vec!["hud", "second"]);
                assert_eq!(app.session, before);
                app.tick_ui_media(&ctx, true, false)?;
                app.ui_media.elapse_for_test(2);
                app.tick_ui_media(&ctx, true, false)?;
            }
            assert_eq!(
                app.session.frame_id,
                if target.is_empty() { "b" } else { "c" }
            );
            assert_eq!(
                app.session.variables["n"],
                crate::model::VariableValue::Int(1)
            );
            assert_eq!(app.custom.state.stack, vec!["hud"]);
            assert!(app.handoff.is_none());
            let committed = app.session.clone();
            assert!(!app.tick_ui_media(&ctx, true, false)?);
            assert_eq!(app.session, committed);
            app.story
                .validate_session_with(&app.session, &app.logic)
                .map_err(anyhow::Error::msg)?;
            let slot: Checkpoint =
                serde_json::from_slice(&std::fs::read(saves.path().join("flow-checkpoint.json"))?)?;
            assert!(!slot.active);
        }
        Ok(())
    }
    #[test]
    fn frame_exit_ui_open_close_resume_commits_continuation_once() -> Result<()> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(root.path(), &standard("'b'"))?;
        let mut app = load(root.path(), saves.path())?;
        let before = app.session.clone();
        advance(&mut app)?;
        assert_eq!(app.session, before);
        assert_eq!(app.custom.state.variables["trace"].to_string(), "AXO");
        assert!(app.handoff.is_some());
        assert_eq!(
            app.handoff.as_ref().unwrap().continuation.variables["n"],
            crate::model::VariableValue::Int(1)
        );
        skip(&mut app)?;
        assert_eq!(app.session.frame_id, "b");
        assert_eq!(
            app.session.variables["n"],
            crate::model::VariableValue::Int(1)
        );
        assert_eq!(app.custom.state.variables["trace"].to_string(), "AXOCB");
        assert!(app.handoff.is_none());
        assert_eq!(app.session.history.len(), 2);
        app.story
            .validate_session_with(&app.session, &app.logic)
            .map_err(anyhow::Error::msg)?;
        let committed = app.session.clone();
        let ctx = app.context.clone();
        assert!(!app.tick_ui_media(&ctx, true, false)?);
        assert_eq!(app.session, committed);
        let slot: Checkpoint =
            serde_json::from_slice(&std::fs::read(saves.path().join("flow-checkpoint.json"))?)?;
        assert!(!slot.active);
        Ok(())
    }
    #[test]
    fn hidden_checkpoint_survives_process_recreation_and_does_not_touch_normal_slots() -> Result<()>
    {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(root.path(), &standard("'c'"))?;
        let mut app = load(root.path(), saves.path())?;
        std::fs::write(saves.path().join("auto-1.json"), b"ordinary-auto")?;
        std::fs::write(saves.path().join("slot-1.json"), b"manual")?;
        advance(&mut app)?;
        drop(app);
        let mut app = load(root.path(), saves.path())?;
        assert_eq!(app.session.frame_id, "a");
        assert!(app.handoff.is_some());
        assert_eq!(app.custom.state.stack, vec!["hud", "movie"]);
        assert!(app.ui_media.active());
        skip(&mut app)?;
        assert_eq!(app.session.frame_id, "c");
        assert_eq!(
            app.session.variables["n"],
            crate::model::VariableValue::Int(1)
        );
        assert_eq!(app.session.history[1].resume_to.as_deref(), Some("c"));
        let saved = Snapshot::capture_with(&app.story, &app.session, &app.logic)?;
        assert_eq!(saved.schema_version, 9);
        assert_eq!(saved.restore_with(&app.story, &app.logic)?, app.session);
        assert_eq!(
            std::fs::read(saves.path().join("auto-1.json"))?,
            b"ordinary-auto"
        );
        assert_eq!(std::fs::read(saves.path().join("slot-1.json"))?, b"manual");
        drop(app);
        let app = load(root.path(), saves.path())?;
        assert!(app.handoff.is_none());
        assert_eq!(app.session.frame_id, "a");
        Ok(())
    }
    #[test]
    fn checkpoint_io_failure_and_lifecycle_cycles_keep_old_state() -> Result<()> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(root.path(), &standard("'b'"))?;
        let mut app = load(root.path(), saves.path())?;
        let before = app.session.clone();
        let ui = app.custom.state.clone();
        let blocked = saves.path().join("flow-checkpoint.json");
        std::fs::create_dir(&blocked)?;
        std::fs::write(blocked.join("marker"), b"old")?;
        assert!(advance(&mut app).is_err());
        assert_eq!(app.session, before);
        assert_eq!(app.custom.state, ui);
        assert_eq!(std::fs::read(blocked.join("marker"))?, b"old");
        let saves2 = tempfile::tempdir()?;
        write_game(
            root.path(),
            "frame('a').exit(ctx=>ctx.open('movie'));ui('movie').bind('screen.open', ctx=>ctx.close());ui('movie').bind('screen.close', ctx=>ctx.open('movie'));",
        )?;
        let mut app = load(root.path(), saves2.path())?;
        let before = app.session.clone();
        assert!(advance(&mut app).unwrap_err().to_string().contains("16"));
        assert_eq!(app.session, before);
        assert_eq!(app.custom.state.stack, vec!["hud"]);
        assert!(!saves2.path().join("flow-checkpoint.json").exists());
        Ok(())
    }
    #[test]
    fn corrupt_hidden_slot_and_bad_resume_keep_session_and_ui() -> Result<()> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(root.path(), &standard("'missing'"))?;
        let mut app = load(root.path(), saves.path())?;
        advance(&mut app)?;
        let before = app.session.clone();
        let ui = app.custom.state.clone();
        let old = std::fs::read(saves.path().join("flow-checkpoint.json"))?;
        assert!(skip(&mut app).is_err());
        assert_eq!(app.session, before);
        assert_eq!(app.custom.state, ui);
        assert_eq!(
            std::fs::read(saves.path().join("flow-checkpoint.json"))?,
            old
        );
        std::fs::write(saves.path().join("flow-checkpoint.json"), b"{}")?;
        drop(app);
        let app = load(root.path(), saves.path())?;
        assert_eq!(app.session.frame_id, "a");
        assert!(app.message.as_ref().unwrap().contains("隐藏交接槽无效"));
        Ok(())
    }
    #[test]
    fn explicit_ui_present_records_return_and_runs_exit_enter() -> Result<()> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(
            root.path(),
            "(event.register('present', ctx=>ctx.present('movie')), ui('present').bind('present'));frame('a').exit(ctx=>ctx.setUi('trace',ctx.ui.trace+'X'));ui('movie').bind('screen.open', ctx=>ctx.sequence('cut'));ui('movie').bind('screen.close', ctx=>ctx.resume('c'));",
        )?;
        let mut app = load(root.path(), saves.path())?;
        app.custom_event(interface::Event::ui("present", "present"))?;
        assert_eq!(app.session.frame_id, "a");
        skip(&mut app)?;
        assert_eq!(app.session.frame_id, "c");
        assert_eq!(
            app.session.variables["n"],
            crate::model::VariableValue::Int(0)
        );
        assert_eq!(app.session.history[0].resume_to.as_deref(), Some("c"));
        assert_eq!(app.custom.state.variables["trace"].to_string(), "AX");
        Ok(())
    }
    #[test]
    fn frame_enter_can_handoff_to_a_second_ui_after_resume() -> Result<()> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(
            root.path(),
            &(standard("'b'")
                + "ui('second').bind('screen.open', ctx=>ctx.sequence('cut2'));ui('second').bind('screen.close', ctx=>ctx.resume('c'));"),
        )?;
        let path = root.path().join("story.js");
        let js = std::fs::read_to_string(&path)?.replace(
            "frame('b').enter(ctx=>ctx.setUi('trace',ctx.ui.trace+'B'));",
            "frame('b').enter(ctx=>{ctx.setUi('trace',ctx.ui.trace+'B');ctx.open('second');});",
        );
        std::fs::write(path, js)?;
        let path = root.path().join("ui.xml");
        let xml=std::fs::read_to_string(&path)?.replace("</ui-media>","<sequence id=\"cut2\" finish=\"close\"><fade target=\"caption2\"/></sequence></ui-media>").replace("</ui>","<screen id=\"second\"><label id=\"caption2\" width=\"200\" height=\"40\" text=\"二\"/></screen></ui>");
        std::fs::write(path, xml)?;
        let mut app = load(root.path(), saves.path())?;
        advance(&mut app)?;
        skip(&mut app)?;
        assert_eq!(app.session.frame_id, "b");
        assert_eq!(app.custom.state.stack, vec!["hud", "second"]);
        assert!(app.handoff.is_some());
        assert!(app.ui_media.active());
        // Release the key so the second sequence sees a separate user action.
        let ctx = app.context.clone();
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        skip(&mut app)?;
        assert_eq!(app.session.frame_id, "c");
        assert!(app.handoff.is_none());
        assert_eq!(
            app.session.variables["n"],
            crate::model::VariableValue::Int(1)
        );
        app.story
            .validate_session_with(&app.session, &app.logic)
            .map_err(anyhow::Error::msg)?;
        Ok(())
    }
    #[test]
    fn initial_frame_and_initial_page_lifecycles_fire_before_ui_handoff() -> Result<()> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(
            root.path(),
            "ui('hud').bind('screen.open', ctx=>ctx.setUi('trace',ctx.ui.trace+'H'));ui('movie').bind('screen.open', ctx=>{ctx.setUi('trace',ctx.ui.trace+'O');ctx.sequence('cut');});ui('movie').bind('screen.close', ctx=>ctx.resume('c'));",
        )?;
        let path = root.path().join("story.js");
        let js = std::fs::read_to_string(&path)?.replace(
            "frame('a').enter(ctx=>ctx.setUi('trace',ctx.ui.trace+'A'));",
            "frame('a').enter(ctx=>{ctx.setUi('trace',ctx.ui.trace+'A');ctx.open('movie');});",
        );
        std::fs::write(path, js)?;
        let mut app = load(root.path(), saves.path())?;
        assert!(app.handoff.is_some());
        assert_eq!(app.custom.state.variables["trace"].to_string(), "AHO");
        skip(&mut app)?;
        assert_eq!(app.session.frame_id, "c");
        Ok(())
    }
    #[test]
    fn entering_a_frame_can_present_ui_and_resume_same_frame_without_reentry_loop() -> Result<()> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(
            root.path(),
            "ui('movie').bind('screen.open', ctx=>ctx.sequence('cut'));ui('movie').bind('screen.close', ctx=>ctx.resume());",
        )?;
        let path = root.path().join("story.js");
        let js = std::fs::read_to_string(&path)?.replace(
            "frame('b').enter(ctx=>ctx.setUi('trace',ctx.ui.trace+'B'));",
            "frame('b').enter(ctx=>{ctx.setUi('trace',ctx.ui.trace+'B');ctx.open('movie');});",
        );
        std::fs::write(path, js)?;
        let mut app = load(root.path(), saves.path())?;
        advance(&mut app)?;
        assert_eq!(app.session.frame_id, "b");
        assert!(app.handoff.as_ref().unwrap().entered);
        assert_eq!(app.custom.state.variables["trace"].to_string(), "AB");
        skip(&mut app)?;
        assert!(app.handoff.is_none());
        assert!(!app.ui_media.active());
        assert_eq!(app.session.frame_id, "b");
        assert_eq!(app.session.history.len(), 2);
        assert_eq!(app.custom.state.variables["trace"].to_string(), "AB");
        Ok(())
    }
    #[test]
    fn return_breakpoint_keeps_active_checkpoint_until_candidate_commits() -> Result<()> {
        use crate::debug::{Command as DebugCommand, Server};
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(root.path(), &standard("'b'"))?;
        let mut app = load(root.path(), saves.path())?;
        app.attach_debug(Server::bind("127.0.0.1:0".parse()?, "t".repeat(32))?);
        let revision = |app: &PlayerApp| app.debug_snapshot()["state_revision"].as_u64();
        app.debug_command(
            DebugCommand::Breakpoints {
                frames: vec!["b".into()],
            },
            revision(&app),
        )?;
        app.debug_command(DebugCommand::Resume, revision(&app))?;
        advance(&mut app)?;
        let old = std::fs::read(saves.path().join("flow-checkpoint.json"))?;
        skip(&mut app)?;
        assert_eq!(app.session.frame_id, "a");
        assert_eq!(app.debug_snapshot()["pending_frame"], "b");
        assert_eq!(
            std::fs::read(saves.path().join("flow-checkpoint.json"))?,
            old
        );
        app.debug_command(DebugCommand::Resume, revision(&app))?;
        assert_eq!(app.session.frame_id, "b");
        assert!(app.handoff.is_none());
        let checkpoint: Checkpoint =
            serde_json::from_slice(&std::fs::read(saves.path().join("flow-checkpoint.json"))?)?;
        assert!(!checkpoint.active);
        Ok(())
    }
    #[test]
    fn tampered_checkpoint_origin_or_source_link_is_rejected_before_resume() -> Result<()> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(root.path(), &standard("'b'"))?;
        let mut app = load(root.path(), saves.path())?;
        advance(&mut app)?;
        let before = app.session.clone();
        let path = saves.path().join("flow-checkpoint.json");
        let bytes = std::fs::read(&path)?;
        let mut bad: serde_json::Value = serde_json::from_slice(&bytes)?;
        bad["origin"] = serde_json::json!(["missing"]);
        std::fs::write(&path, serde_json::to_vec(&bad)?)?;
        assert!(skip(&mut app).is_err());
        assert_eq!(app.session, before);
        let mut bad: Checkpoint = serde_json::from_slice(&bytes)?;
        bad.continuation = bad.source.clone();
        std::fs::write(&path, serde_json::to_vec(&bad)?)?;
        let plan = app.empty_ui_plan()?;
        assert!(app.resume_handoff(plan, Some("b".into())).is_err());
        assert_eq!(app.session, before);
        Ok(())
    }
    #[test]
    fn implicit_resume_closes_page_and_runs_close_callback_once() -> Result<()> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(
            root.path(),
            &(standard("'b'")
                + "(event.register('resume-direct', ctx=>ctx.resume()), ui('resume-direct').bind('resume-direct'));"),
        )?;
        let mut app = load(root.path(), saves.path())?;
        advance(&mut app)?;
        app.ui_media.cancel();
        app.custom_event(interface::Event::ui("resume-direct", "resume-direct"))?;
        assert_eq!(app.session.frame_id, "b");
        assert_eq!(app.custom.state.stack, vec!["hud"]);
        assert_eq!(app.custom.state.variables["trace"].to_string(), "AXOCB");
        assert!(app.handoff.is_none());
        Ok(())
    }
    #[test]
    fn terminal_exit_runs_once_and_survives_handoff_restore() -> Result<()> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(
            root.path(),
            "frame('b').exit(ctx=>{ctx.setUi('trace',ctx.ui.trace+'X');ctx.open('movie');});ui('movie').bind('screen.close', ctx=>ctx.resume());",
        )?;
        let mut app = load(root.path(), saves.path())?;
        advance(&mut app)?;
        app.typewriter.finish();
        app.effects.finish();
        let before = app.session.clone();
        advance(&mut app)?;
        assert_eq!(app.session, before);
        assert!(app.handoff.as_ref().unwrap().terminal_exit());
        drop(app);
        let mut app = load(root.path(), saves.path())?;
        let mut plan = app.empty_ui_plan()?;
        plan.operation = Operation::Resume(None);
        app.resume_handoff(plan, None)?;
        app.typewriter.finish();
        app.effects.finish();
        assert!(app.terminal_exit_done);
        assert_eq!(app.session, before);
        let trace = app.custom.state.variables["trace"].clone();
        advance(&mut app)?;
        assert_eq!(app.custom.state.variables["trace"], trace);
        assert!(app.handoff.is_none());
        Ok(())
    }
    #[test]
    fn terminal_exit_failure_is_retryable_and_keyboard_uses_same_path() -> Result<()> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(root.path(), "frame('b').exit(ctx=>ctx.open('movie'));")?;
        let mut app = load(root.path(), saves.path())?;
        advance(&mut app)?;
        app.typewriter.finish();
        app.effects.finish();
        let before = app.session.clone();
        std::fs::create_dir(saves.path().join("flow-checkpoint.json"))?;
        assert!(
            app.handle(UiIntent::Play(controller::Intent::Advance))
                .is_err()
        );
        assert!(!app.terminal_exit_done);
        assert_eq!(app.session, before);
        assert_eq!(app.custom.state.stack, vec!["hud"]);
        std::fs::remove_dir(saves.path().join("flow-checkpoint.json"))?;
        app.handle(UiIntent::Play(controller::Intent::Advance))?;
        assert!(app.handoff.is_some());
        Ok(())
    }
    #[test]
    fn terminal_exit_plain_callback_and_explicit_resume_do_not_repeat_exit() -> Result<()> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(
            root.path(),
            "frame('b').exit(ctx=>ctx.setUi('trace',ctx.ui.trace+'X'));",
        )?;
        let mut app = load(root.path(), saves.path())?;
        advance(&mut app)?;
        app.typewriter.finish();
        app.effects.finish();
        advance(&mut app)?;
        advance(&mut app)?;
        assert_eq!(app.custom.state.variables["trace"].to_string(), "ABX");
        app.enter(app.session.clone(), true, true)?;
        app.typewriter.finish();
        app.effects.finish();
        advance(&mut app)?;
        assert_eq!(app.custom.state.variables["trace"].to_string(), "ABXX");
        write_game(
            root.path(),
            "frame('b').exit(ctx=>{ctx.setUi('trace',ctx.ui.trace+'X');ctx.open('movie');});ui('movie').bind('screen.close', ctx=>ctx.resume('c'));",
        )?;
        let mut app = load(root.path(), saves.path())?;
        advance(&mut app)?;
        app.typewriter.finish();
        app.effects.finish();
        advance(&mut app)?;
        let mut plan = app.empty_ui_plan()?;
        plan.operation = Operation::Resume(None);
        app.resume_handoff(plan, None)?;
        assert_eq!(app.session.frame_id, "c");
        assert_eq!(app.custom.state.variables["trace"].to_string(), "ABX");
        Ok(())
    }
    #[test]
    fn ui_resume_uses_durable_continuation_and_preserves_explicit_target() -> Result<()> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(
            root.path(),
            "frame('a').exit(ctx=>ctx.open('movie'));ui('movie').bind('screen.close', ctx=>ctx.resume());(event.register('start', ctx=>ctx.resume('c')), ui('start').bind('start'));(event.register('continue', ctx=>ctx.resume()), ui('continue').bind('continue'));",
        )?;
        let mut app = load(root.path(), saves.path())?;
        advance(&mut app)?;
        let mut plan = app.empty_ui_plan()?;
        plan.operation = Operation::Resume(Some("c".into()));
        app.resume_handoff(plan, Some("c".into()))?;
        assert_eq!(app.session.frame_id, "c");
        let saved = std::fs::read(saves.path().join("flow-checkpoint.json"))?;
        let checkpoint: Checkpoint = serde_json::from_slice(&saved)?;
        assert_eq!(checkpoint.continuation.frame_id, "c");
        assert_eq!(
            checkpoint.continuation.variables["n"],
            crate::model::VariableValue::Int(1)
        );
        app.enter(app.story.start(), true, true)?;
        let mut plan = app.empty_ui_plan()?;
        plan.operation = Operation::Resume(None);
        app.resume_handoff(plan, None)?;
        assert_eq!(app.session.frame_id, "c");
        assert_eq!(
            app.session.variables["n"],
            crate::model::VariableValue::Int(1)
        );
        let before = app.session.clone();
        let bytes = std::fs::read(saves.path().join("flow-checkpoint.json"))?;
        let mut plan = app.empty_ui_plan()?;
        plan.operation = Operation::Resume(Some("missing".into()));
        assert!(app.resume_handoff(plan, Some("missing".into())).is_err());
        assert_eq!(app.session, before);
        assert_eq!(
            std::fs::read(saves.path().join("flow-checkpoint.json"))?,
            bytes
        );
        Ok(())
    }
    #[test]
    fn ui_resume_requires_a_checkpoint_or_explicit_start() -> Result<()> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(root.path(), "")?;
        let mut app = load(root.path(), saves.path())?;
        let plan = app.empty_ui_plan()?;
        assert!(app.resume_handoff(plan, None).is_err());
        let plan = app.empty_ui_plan()?;
        app.resume_handoff(plan, Some("c".into()))?;
        assert_eq!(app.session.frame_id, "c");
        assert!(saves.path().join("flow-checkpoint.json").is_file());
        Ok(())
    }
    #[test]
    fn quit_is_committed_only_after_validation_and_preserves_active_checkpoint() -> Result<()> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(
            root.path(),
            &(standard("'b'")
                + "(event.register('quit', ctx=>ctx.quit()), ui('quit').bind('quit'));(event.register('bad-quit', ctx=>{ctx.setUi('trace','changed');ctx.quit();ctx.setUi('missing',true);}), ui('bad-quit').bind('bad-quit'));(event.register('mixed-quit', ctx=>{ctx.quit();ctx.advance();}), ui('mixed-quit').bind('mixed-quit'));(event.register('media-quit', ctx=>{ctx.quit();ctx.sequence('cut');}), ui('media-quit').bind('media-quit'));"),
        )?;
        let event = |name: &str| interface::Event::ui(name, name);
        let mut app = load_ui(root.path(), saves.path())?;
        let before = app.session.clone();
        let state = app.custom.state.clone();
        for name in ["bad-quit", "mixed-quit", "media-quit"] {
            assert!(app.custom_event(event(name)).is_err());
            assert!(!app.quit_requested());
            assert_eq!(app.session, before);
            assert_eq!(app.custom.state, state);
        }
        app.custom_event(event("quit"))?;
        assert!(app.quit_requested());
        assert!(!app.effects.has_entered);
        assert!(!saves.path().join("flow-checkpoint.json").exists());
        let mut app = load(root.path(), saves.path())?;
        advance(&mut app)?;
        let hidden = saves.path().join("flow-checkpoint.json");
        let bytes = std::fs::read(&hidden)?;
        let session = app.session.clone();
        app.custom_event(event("quit"))?;
        assert!(app.quit_requested());
        assert!(app.handoff.is_some());
        assert_eq!(app.session, session);
        app.shutdown();
        assert_eq!(std::fs::read(hidden)?, bytes);
        let restored = load_ui(root.path(), saves.path())?;
        assert!(!restored.quit_requested());
        assert!(restored.handoff.is_some());
        Ok(())
    }

    #[test]
    fn frame_and_screen_lifecycles_cannot_quit_the_host() -> Result<()> {
        for extra in [
            "frame('a').exit(ctx=>ctx.quit());",
            "frame('c').enter(ctx=>ctx.quit());",
        ] {
            let root = tempfile::tempdir()?;
            let saves = tempfile::tempdir()?;
            write_game(root.path(), extra)?;
            if extra.contains("frame(\'c\').enter") {
                let path = root.path().join("story.js");
                std::fs::write(
                    &path,
                    std::fs::read_to_string(&path)?.replace("return 'b';", "return 'c';"),
                )?;
            }
            let mut app = load(root.path(), saves.path())?;
            let before = app.session.clone();
            assert!(advance(&mut app).is_err());
            assert!(!app.quit_requested());
            assert_eq!(app.session, before);
        }
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(
            root.path(),
            "(event.register('open', ctx=>ctx.open('movie')), ui('open').bind('open'));ui('movie').bind('screen.open', ctx=>ctx.quit());",
        )?;
        let mut app = load(root.path(), saves.path())?;
        let state = app.custom.state.clone();
        assert!(
            app.custom_event(interface::Event::ui("open", "open"))
                .is_err()
        );
        assert_eq!(app.custom.state, state);
        assert!(!app.quit_requested());
        Ok(())
    }

    #[test]
    fn title_to_hud_navigation_is_not_a_story_handoff_and_hooks_run_once() -> Result<()> {
        for movie in [false, true] {
            let root = tempfile::tempdir()?;
            let saves = tempfile::tempdir()?;
            write_game(
                root.path(),
                &format!(
                    "(event.register('start', ctx=>{{ctx.open('hud');ctx.resume('c');}}), ui('start').bind('start'));(event.register('restart-main', ctx=>{{ctx.open('hud');ctx.restart('a');}}), ui('restart-main').bind('restart-main'));(event.register('title', ctx=>ctx.open('title')), ui('titleButton').bind('title'));ui('hud').bind('screen.open', ctx=>ctx.setUi('trace',ctx.ui.trace+'H'));frame('c').enter(ctx=>{{ctx.setUi('trace',ctx.ui.trace+'C');{}}});ui('movie').bind('screen.close', ctx=>ctx.resume());",
                    if movie { "ctx.open('movie');" } else { "" }
                ),
            )?;
            let path = root.path().join("story.xml");
            std::fs::write(
                &path,
                std::fs::read_to_string(&path)?.replace("entry=\"a\"", "entry=\"a c\""),
            )?;
            let path = root.path().join("ui.xml");
            std::fs::write(
                &path,
                std::fs::read_to_string(&path)?
                    .replace("initial=\"hud\"", "initial=\"title\"")
                    .replace("</ui>", "<screen id=\"title\" modal=\"false\"/></ui>"),
            )?;
            let mut app = load_ui(root.path(), saves.path())?;
            app.lifecycle_ready = true;
            let event = |name: &str| {
                if name == "title" {
                    interface::Event::ui("titleButton", name)
                } else {
                    interface::Event::ui(name, name)
                }
            };
            app.custom_event(event("start"))?;
            assert_eq!(app.session.frame_id, "c");
            assert_eq!(app.custom.state.variables["trace"].to_string(), "HC");
            assert_eq!(app.handoff.is_some(), movie);
            if movie {
                assert_eq!(app.handoff.as_ref().unwrap().origin, ["hud"]);
                app.resume_handoff(app.empty_ui_plan()?, None)?;
            } else {
                assert!(!saves.path().join("flow-checkpoint.json").exists());
            }
            assert_eq!(app.custom.state.stack, ["hud"]);
            assert!(app.handoff.is_none());
            assert!(app.pending_auto.is_some());
            app.custom_event(event("title"))?;
            app.custom_event(event("restart-main"))?;
            assert_eq!(app.session.frame_id, "a");
            assert_eq!(app.session.history.len(), 1);
            assert_eq!(app.custom.state.stack, ["hud"]);
            assert!(app.handoff.is_none());
            assert_eq!(app.custom.state.variables["trace"].to_string(), "HCHA");
        }
        Ok(())
    }

    #[test]
    fn multiple_entries_wait_for_explicit_ui_selection_without_default_history() -> Result<()> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(
            root.path(),
            "frame('c').enter(ctx=>ctx.setUi('trace',ctx.ui.trace+'C'));(event.register('start', ctx=>ctx.resume('secret')), ui('start').bind('start'));(event.register('bare', ctx=>ctx.resume()), ui('bare').bind('bare'));(event.register('save', ctx=>ctx.save(1)), ui('save').bind('save'));(event.register('auto', ctx=>ctx.autoPlay(true)), ui('auto').bind('auto'));(event.register('restart', ctx=>ctx.restart()), ui('restart').bind('restart'));",
        )?;
        let path = root.path().join("story.xml");
        let xml = std::fs::read_to_string(&path)?
            .replace("entry=\"a\"", "entry=\"a secret\"")
            .replace("<frame id=\"c\">", "<frame id=\"c\" name=\"secret\">");
        std::fs::write(&path, xml)?;
        let mut app = load_ui(root.path(), saves.path())?;
        assert!(!app.effects.has_entered);
        assert_eq!(app.custom.state.variables["trace"].to_string(), "");
        assert_eq!(app.custom.data.values["game"]["frameId"], "");
        assert!(app.pending_auto.is_none());
        app.typewriter.finish();
        app.mark_current_read();
        assert!(!app.read.contains("a"));
        assert!(
            app.handle(UiIntent::Play(controller::Intent::Advance))
                .is_err()
        );
        for name in ["bare", "advance", "save", "auto", "restart"] {
            assert!(
                app.custom_event(interface::Event::ui(name, name)).is_err(),
                "{name}"
            );
        }
        assert!(
            app.resume_handoff(app.empty_ui_plan()?, Some("b".into()))
                .is_err()
        );
        app.rebuild_cpu_presentation()?;
        assert!(!app.effects.has_entered);
        // A stale inactive slot must not supply another route's variables to a fresh start.
        std::fs::write(saves.path().join("flow-checkpoint.json"), "stale")?;
        app.custom_event(interface::Event::ui("start", "start"))?;
        assert!(app.effects.has_entered);
        assert_eq!(app.session.frame_id, "c");
        assert_eq!(app.session.history.len(), 1);
        assert_eq!(app.session.history[0].frame_id, "c");
        assert_eq!(app.session.variables, app.story.initial_variables());
        assert_eq!(app.custom.state.variables["trace"].to_string(), "C");
        app.story
            .validate_session_with(&app.session, &app.logic)
            .map_err(anyhow::Error::msg)?;
        Ok(())
    }

    #[test]
    fn multiple_entry_validation_rejects_duplicates_unknown_and_checks_each_guard() -> Result<()> {
        let root = tempfile::tempdir()?;
        write_game(root.path(), "")?;
        let path = root.path().join("story.xml");
        let original = std::fs::read_to_string(&path)?
            .replace("<frame id=\"c\">", "<frame id=\"c\" name=\"secret\">");
        for entries in ["a a", "c secret", "a missing", " "] {
            std::fs::write(
                &path,
                original.replace("entry=\"a\"", &format!("entry=\"{entries}\"")),
            )?;
            let error =
                crate::loader::load_game(&crate::content::DirectorySource::new(root.path())?)
                    .err()
                    .expect("invalid entries");
            assert!(format!("{error:#}").contains("story.xml:"));
        }
        std::fs::write(&path, original.replace("entry=\"a\"", "entry=\"a secret\""))?;
        let script = std::fs::read_to_string(root.path().join("story.js"))?.replace(
            "frame('c').end();",
            "frame('c').end();ui('$game').bind('game.enter', ctx=>{if(ctx.game.frameId==='secret')throw Error('second entry guard');});",
        );
        std::fs::write(root.path().join("story.js"), script)?;
        assert!(
            crate::loader::load_game(&crate::content::DirectorySource::new(root.path())?).is_err()
        );
        Ok(())
    }

    #[test]
    fn first_entry_load_and_restart_keep_routes_separate_and_fail_atomically() -> Result<()> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(
            root.path(),
            "frame('c').enter(ctx=>ctx.open('movie'));(event.register('load', ctx=>ctx.load(1)), ui('load').bind('load'));(event.register('restart-c', ctx=>ctx.restart('c')), ui('restart-c').bind('restart-c'));",
        )?;
        let path = root.path().join("story.xml");
        std::fs::write(
            &path,
            std::fs::read_to_string(&path)?.replace("entry=\"a\"", "entry=\"a c\""),
        )?;
        let mut app = load_ui(root.path(), saves.path())?;
        let before = app.session.clone();
        std::fs::create_dir(saves.path().join("flow-checkpoint.json"))?;
        assert!(
            app.resume_handoff(app.empty_ui_plan()?, Some("c".into()))
                .is_err()
        );
        assert_eq!(app.session, before);
        assert!(!app.effects.has_entered);
        assert_eq!(app.custom.state.stack, ["hud"]);
        std::fs::remove_dir(saves.path().join("flow-checkpoint.json"))?;
        // Load a non-entry position using its validated history, without entering placeholder a.
        let session = controller::transition_with(
            &app.story,
            &app.story.start_at("a").map_err(anyhow::Error::msg)?,
            controller::Intent::Advance,
            &app.logic,
        )?
        .session;
        let snapshot = Snapshot::capture_with(&app.story, &session, &app.logic)?;
        app.saves.write_snapshot(
            &app.saves.slot_path(crate::save::SlotId::new(1)?),
            &snapshot,
        )?;
        app.custom_event(interface::Event::ui("load", "load"))?;
        assert_eq!(app.session, session);
        assert_eq!(app.custom.state.variables["trace"].to_string(), "");
        assert!(app.pending_auto.is_none());
        app.custom_event(interface::Event::ui("restart-c", "restart-c"))?;
        assert_eq!(app.session.frame_id, "c");
        assert_eq!(app.session.history.len(), 1);
        assert_eq!(app.session.variables, app.story.initial_variables());
        Ok(())
    }

    #[test]
    fn cg_assets_have_shared_unlocks_counts_and_background_only_frames() -> Result<()> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        write_game(
            root.path(),
            "ui('$game').bind('game.enter', ctx=>{if(ctx.game.cgId)ctx.unlock(ctx.game.cgId);});",
        )?;
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/bg/room.png"),
            root.path().join("cg.png"),
        )?;
        let path = root.path().join("story.xml");
        let xml = std::fs::read_to_string(&path)?
            .replace("<assets>", "<assets><cg id=\"cg.test\" src=\"cg.png\"/>")
            .replace(
                "<visuals/>",
                "<visuals><visual id=\"full\" cg=\"cg.test\"/></visuals>",
            )
            .replace("<frame id=\"b\">", "<frame id=\"b\" visual=\"full\">");
        std::fs::write(&path, &xml)?;
        let mut app = load(root.path(), saves.path())?;
        let initial = app.session.clone();
        assert_eq!(app.custom.definition.gallery.len(), 1);
        advance(&mut app)?;
        assert!(app.custom.profile.unlocked.contains("cg.test"));
        assert!(app.story.frames["b"].visual.portraits.is_empty());
        let bytes = std::fs::read(saves.path().join("profile.json"))?;
        app.enter(initial, true, true)?;
        assert!(app.custom.profile.unlocked.contains("cg.test"));
        drop(app);
        let app = load(root.path(), saves.path())?;
        assert_eq!(app.custom.profile.unlocked.len(), 1);
        assert_eq!(std::fs::read(saves.path().join("profile.json"))?, bytes);
        let data = interface::Data::build(interface::data::Input {
            definition: &app.custom.definition,
            story: &app.story,
            session: &app.session,
            available: &app.available,
            state: &app.custom.state,
            profile: &app.custom.profile,
            slots: &[],
            settings: &app.settings,
            complete: true,
        });
        assert_eq!(data.values["cg"]["total"], 1);
        assert_eq!(data.values["cg"]["unlocked"], 1);
        for bad in [
            xml.replace("cg=\"cg.test\"", "cg=\"font\""),
            xml.replace("cg=\"cg.test\"", "cg=\"cg.test\" background=\"cg.test\""),
        ] {
            std::fs::write(&path, bad)?;
            assert!(load(root.path(), saves.path()).is_err());
        }
        Ok(())
    }
}
