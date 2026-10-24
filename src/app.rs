use crate::{
    assets::{AssetCache, PreparedAssets},
    audio::{AudioEngine, PreparedAudio},
    controller,
    loader::LoadedStory,
    menus::{self, MenuAction, SlotMode},
    model::{Logic, Session, Story},
    presentation::{self, GatedIntent, PresentationClock, Typewriter},
    save::{SaveStore, SlotId, SlotInfo},
    script::LogicEngine,
    settings::Settings,
    view::{self, RenderState, UiIntent},
};
use eframe::egui;
use std::time::{Duration, Instant};
mod custom;
mod media;
use custom::CustomUi;

enum Overlay {
    History,
    Slots {
        mode: SlotMode,
        infos: Vec<SlotInfo>,
        pending: Option<SlotId>,
    },
    Settings,
}

pub struct RuntimeAssets {
    pub images: PreparedAssets,
    pub audio: PreparedAudio,
    pub media: crate::media::PreparedMedia,
    pub live2d: crate::live2d::PreparedLive2D,
}
pub struct PlayerApp {
    context: egui::Context,
    prepared_media: crate::media::PreparedMedia,
    prepared_live2d: crate::live2d::PreparedLive2D,
    video: crate::media::VideoPlayer,
    live2d: crate::live2d::Live2DPlayer,

    auto_play: presentation::AutoPlay,
    effects: crate::effects::Player,
    story: Story,
    session: Session,
    logic: LogicEngine,
    available: Vec<String>,
    assets: AssetCache,
    saves: SaveStore,
    typewriter: Typewriter,
    clock: PresentationClock,
    audio: AudioEngine,
    settings: Settings,
    overlay: Option<Overlay>,
    message: Option<String>,
    custom: Option<CustomUi>,
}

impl PlayerApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        loaded: LoadedStory,
        runtime: RuntimeAssets,
        saves: SaveStore,
        no_audio: bool,
        logic: LogicEngine,
    ) -> anyhow::Result<Self> {
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        let assets = runtime.images.install(&cc.egui_ctx);
        let mut warnings = loaded.warnings;
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
        let custom = loaded.interface.map(|definition| {
            CustomUi::new(definition, &loaded.story, &saves, &logic, &mut warnings)
        });
        let message = if warnings.is_empty() {
            None
        } else {
            Some(warnings.join("\n"))
        };
        let mut app = Self {
            context: cc.egui_ctx.clone(),
            prepared_media: runtime.media,
            prepared_live2d: runtime.live2d,
            video: Default::default(),
            live2d: Default::default(),
            auto_play: Default::default(),
            effects: Default::default(),
            story: loaded.story,
            session,
            logic,
            available,
            assets,
            saves,
            typewriter,
            clock: PresentationClock::new(Instant::now()),
            audio,
            settings,
            overlay: None,
            message,
            custom,
        };
        app.enter(app.session.clone(), false, true)?;
        Ok(app)
    }

    fn blocked(&self) -> bool {
        self.overlay.is_some()
            || self.message.is_some()
            || self
                .custom
                .as_ref()
                .is_some_and(|ui| ui.state.paused(&ui.definition))
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
        let live2d = crate::live2d::Live2DPlayer::prepare(candidate, &self.prepared_live2d)?;
        if let Some(plan) = &plan
            && matches!(plan.operation, crate::interface::host::Operation::Profile)
        {
            plan.profile
                .save(&self.saves.directory().join("profile.json"))?;
        }
        let previous = if self.effects.has_entered {
            self.story.frame(&self.session).cloned()
        } else {
            None
        };
        let transition = plan
            .as_ref()
            .and_then(|p| p.transition)
            .unwrap_or(self.story.frames[&session.frame_id].transition);
        self.effects.enter_scene(
            previous,
            &self.story.frames[&session.frame_id],
            transition,
            restored,
        );
        for (slot, image) in &self.live2d.images {
            self.assets.update_stream(
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
        if let Some(plan) = plan {
            self.commit_ui(plan, false);
        }
        if restored {
            self.effects.age = 6.0;
            self.effects.ui_age = 6.0;
            self.effects.animations.clear();
            self.effects.portrait_actions.clear();
            self.effects.departing.clear();
        }
        if !restored
            && self.settings.autosave
            && let Err(error) = self.saves.autosave_committed(&self.story, &self.session)
        {
            self.settings.autosave = false;
            self.message = Some(format!(
                "剧情已进入，但自动存档失败（已有文件保留）：{error:#}"
            ));
        }
        Ok(())
    }

    fn handle(&mut self, intent: UiIntent) -> anyhow::Result<()> {
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
            UiIntent::Save | UiIntent::Load => {
                let mode = if matches!(intent, UiIntent::Save) {
                    SlotMode::Save
                } else {
                    SlotMode::Load
                };
                self.overlay = Some(Overlay::Slots {
                    mode,
                    infos: self.saves.list_with(&self.story, &self.logic),
                    pending: None,
                });
            }
            UiIntent::History => self.overlay = Some(Overlay::History),
            UiIntent::Settings => self.overlay = Some(Overlay::Settings),
            UiIntent::Restart => self.enter(self.story.start(), false, true)?,
        }
        Ok(())
    }

    fn menu_action(&mut self, action: MenuAction) -> anyhow::Result<()> {
        match action {
            MenuAction::Close => self.overlay = None,
            MenuAction::Cancel => {
                if let Some(Overlay::Slots { pending, .. }) = &mut self.overlay {
                    *pending = None;
                }
            }
            MenuAction::Slot(slot) => {
                if let Some(Overlay::Slots { mode, pending, .. }) = &mut self.overlay {
                    match mode {
                        SlotMode::Save if self.saves.slot_path(slot).exists() => {
                            *pending = Some(slot)
                        }
                        SlotMode::Save => self.write_slot(slot)?,
                        SlotMode::Load => {
                            let session = self.saves.load_with(slot, &self.story, &self.logic)?;
                            self.enter(session, true, true)?;
                            self.overlay = None;
                            self.message = Some(format!("槽位 {} 读档成功", slot.number()));
                        }
                    }
                }
            }
            MenuAction::Confirm(slot) => {
                if matches!(&self.overlay, Some(Overlay::Slots { mode: SlotMode::Save, pending: Some(expected), .. }) if *expected == slot)
                {
                    self.write_slot(slot)?;
                }
            }
            MenuAction::ImportLegacy => {
                let session = self.saves.import_legacy_with(&self.story, &self.logic)?;
                self.enter(session, true, true)?;
                self.overlay = None;
                self.message = Some("已导入 v0.1 存档。旧档不含历史，仅恢复当前位置；原文件未改写。请手动保存到新槽位。".into());
            }
        }
        Ok(())
    }

    fn write_slot(&mut self, slot: SlotId) -> anyhow::Result<()> {
        self.saves
            .save_with(slot, &self.story, &self.session, &self.logic)?;
        if let Some(Overlay::Slots { infos, pending, .. }) = &mut self.overlay {
            *infos = self.saves.list_with(&self.story, &self.logic);
            *pending = None;
        }
        self.message = Some(format!(
            "槽位 {} 存档成功\n{}",
            slot.number(),
            self.saves.slot_path(slot).display()
        ));
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
        match &self.overlay {
            Some(Overlay::History) => {
                if !menus::history(ctx, &self.story, &self.session) {
                    self.overlay = None;
                }
            }
            Some(Overlay::Slots {
                mode,
                infos,
                pending,
            }) => {
                if let Some(action) = menus::slots(
                    ctx,
                    infos,
                    *mode,
                    *pending,
                    self.saves.legacy_path().is_file(),
                ) {
                    self.menu_action(action)?;
                }
            }
            Some(Overlay::Settings) => {
                let (edited, open) = menus::settings(ctx, &self.settings, &self.audio.status());
                self.settings = edited;
                if self.settings.text_speed == 0.0 {
                    self.typewriter.finish();
                }
                if !open {
                    self.overlay = None;
                    self.settings.save(&self.saves.settings_path())?;
                }
            }
            None => {}
        }
        Ok(())
    }
}

