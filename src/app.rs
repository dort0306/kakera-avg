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
mod custom;
mod debug;
mod development;
mod media;
mod skip;
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
    video: crate::media::VideoPlayer,
    live2d: crate::live2d::Live2DPlayer,

    auto_play: presentation::AutoPlay,
    skip: presentation::FastForward,
    // Track raw Ctrl across pauses so its release interval never counts as auto wait.
    ctrl_held: bool,
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
                warnings.push(format!("设置无效，已使用默认值：{error:#}"));
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
        let read = Self::load_read_history(&saves, &loaded.story.id, &mut warnings);
        let message = if warnings.is_empty() {
            None
        } else {
            Some(warnings.join("\n"))
        };
        let mut app = Self {
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
            video: Default::default(),
            live2d: Default::default(),
            auto_play: Default::default(),
            skip: Default::default(),
            ctrl_held: false,
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
        app.enter(app.session.clone(), false, true)?;
        app.sync_save_previews();
        #[cfg(not(windows))]
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(app.window_title().to_owned()));
        Ok(app)
    }

    fn blocked(&self) -> bool {
        self.debug_paused()
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
        let plan = self.prepare_ui_entry(&session, &available, restored, base)?;
        let candidate = &self.story.frames[&session.frame_id];
        let video =
            crate::media::VideoPlayer::prepare(candidate.video.as_ref(), &self.prepared_media)?;
        let live2d = crate::live2d::Live2DPlayer::prepare_with(
            candidate,
            &self.prepared_live2d,
            self.renderer.clone(),
        )?;
        let entry = PreparedEntry {
            session,
            restored,
            force_audio,
            available,
            plan,
            video,
            live2d,
        };
        if let Some(entry) = self.debug_break(entry) {
            self.commit_entry(entry)?;
        }
        Ok(())
    }
    fn commit_entry(&mut self, entry: PreparedEntry) -> anyhow::Result<()> {
        let PreparedEntry {
            session,
            restored,
            force_audio,
            available,
            plan,
            video,
            live2d,
        } = entry;
        if matches!(plan.operation, crate::interface::host::Operation::Profile) {
            plan.profile
                .save(&self.saves.directory().join("profile.json"))?;
        }
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
        self.video = video;
        self.live2d = live2d;
        self.available = available;
        if restored || force_audio {
            self.auto_play.set(false);
        } else {
            self.auto_play.enter();
        }
        self.session = session;
        let frame = self.story.frame(&self.session).expect("validated session");
        self.typewriter = Typewriter::new(&frame.text, restored || self.settings.text_speed == 0.0);
        self.clock = PresentationClock::new(Instant::now());
        self.audio.enter(&frame.audio, force_audio);
        self.audio
            .video(frame.video.as_ref().map(|v| v.asset.as_str()));
        self.commit_ui(plan, false);
        if restored {
            self.effects.age = 6.0;
            self.effects.ui_age = 6.0;
            self.effects.animations.clear();
            self.effects.portrait_actions.clear();
            self.effects.departing.clear();
        }
        self.revision = self.revision.wrapping_add(1);
        self.pending_auto = None;
        if !restored && self.autosave_enabled() {
            self.queue_autosave();
        }
        Ok(())
    }

    fn handle(&mut self, intent: UiIntent) -> anyhow::Result<()> {
        self.debug_changed();
        match intent {
            UiIntent::Custom(event) => self.custom_event(event)?,
            UiIntent::Play(intent) => {
                if self.video.playing() {
                    if matches!(intent, controller::Intent::Advance) {
                        self.skip_video(None)?;
                    }
                    return Ok(());
                }
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
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn overlays(&mut self, ctx: &egui::Context) -> anyhow::Result<()> {
        if let Some(message) = self.message.clone() {
            egui::Window::new("提示")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_max_width(650.0);
                    ui.label(message);
                    if ui.button("关闭").clicked() {
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
        let capture_busy = self.pending_auto.is_some();
        let reloaded =
            !capture_busy && !debug_input && !self.debug_paused() && self.poll_project(ctx);
        let focused = ctx.input(|input| input.focused);
        if let Some(error) = self.audio.poll_error() {
            self.message = Some(error);
        }
        let paused = self.blocked() || !focused;
        let now = Instant::now();
        let elapsed = self.clock.elapsed(now, paused);
        // Holding Ctrl fast-forwards; pause, host prompts and videos suspend the hold.
        let ctrl = ctx.input(|input| input.modifiers.ctrl);
        let ctrl_released = std::mem::replace(&mut self.ctrl_held, ctrl) && !ctrl;
        let story_input =
            !paused && !capture_busy && self.custom.state.keyboard(&self.custom.definition);
        let skip_held = ctrl && story_input && !self.video.playing();
        let skip = self.skip.tick(elapsed.as_secs_f32(), skip_held) && self.may_fast_forward();
        let transitioning = self.effects.active();
        self.effects.tick(elapsed.as_secs_f32());
        let video_event = match self
            .video
            .tick(elapsed.as_secs_f32(), ctx, &mut self.assets)
        {
            Ok(ended) => ended,
            Err(error) => {
                self.message = Some(format!("视频播放失败：{error:#}"));
                false
            }
        };
        if let Err(error) = self
            .live2d
            .tick(elapsed.as_secs_f32(), ctx, &mut self.assets)
        {
            self.live2d = Default::default();
            self.message = Some(format!(
                "Live2D 更新失败，已冻结当前画面；可读档或重新开始：{error:#}"
            ));
        }
        if self.video.ended {
            self.audio.video(None);
        }
        if !transitioning && !self.video.playing() {
            self.typewriter.tick(elapsed, self.settings.text_speed);
        }
        self.mark_current_read();
        let mut intent = None;
        self.painted = (
            self.revision,
            !self.blocked() && self.custom.state.stack.len() == 1,
        );
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
            .show(ctx, |ui| {
                let frame = self.story.frame(&self.session).expect("validated session");
                {
                    let custom = &mut self.custom;
                    custom.data.progress(
                        self.typewriter.complete()
                            && !self.effects.active()
                            && !self.video.playing(),
                        &self.story.routes[&frame.id],
                        self.available.is_empty(),
                    );
                    custom.data.values["game"]["transitioning"] =
                        serde_json::json!(self.effects.active());
                    custom.data.values["game"]["autoPlaying"] =
                        serde_json::json!(self.auto_play.enabled);
                    custom.data.values["game"]["autoText"] =
                        serde_json::json!(if self.auto_play.enabled {
                            "停止自动"
                        } else {
                            "自动播放"
                        });
                    custom.data.values["game"]["live2dEnabled"] =
                        serde_json::json!(self.live2d.active());
                    custom.data.values["game"]["videoPlaying"] =
                        serde_json::json!(self.video.playing());
                    custom.data.values["game"]["videoSkippable"] = serde_json::json!(
                        self.video.playing()
                            && self.video.cue.as_ref().is_some_and(|v| v.skippable)
                    );
                    if self.video.playing() {
                        custom.data.values["game"]["canAdvance"] = serde_json::json!(false);
                    }
                    if self.effects.active() {
                        custom.data.values["game"]["continueText"] = serde_json::json!("跳过转场");
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
                            visible_text: self.typewriter.visible_text(),
                            blocked: self.message.is_some()
                                || capture_busy
                                || debug_paused
                                || debug_input,
                        },
                    )
                    .map(UiIntent::Custom);
                    if intent.is_none()
                        && !capture_busy
                        && self.message.is_none()
                        && !debug_paused
                        && !debug_input
                        && custom.state.keyboard(&custom.definition)
                        && ctx.input(|i| i.key_pressed(egui::Key::Space))
                    {
                        intent = Some(UiIntent::Play(controller::Intent::Advance));
                    }
                }
            });
        let route = &self.story.routes[&self.session.frame_id];
        let advance_route = matches!(route, crate::model::Route::Advance);
        let auto_advance = self.auto_play.tick(
            elapsed.as_secs_f32(),
            story_input
                && !ctrl
                && !ctrl_released
                && self.typewriter.complete()
                && !self.effects.active()
                && !self.video.playing()
                && self.audio.voice_finished(),
            advance_route,
            self.settings.auto_delay,
        );
        let result = if reloaded || debug_input || self.debug_paused() {
            Ok(())
        } else if let Some(intent) = intent {
            if matches!(intent, UiIntent::Play(_)) {
                self.auto_play.set(false);
            }
            ctx.request_repaint();
            self.handle(intent)
        } else if video_event && !paused && !capture_busy {
            self.video.pending_advance = false;
            self.advance_video()
        } else if skip || (auto_advance && !paused && !capture_busy) {
            // Ctrl fast-forward and auto-play share the single sanctioned advance per update.
            self.handle(UiIntent::Play(controller::Intent::Advance))
        } else {
            Ok(())
        };
        if let Err(error) = result.and_then(|()| self.overlays(ctx)) {
            self.message = Some(format!("{error:#}"));
        }
        let blocked = self.blocked();
        // A newly opened overlay also pauses the interval before the next repaint.
        self.clock.set_paused(blocked || !focused);
        self.audio
            .configure(&self.settings, blocked, focused && !self.debug_paused());
        self.debug_overlay(ctx);
        if !blocked
            && focused
            && (!self.typewriter.complete()
                || self.effects.repaint()
                || self.auto_play.enabled
                || skip_held
                || self.video.playing()
                || self.live2d.active())
        {
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
            eprintln!("保存设置失败：{error:#}");
        }
    }
}

impl eframe::App for PlayerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let previous_title = self.window_title().to_owned();
        self.update_ui(ctx);
        if previous_title != self.window_title() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title().to_owned()));
        }
    }
    fn on_exit(&mut self) {
        self.shutdown();
    }
}
