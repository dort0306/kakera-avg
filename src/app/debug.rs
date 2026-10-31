use super::*;
use crate::{
    debug::{Command, Server},
    interface::host::Plan,
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(super) struct PreparedEntry {
    pub session: Session,
    pub restored: bool,
    pub force_audio: bool,
    pub available: Vec<String>,
    pub plan: Plan,
    pub ui_media: super::ui_media::Prepared,
    pub handoff_commit: Option<super::lifecycle::Checkpoint>,
    pub live2d: crate::live2d::Live2DPlayer,
}
pub(super) struct DebugSession {
    server: Server,
    paused: bool,
    epoch: u64,
    breakpoints: BTreeSet<String>,
    pub pending: Option<PreparedEntry>,
    reason: String,
    last_error: Option<String>,
}
fn variables(values: &crate::model::Variables) -> Value {
    Value::Object(
        values
            .iter()
            .map(|(name, value)| {
                let value = match value {
                    crate::model::VariableValue::Int(v) => {
                        json!({"type":"int","value":v.to_string()})
                    }
                    crate::model::VariableValue::Bool(v) => json!({"type":"bool","value":v}),
                    crate::model::VariableValue::String(v) => json!({"type":"string","value":v}),
                };
                (name.clone(), value)
            })
            .collect(),
    )
}
impl PlayerApp {
    pub fn attach_debug(&mut self, server: Server) {
        self.pending_auto = None;
        self.settings.autosave = false;
        self.auto_play.set(false);
        self.clock.set_paused(true);
        self.audio.configure(&self.settings, true, false);
        self.debug = Some(DebugSession {
            server,
            paused: true,
            epoch: 0,
            breakpoints: BTreeSet::new(),
            pending: None,
            reason: "attached".into(),
            last_error: None,
        });
    }
    pub(super) fn debug_paused(&self) -> bool {
        self.debug.as_ref().is_some_and(|d| d.paused)
    }
    pub(super) fn debug_changed(&mut self) {
        if let Some(debug) = &mut self.debug {
            debug.epoch = debug.epoch.wrapping_add(1);
        }
    }
    pub(super) fn debug_discard_candidate(&mut self) {
        if let Some(debug) = &mut self.debug
            && debug.pending.take().is_some()
        {
            debug.reason = "candidate_discarded".into();
            debug.last_error = Some("renderer rebuilt; repeat the story action".into());
            debug.epoch = debug.epoch.wrapping_add(1);
        }
    }
    pub(super) fn debug_break(&mut self, entry: PreparedEntry) -> Option<PreparedEntry> {
        if let Some(debug) = &mut self.debug
            && debug.breakpoints.contains(&entry.session.frame_id)
        {
            debug.paused = true;
            debug.reason = "breakpoint_before_commit".into();
            debug.pending = Some(entry);
            debug.epoch = debug.epoch.wrapping_add(1);
            self.clock.set_paused(true);
            self.audio.configure(&self.settings, true, false);
            return None;
        }
        Some(entry)
    }
    pub(super) fn debug_snapshot(&self) -> Value {
        let debug = self.debug.as_ref().expect("attached debug");
        json!({"protocol":"KakeraEditorDebug","version":1,"process_id":std::process::id(),
            "engine_version":env!("CARGO_PKG_VERSION"),"state_revision":debug.epoch,
            "presentation_revision":self.revision,"story_id":self.story.id,"content_version":self.story.content_version,
            "game_started":self.effects.has_entered,
            "frame_id":self.effects.has_entered.then_some(&self.session.frame_id),"frame_name":self.effects.has_entered.then_some(self.story.frames[&self.session.frame_id].reference()),
            "paused":debug.paused,"reason":debug.reason,"last_error":debug.last_error,
            "pending_frame":debug.pending.as_ref().map(|entry|&entry.session.frame_id),
            "pending_variables":debug.pending.as_ref().map(|entry|variables(&entry.session.variables)),
            "variables":variables(&self.session.variables),"ui_variables":variables(&self.custom.state.variables),
            "history_length":if self.effects.has_entered {self.session.history.len()} else {0},"available_options":if self.effects.has_entered {self.available.clone()} else {Vec::new()},
            "ui_stack":self.custom.state.stack,"window_title":self.window_title(),
            "text_complete":self.typewriter.complete(),"transitioning":self.effects.active(),
            "video_playing":self.ui_media.video(),"live2d_active":self.live2d.active(),
            "video_position":self.ui_video_position(),"transition_age":self.effects.age,
            "host_prompt":self.message.is_some(),"menu_paused":!self.custom.state.keyboard(&self.custom.definition),
            "auto_playing":self.auto_play.enabled,
            "autosave_pending":self.pending_auto.is_some(),"breakpoints":debug.breakpoints,
            "capabilities":["status","pause","resume","breakpoints","advance","choose","reveal","skip_transition"]})
    }
    pub(super) fn debug_command(&mut self, command: Command, expected: Option<u64>) -> Result<()> {
        let stepping = matches!(
            command,
            Command::Advance | Command::Choose { .. } | Command::Reveal | Command::SkipTransition
        );
        let debug = self.debug.as_ref().expect("attached debug");
        ensure!(
            expected == Some(debug.epoch),
            "state revision conflict; query status"
        );
        match command {
            Command::Status => unreachable!(),
            Command::Breakpoints { frames } => {
                ensure!(
                    frames.len() <= 1000,
                    "{}",
                    crate::i18n::format("host-app-debug-0001", &[])
                );
                let mut resolved = BTreeSet::new();
                for frame in frames {
                    ensure!(
                        frame.len() <= 256,
                        "{}",
                        crate::i18n::format("host-app-debug-0002", &[])
                    );
                    resolved.insert(
                        self.story
                            .resolve_frame(&frame)
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "{}",
                                    crate::i18n::format(
                                        "host-app-debug-0003",
                                        &[("arg0", frame.to_string())]
                                    )
                                )
                            })?
                            .to_owned(),
                    );
                }
                self.debug.as_mut().unwrap().breakpoints = resolved;
            }
            Command::Pause => {
                let debug = self.debug.as_mut().unwrap();
                debug.paused = true;
                debug.reason = "pause".into();
                self.auto_play.set(false);
            }
            Command::Resume => {
                let pending = self.debug.as_mut().unwrap().pending.take();
                if let Some(entry) = pending {
                    self.commit_entry(entry)?;
                }
                let debug = self.debug.as_mut().unwrap();
                debug.paused = false;
                debug.reason = "running".into();
            }
            command => {
                ensure!(debug.paused, "step requires paused state");
                ensure!(
                    self.effects.has_entered || debug.pending.is_some(),
                    "{}",
                    crate::i18n::format("host-app-debug-0004", &[])
                );
                ensure!(
                    self.message.is_none()
                        && !self.ui_media.active()
                        && self.handoff.is_none()
                        && self.custom.state.keyboard(&self.custom.definition)
                        && self.pending_auto.is_none(),
                    "{}",
                    crate::i18n::format("host-app-debug-0005", &[])
                );
                if self.debug.as_ref().unwrap().pending.is_some() {
                    ensure!(
                        matches!(command, Command::Advance),
                        "{}",
                        crate::i18n::format("host-app-debug-0006", &[])
                    );
                    let entry = self.debug.as_mut().unwrap().pending.take().unwrap();
                    self.commit_entry(entry)?;
                } else {
                    match command {
                        Command::Advance => {
                            self.handle(UiIntent::Play(controller::Intent::Advance))?
                        }
                        Command::Choose { option } => {
                            ensure!(
                                self.typewriter.complete()
                                    && !self.effects.active()
                                    && !self.ui_media.video(),
                                "{}",
                                crate::i18n::format("host-app-debug-0007", &[])
                            );
                            ensure!(
                                self.available.contains(&option),
                                "{}",
                                crate::i18n::format("host-app-debug-0008", &[])
                            );
                            self.handle(UiIntent::Play(controller::Intent::Choose(option)))?;
                        }
                        Command::Reveal => {
                            ensure!(
                                !self.ui_media.video(),
                                "{}",
                                crate::i18n::format("host-app-debug-0009", &[])
                            );
                            self.typewriter.finish();
                        }
                        Command::SkipTransition => self.effects.finish(),
                        _ => unreachable!(),
                    }
                }
            }
        }
        self.debug_changed();
        if let Some(debug) = &mut self.debug {
            debug.last_error = None;
            if stepping && debug.pending.is_none() {
                debug.reason = "step".into();
            }
        }
        self.clock = PresentationClock::new(Instant::now());
        self.clock.set_paused(self.debug_paused());
        self.audio
            .configure(&self.settings, self.blocked(), !self.debug_paused());
        Ok(())
    }
    /// Called at the App update boundary, including when the native client area is minimized.
    pub fn poll_debug(&mut self) -> bool {
        let Some(job) = self.debug.as_ref().and_then(|d| d.server.try_next()) else {
            return false;
        };
        let mutation = !matches!(job.command, Command::Status);
        let result = if Instant::now() > job.deadline {
            Err(anyhow::anyhow!("expired debug command"))
        } else if !mutation {
            Ok(())
        } else {
            self.debug_command(job.command.clone(), job.expected_revision)
        };
        let result = match result {
            Ok(()) => Ok(self.debug_snapshot()),
            Err(error) => {
                let message = crate::i18n::diagnostics(&format!("{error:#}"));
                if let Some(debug) = &mut self.debug {
                    debug.last_error = Some(message.clone());
                }
                Err(message)
            }
        };
        job.respond(result);
        mutation
    }
    pub(super) fn debug_overlay(&self, ctx: &egui::Context) {
        if let Some(debug) = &self.debug
            && debug.paused
        {
            egui::Window::new(crate::i18n::text("host-app-debug-0012"))
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(crate::i18n::format(
                        "host-app-debug-0010",
                        &[
                            ("arg0", debug.reason.to_string()),
                            ("arg1", debug.epoch.to_string()),
                        ],
                    ));
                    if let Some(entry) = &debug.pending {
                        ui.label(crate::i18n::format(
                            "host-app-debug-0011",
                            &[("arg0", entry.session.frame_id.to_string())],
                        ));
                    }
                });
        }
    }
}