impl eframe::App for PlayerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let focused = ctx.input(|input| input.focused);
        if let Some(error) = self.audio.poll_error() {
            self.message = Some(error);
        }
        let paused = self.blocked() || !focused;
        let now = Instant::now();
        let elapsed = self.clock.elapsed(now, paused);
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
        let mut intent = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
            .show(ctx, |ui| {
                let frame = self.story.frame(&self.session).expect("validated session");
                if let Some(custom) = &mut self.custom {
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
                            blocked: self.message.is_some(),
                        },
                    )
                    .map(UiIntent::Custom);
                    if intent.is_none()
                        && self.message.is_none()
                        && custom.state.keyboard(&custom.definition)
                        && ctx.input(|i| i.key_pressed(egui::Key::Space))
                    {
                        intent = Some(UiIntent::Play(controller::Intent::Advance));
                    }
                    return;
                }
                let options: Vec<_> = frame
                    .options
                    .iter()
                    .filter(|option| self.available.contains(&option.id))
                    .collect();
                intent = view::draw(
                    ui,
                    &self.story,
                    frame,
                    &self.assets,
                    RenderState {
                        visible_text: self.typewriter.visible_text(),
                        complete: self.typewriter.complete(),
                        blocked: self.blocked(),
                        options: &options,
                    },
                );
            });
        let route = &self.story.routes[&self.session.frame_id];
        let advance_route = matches!(
            route,
            crate::model::Route::Next(_)
                | crate::model::Route::Branch { .. }
                | crate::model::Route::Script(crate::model::ScriptRoute::Advance)
        );
        let auto_advance = self.auto_play.tick(
            elapsed.as_secs_f32(),
            self.typewriter.complete()
                && !self.effects.active()
                && !self.video.playing()
                && self.audio.voice_finished(),
            advance_route,
            self.settings.auto_delay,
        );
        let result = if let Some(intent) = intent {
            if matches!(intent, UiIntent::Play(_)) {
                self.auto_play.set(false);
            }
            ctx.request_repaint();
            self.handle(intent)
        } else if video_event && !paused {
            self.video.pending_advance = false;
            self.advance_video()
        } else if auto_advance && !paused {
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
        self.audio.configure(&self.settings, blocked, focused);
        if !blocked
            && focused
            && (!self.typewriter.complete()
                || self.effects.repaint()
                || self.auto_play.enabled
                || self.video.playing()
                || self.live2d.active())
        {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        // Poll backend failures without relying on further user input.
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    fn on_exit(&mut self) {
        if let Err(error) = self.settings.save(&self.saves.settings_path()) {
            eprintln!("保存设置失败：{error:#}");
        }
    }
}
