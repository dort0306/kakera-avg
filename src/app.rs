use crate::{
    assets::{AssetCache, PreparedAssets},
    audio::{AudioEngine, PreparedAudio},
    controller,
    loader::LoadedStory,
    model::{Logic, Session, Story},
    presentation::{self, GatedIntent, PresentationClock, Typewriter},
    save::SaveStore,
    script::ScriptLogic,
    settings::Settings,
};
use eframe::egui;
use std::time::{Duration, Instant};
mod capture;
mod colors;
mod custom;
mod debug;
mod development;
mod input;
mod lifecycle;
mod localization;
mod shortcuts;
mod skip;
mod ui_media;
use custom::CustomUi;
use debug::{DebugSession, PreparedEntry};

enum UiIntent {
    Custom(crate::interface::Event),
    Play(controller::Intent),
}

pub struct RuntimeAssets {
    pub images: PreparedAssets,
    pub audio: PreparedAudio,
    pub media: crate::media::PreparedMedia,
    pub live2d: crate::live2d::PreparedLive2D,
}
pub struct PlayerApp {
    quit_requested: bool,
    ui_media: ui_media::UiMedia,
    debug: Option<DebugSession>,
    capture: Option<crate::capture::Shared>,
    revision: u64,
    painted: (u64, bool),
    pending_auto: Option<(u64, crate::save::Snapshot)>,
    context: egui::Context,
    renderer: crate::render::Shared,
    cpu_assets: PreparedAssets,
    project: Option<crate::development::ProjectMonitor>,
    no_audio: bool,
    prepared_media: crate::media::PreparedMedia,
    prepared_live2d: crate::live2d::PreparedLive2D,
    handoff: Option<lifecycle::Checkpoint>,
    lifecycle_ready: bool,
    terminal_exit_done: bool,
    live2d: crate::live2d::Live2DPlayer,

    auto_play: presentation::AutoPlay,
    skip: presentation::FastForward,
    // Mapped fast-forward hold from the player's committed keybindings.
    fast_forward_held_last: bool,
    input_barrier: bool,
    held_keys: std::collections::BTreeSet<crate::shortcuts::Key>,
    shortcut_capture: Option<input::Capture>,
    shortcut_scope: Option<shortcuts::Scope>,
    pickers: crate::interface::render::PickerState,
    ui_generation: u64,
    effects: crate::effects::Player,
    story: Story,
    session: Session,
    logic: ScriptLogic,
    available: Vec<String>,
    assets: AssetCache,
    saves: SaveStore,
    read: crate::read_history::ReadHistory,
    read_flush_at: Instant,
    read_error_reported: bool,
    typewriter: Typewriter,
    clock: PresentationClock,
    audio: AudioEngine,
    settings: Settings,
    message: Option<String>,
    custom: CustomUi,
}

impl PlayerApp {
    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    pub fn window_title(&self) -> &str {
        &self.custom.state.window_title
    }
    pub fn fullscreen(&self) -> bool {
        self.settings.fullscreen
    }

    pub fn new_context(
        ctx: &egui::Context,
        mut loaded: LoadedStory,
        runtime: RuntimeAssets,
        saves: SaveStore,
        no_audio: bool,
        logic: ScriptLogic,
        renderer: crate::render::Shared,
    ) -> anyhow::Result<Self> {
        runtime.images.validate_interface(&mut loaded.interface)?;
        ctx.set_theme(egui::ThemePreference::Dark);
        let cpu_assets = runtime.images.clone();
        let assets = runtime.images.install_render(ctx, &renderer);
        let mut warnings = Vec::new();
        let settings = if saves.settings_path().exists() {
            Settings::load(&saves.settings_path()).unwrap_or_else(|error| {
                warnings.push(crate::i18n::format(
                    "host-app-0001",
                    &[("arg0", format!("{:#}", error))],
                ));
                Settings::default()
            })
        } else {
            Settings::default()
        };
        let session = loaded.story.start();
        let available = logic
            .options(&loaded.story, &session)
            .expect("validated initial options");
        let (audio, warning) = AudioEngine::new(runtime.audio, no_audio);
        if let Some(warning) = warning {
            warnings.push(warning);
        }
        let frame = loaded.story.frame(&session).expect("validated entry");
        let typewriter = Typewriter::new(&frame.text, settings.text_speed == 0.0);
        let custom = CustomUi::new(
            loaded.interface,
            &loaded.story,
            &saves,
            &logic,
            &mut warnings,
        );
        let keybindings = Self::load_shortcut_bindings(&saves, &loaded.story.id, &mut warnings);
        let mut custom = custom;
        custom.state.keybindings = keybindings;
        Self::load_colors(
            &saves,
            &loaded.story.id,
            &custom.definition,
            &mut custom.state,
            &mut warnings,
        );
        Self::load_languages(
            &saves,
            &loaded.story.id,
            &custom.definition,
            &mut custom.state,
            &mut warnings,
        )?;
        let read = Self::load_read_history(&saves, &loaded.story.id, &mut warnings);
        let prefs_path = saves.directory().join("ui-audio.json");
        let prefs = if prefs_path.exists() {
            crate::interface::presentation::Preferences::load(&prefs_path, &loaded.story.id)
                .unwrap_or_else(|e| {
                    warnings.push(crate::i18n::format(
                        "host-app-0002",
                        &[("arg0", format!("{:#}", e))],
                    ));
                    crate::interface::presentation::Preferences::new(&loaded.story.id)
                })
        } else {
            crate::interface::presentation::Preferences::new(&loaded.story.id)
        };
        let message = if warnings.is_empty() {
            None
        } else {
            Some(warnings.join("\n"))
        };
        let mut app = Self {
            quit_requested: false,
            ui_media: ui_media::UiMedia::new(prefs),
            debug: None,
            capture: None,
            revision: 0,
            painted: (0, false),
            pending_auto: None,
            context: ctx.clone(),
            renderer,
            cpu_assets,
            project: None,
            no_audio,
            prepared_media: runtime.media,
            prepared_live2d: runtime.live2d,
            handoff: None,
            lifecycle_ready: false,
            terminal_exit_done: false,
            live2d: Default::default(),
            auto_play: Default::default(),
            skip: Default::default(),
            fast_forward_held_last: false,
            input_barrier: false,
            held_keys: Default::default(),
            shortcut_capture: None,
            shortcut_scope: None,
            pickers: Default::default(),
            ui_generation: 0,
            effects: Default::default(),
            story: loaded.story,
            session,
            logic,
            available,
            assets,
            saves,
            read,
            read_flush_at: Instant::now(),
            read_error_reported: false,
            typewriter,
            clock: PresentationClock::new(Instant::now()),
            audio,
            settings,
            message,
            custom,
        };
        app.restore_handoff()?;
        if !app.effects.has_entered {
            app.commit_ui(app.empty_ui_plan()?, true);
        }
        if let Some(locale) = app
            .custom
            .state
            .languages
            .text
            .as_ref()
            .and_then(|id| app.custom.definition.localization.texts.get(id))
        {
            app.cpu_assets.set_font_order(ctx, &locale.fonts);
        }
        app.sync_save_previews();
        #[cfg(not(windows))]
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(app.window_title().to_owned()));
        Ok(app)
    }

    fn blocked(&self) -> bool {
        !self.effects.has_entered
            || self.handoff.is_some()
            || self.ui_media.active()
            || self.debug_paused()
            || self.message.is_some()
            || self.custom.state.paused(&self.custom.definition)
    }

    fn autosave_enabled(&self) -> bool {
        self.settings.autosave
    }

    fn enter(&mut self, session: Session, restored: bool, force_audio: bool) -> anyhow::Result<()> {
        self.enter_planned(session, restored, force_audio, None)
    }
    fn enter_planned(
        &mut self,
        session: Session,
        restored: bool,
        force_audio: bool,
        base: Option<crate::interface::host::Plan>,
    ) -> anyhow::Result<()> {
        // Evaluate before committing; a broken destination guard must leave the old state intact.
        let available = self
            .logic
            .options(&self.story, &session)
            .map_err(anyhow::Error::msg)?;
        let mut base = base.unwrap_or(self.empty_ui_plan()?);
        if matches!(
            base.operation,
            crate::interface::host::Operation::Advance
                | crate::interface::host::Operation::Choose(_)
                | crate::interface::host::Operation::Load(_)
                | crate::interface::host::Operation::LoadAuto(_)
                | crate::interface::host::Operation::Restart(_)
        ) {
            base.operation = crate::interface::host::Operation::None;
        }
        // UI navigation supplied by the initiating event is already prepared. Only a
        // lifecycle changing this candidate stack hands the story back to UI.
        let entry_stack = base.state.stack.clone();
        let page_origin = if !self.effects.has_entered && !self.lifecycle_ready {
            Vec::new()
        } else {
            entry_stack.clone()
        };
        if !restored && self.effects.has_entered && !self.terminal_exit_done {
            base =
                self.lifecycle_hook(base, &self.session, "frame.exit", &self.session.frame_id)?;
            anyhow::ensure!(
                entry_stack != base.state.stack
                    || !base
                        .ui_actions
                        .iter()
                        .any(|a| matches!(a, crate::interface::presentation::Action::Sequence(_))),
                "{}",
                crate::i18n::format("host-app-0003", &[])
            );
            if entry_stack != base.state.stack
                || matches!(base.operation, crate::interface::host::Operation::Present)
            {
                return self.begin_handoff(
                    &session,
                    self.page_lifecycle(base, &self.session, Some(entry_stack.clone()))?,
                    entry_stack,
                );
            }
        }
        let mut plan = self.prepare_ui_entry(&session, &available, restored, Some(base))?;
        if !restored {
            plan = self.lifecycle_hook(plan, &session, "frame.enter", &session.frame_id)?;
            if plan
                .ui_actions
                .iter()
                .any(|a| matches!(a, crate::interface::presentation::Action::Sequence(_)))
                || entry_stack != plan.state.stack
                || matches!(plan.operation, crate::interface::host::Operation::Present)
            {
                plan = self.page_lifecycle(plan, &session, Some(page_origin))?;
                return self.begin_enter_handoff(session, available, plan, entry_stack);
            }
        }
        plan = self.page_lifecycle(plan, &session, Some(page_origin))?;
        let candidate = &self.story.frames[&session.frame_id];
        let live2d = crate::live2d::Live2DPlayer::prepare_with(
            candidate,
            &self.prepared_live2d,
            self.renderer.clone(),
        )?;
        let prepared_ui = self.prepare_ui_media(&plan)?;
        let entry = PreparedEntry {
            ui_media: prepared_ui,
            handoff_commit: None,
            session,
            restored,
            force_audio,
            available,
            plan,
            live2d,
        };
        if let Some(entry) = self.debug_break(entry) {
            self.commit_entry(entry)?;
        }
        Ok(())
    }
    fn commit_entry(&mut self, entry: PreparedEntry) -> anyhow::Result<()> {
        let PreparedEntry {
            ui_media: prepared_ui,
            handoff_commit,
            session,
            restored,
            force_audio,
            available,
            plan,
            live2d,
        } = entry;
        if matches!(plan.operation, crate::interface::host::Operation::Profile) {
            plan.profile
                .save(&self.saves.directory().join("profile.json"))?;
        }
        let terminal_exit_done = handoff_commit.as_ref().is_some_and(|c| c.terminal_exit());
        if let Some(checkpoint) = handoff_commit {
            self.commit_handoff_file(checkpoint)?;
        }
        self.terminal_exit_done = terminal_exit_done;
        if self.ui_media.video() {
            self.audio.video(None);
        }
        self.ui_media.cancel();
        self.effects.ui_alpha.clear();
        let previous = if self.effects.has_entered {
            self.story.frame(&self.session).cloned()
        } else {
            None
        };
        let transition = plan
            .transition
            .unwrap_or(self.story.frames[&session.frame_id].transition);
        self.effects.enter_scene(
            previous,
            &self.story.frames[&session.frame_id],
            transition,
            restored,
        );
        for (slot, image) in &self.live2d.images {
            self.assets.update_render(
                &self.context,
                crate::live2d::old_texture_id(*slot),
                image.clone(),
            );
        }
        self.live2d = live2d;
        self.available = available;
        if restored || force_audio {
            self.auto_play.set(false);
        } else {
            self.auto_play.enter();
        }
        self.session = session;
        let frame = self.story.frame(&self.session).expect("validated session");
        self.typewriter = Typewriter::new(
            &self
                .custom
                .definition
                .localization
                .frame_text(frame, plan.state.languages.text.as_deref()),
            restored || self.settings.text_speed == 0.0,
        );
        self.clock = PresentationClock::new(Instant::now());
        self.audio.enter(
            &self
                .custom
                .definition
                .localization
                .cue(frame, &plan.state.languages),
            force_audio,
        );
        self.pickers = Default::default();
        self.commit_ui(plan, false);
        self.apply_ui_media(prepared_ui);
        if restored {
            self.effects.age = 6.0;
            self.effects.ui_age = 6.0;
            self.effects.animations.clear();
            self.effects.portrait_actions.clear();
            self.effects.departing.clear();
        }
        self.revision = self.revision.wrapping_add(1);
        self.pending_auto = None;
        if !restored && self.handoff.is_none() && self.autosave_enabled() {
            self.queue_autosave();
        }
        Ok(())
    }

    fn handle(&mut self, intent: UiIntent) -> anyhow::Result<()> {
        self.debug_changed();
        match intent {
            UiIntent::Custom(event) => self.custom_event(event)?,
            UiIntent::Play(intent) => {
                anyhow::ensure!(
                    self.effects.has_entered,
                    "{}",
                    crate::i18n::format("host-app-0004", &[])
                );
                if self.effects.active() {
                    if matches!(intent, controller::Intent::Advance) {
                        self.effects.finish();
                    }
                    return Ok(());
                }
                match presentation::gate(intent, self.typewriter.complete()) {
                    GatedIntent::Reveal => self.typewriter.finish(),
                    GatedIntent::Ignore => {}
                    GatedIntent::Dispatch(intent) => {
                        let result = controller::transition_with(
                            &self.story,
                            &self.session,
                            intent,
                            &self.logic,
                        )?;
                        if result.entered {
                            self.enter(result.session, false, false)?;
                        } else {
                            self.exit_terminal(None)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn overlays(&mut self, ctx: &egui::Context) -> anyhow::Result<()> {
        if let Some(message) = self.message.clone() {
            egui::Window::new(crate::i18n::text("host-app-0010"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_max_width(650.0);
                    ui.label(&message);
                    if ui.button(crate::i18n::text("host-app-0011")).clicked() {
                        self.message = None;
                    }
                });
            return Ok(());
        }
        Ok(())
    }
}

impl PlayerApp {
    pub fn update_ui(&mut self, ctx: &egui::Context) {
        self.finish_autosave();
        let debug_input = self.poll_debug();
        let debug_paused = self.debug_paused();
        let focused = ctx.input(|input| input.focused);
        let input_frame = self.collect_input(ctx);
        let capture_busy = self.pending_auto.is_some() || self.shortcut_capture_active();
        let reloaded = !capture_busy
            && !debug_input
            && !self.debug_paused()
            && !self.ui_media.active()
            && self.handoff.is_none()
            && self.poll_project(ctx);
        if !self.lifecycle_ready && self.pending_auto.is_none() && !self.debug_paused() {
            self.lifecycle_ready = true;
            let result = self
                .empty_ui_plan()
                .and_then(|p| self.page_lifecycle(p, &self.session, Some(vec![])))
                .and_then(|p| self.commit_page_plan(p));
            if let Err(e) = result {
                self.message = Some(crate::i18n::format(
                    "host-app-0005",
                    &[("arg0", format!("{:#}", e))],
                ));
            }
        }
        if let Some(error) = self.audio.poll_error() {
            self.message = Some(error);
        }
        // A completed capture delivers exactly one ordinary UI event before drawing.
        if !reloaded
            && let Some(event) = self.tick_shortcut_capture(&input_frame)
            && let Err(error) = self.custom_event(event)
        {
            self.message = Some(crate::i18n::format(
                "host-app-0006",
                &[("arg0", format!("{:#}", error))],
            ));
        }
        let capture_active = self.shortcut_capture_active();
        let ui_input = match self.tick_ui_media(
            ctx,
            focused,
            input_frame.skip_sequence_pressed && !capture_active,
        ) {
            Ok(value) => value,
            Err(error) => {
                if self.ui_media.video() {
                    self.audio.video(None);
                }
                self.ui_media.cancel();
                self.effects.ui_alpha.clear();
                self.message = Some(crate::i18n::format(
                    "host-app-0007",
                    &[("arg0", format!("{:#}", error))],
                ));
                true
            }
        };
        let paused = self.blocked() || ui_input || !focused || capture_active;
        let now = Instant::now();
        let elapsed = self.clock.elapsed(now, paused);
        // Keep the read-only skip decision at the frame boundary: text becoming
        // fully displayed below must not make an unread line skippable this tick.
        let may_fast_forward = self.may_fast_forward();
        let transitioning = self.effects.active();
        self.effects.tick(elapsed.as_secs_f32());
        if let Err(error) = self
            .live2d
            .tick(elapsed.as_secs_f32(), ctx, &mut self.assets)
        {
            self.live2d = Default::default();
            self.message = Some(crate::i18n::format(
                "host-app-0008",
                &[("arg0", format!("{:#}", error))],
            ));
        }
        if !transitioning {
            self.typewriter.tick(elapsed, self.settings.text_speed);
        }
        if self.effects.has_entered && !self.ui_media.active() {
            self.mark_current_read();
        }
        let mut intent = None;
        self.painted = (
            self.revision,
            !self.blocked() && self.custom.state.stack.len() == 1,
        );
        self.refresh_shortcut_data();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
            .show(ctx, |ui| {
                let frame = self.story.frame(&self.session).expect("validated session");
                {
                    let custom = &mut self.custom;
                    custom.data.values["audio"] = serde_json::json!({
                        "enabled": self.ui_media.prefs.enabled,
                        "actor": custom.definition.media.actors.iter()
                            .map(|a| (a.clone(), !self.ui_media.prefs.disabled_actors.contains(a)))
                            .collect::<std::collections::BTreeMap<_, _>>(),
                        "excluded": custom.definition.media.audio.iter()
                            .map(|a| (a.clone(), self.ui_media.prefs.excluded.contains(a)))
                            .collect::<std::collections::BTreeMap<_, _>>()
                    });
                    custom.data.progress(
                        self.typewriter.complete() && !self.effects.active(),
                        &self.story.routes[&frame.id],
                        self.available.is_empty(),
                    );
                    if !self.terminal_exit_done
                        && matches!(self.story.routes[&frame.id], crate::model::Route::End)
                        && custom
                            .definition
                            .lifecycle
                            .contains_key(&format!("frame.exit:{}", frame.id))
                    {
                        custom.data.values["game"]["canAdvance"] = serde_json::json!(true);
                    }
                    custom.data.values["game"]["transitioning"] =
                        serde_json::json!(self.effects.active());
                    custom.data.values["game"]["autoPlaying"] =
                        serde_json::json!(self.auto_play.enabled);
                    custom.data.values["game"]["autoText"] =
                        serde_json::json!(if self.auto_play.enabled {
                            crate::i18n::text("host-app-0012")
                        } else {
                            crate::i18n::text("host-app-0013")
                        });
                    custom.data.values["game"]["live2dEnabled"] =
                        serde_json::json!(self.live2d.active());
                    if self.effects.active() {
                        custom.data.values["game"]["continueText"] =
                            serde_json::json!(crate::i18n::text("host-app-0014"));
                    }
                    if !self.effects.has_entered {
                        custom.data.awaiting_start(&custom.definition);
                    }
                    intent = crate::interface::render::draw(
                        ui,
                        crate::interface::render::Input {
                            effects: &self.effects,
                            definition: &custom.definition,
                            state: &custom.state,
                            data: &custom.data,
                            frame,
                            assets: &self.assets,
                            visible_text: if self.effects.has_entered {
                                self.typewriter.visible_text()
                            } else {
                                ""
                            },
                            blocked: self.message.is_some()
                                || self.pending_auto.is_some()
                                || debug_paused
                                || debug_input
                                || ui_input
                                || self.ui_media.active(),
                            pickers: &mut self.pickers,
                        },
                    )
                    .map(UiIntent::Custom);
                }
                self.draw_ui_video(ui);
            });
        // Drawing determines this frame's editing ownership, even when clicking
        // a picker does not change its value and therefore emits no UI event.
        if self.pickers.take_release_request() {
            self.input_barrier = true;
        }
        let story_input = !paused
            && !capture_busy
            && !self.input_barrier
            && self.custom.state.keyboard(&self.custom.definition)
            && !ctx.wants_keyboard_input()
            && !self.pickers.owns_input()
            && ctx.memory(|memory| memory.focused().is_none());
        let skip_held = input_frame.fast_forward_held && story_input;
        let skip = self.skip.tick(elapsed.as_secs_f32(), skip_held) && may_fast_forward;
        if intent.is_none() && story_input && input_frame.advance_pressed {
            intent = Some(UiIntent::Play(controller::Intent::Advance));
        }
        // A click that produced no UI event still cancels capture without leaking to the story.
        if intent.is_none() && self.shortcut_capture_active() && input_frame.pointer_clicked {
            self.cancel_shortcut_capture();
        }
        let route = &self.story.routes[&self.session.frame_id];
        let advance_route = matches!(route, crate::model::Route::Advance);
        let auto_advance = self.auto_play.tick(
            elapsed.as_secs_f32(),
            story_input
                && !input_frame.fast_forward_held
                && !input_frame.fast_forward_released
                && self.typewriter.complete()
                && !self.effects.active()
                && self.audio.voice_finished(),
            advance_route,
            self.settings.auto_delay,
        );
        let result = if reloaded || debug_input || ui_input || self.debug_paused() {
            Ok(())
        } else if let Some(intent) = intent {
            if matches!(intent, UiIntent::Play(_)) {
                self.auto_play.set(false);
            }
            ctx.request_repaint();
            self.handle(intent)
        } else if skip || (auto_advance && !paused && !capture_busy) {
            // Ctrl fast-forward and auto-play share the single sanctioned advance per update.
            self.handle(UiIntent::Play(controller::Intent::Advance))
        } else {
            Ok(())
        };
        if let Err(error) = result.and_then(|()| self.overlays(ctx)) {
            self.message = Some(crate::i18n::diagnostics(&format!("{error:#}")));
        }
        let blocked = self.blocked();
        // A newly opened overlay also pauses the interval before the next repaint.
        self.clock.set_paused(blocked || !focused);
        self.audio
            .configure(&self.settings, blocked, focused && !self.debug_paused());
        if self.ui_media.video() {
            self.audio.configure_ui_video(
                &self.settings,
                !focused || self.message.is_some() || self.debug_paused(),
            );
        }
        self.audio.configure_ui(
            &self.settings,
            focused && self.message.is_none() && !self.debug_paused(),
            |asset| {
                self.custom
                    .definition
                    .media
                    .allowed(asset, &self.ui_media.prefs)
            },
        );
        self.debug_overlay(ctx);
        if !blocked
            && focused
            && (!self.typewriter.complete()
                || self.effects.repaint()
                || self.auto_play.enabled
                || skip_held
                || self.live2d.active())
        {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        if self.ui_media.active() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        self.maybe_flush_read(now);
        // Poll backend failures without relying on further user input.
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    pub fn shutdown(&mut self) {
        self.finish_autosave();
        self.flush_read();
        if let Err(error) = self.settings.save(&self.saves.settings_path()) {
            eprintln!(
                "{}",
                crate::i18n::format("host-app-0009", &[("arg0", format!("{:#}", error))])
            );
        }
    }
}

impl eframe::App for PlayerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let previous_title = self.window_title().to_owned();
        self.update_ui(ctx);
        if self.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if previous_title != self.window_title() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title().to_owned()));
        }
    }
    fn on_exit(&mut self) {
        self.shutdown();
    }
}
