use crate::{
    assets::{AssetCache, PreparedAssets},
    audio::{AudioEngine, PreparedAudio},
    controller,
    loader::LoadedStory,
    menus::{self, MenuAction, SlotMode},
    model::{Session, Story},
    presentation::{self, GatedIntent, PresentationClock, Typewriter},
    save::{SaveStore, SlotId, SlotInfo},
    settings::Settings,
    view::{self, RenderState, UiIntent},
};
use eframe::egui;
use std::time::{Duration, Instant};

enum Overlay {
    History,
    Slots {
        mode: SlotMode,
        infos: Vec<SlotInfo>,
        pending: Option<SlotId>,
    },
    Settings,
}

pub struct PlayerApp {
    story: Story,
    session: Session,
    assets: AssetCache,
    saves: SaveStore,
    typewriter: Typewriter,
    clock: PresentationClock,
    audio: AudioEngine,
    settings: Settings,
    overlay: Option<Overlay>,
    message: Option<String>,
}

impl PlayerApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        loaded: LoadedStory,
        prepared: PreparedAssets,
        prepared_audio: PreparedAudio,
        saves: SaveStore,
        no_audio: bool,
    ) -> Self {
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        let assets = prepared.install(&cc.egui_ctx);
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
        let (mut audio, warning) = AudioEngine::new(prepared_audio, no_audio);
        if let Some(warning) = warning {
            warnings.push(warning);
        }
        let frame = loaded.story.frame(&session).expect("validated entry");
        audio.enter(&frame.audio, true);
        let typewriter = Typewriter::new(&frame.text, settings.text_speed == 0.0);
        let message = if warnings.is_empty() {
            None
        } else {
            Some(warnings.join("\n"))
        };
        Self {
            story: loaded.story,
            session,
            assets,
            saves,
            typewriter,
            clock: PresentationClock::new(Instant::now()),
            audio,
            settings,
            overlay: None,
            message,
        }
    }

    fn blocked(&self) -> bool {
        self.overlay.is_some() || self.message.is_some()
    }

    fn enter(&mut self, session: Session, restored: bool, force_audio: bool) {
        self.session = session;
        let frame = self.story.frame(&self.session).expect("validated session");
        self.typewriter = Typewriter::new(&frame.text, restored || self.settings.text_speed == 0.0);
        self.clock = PresentationClock::new(Instant::now());
        self.audio.enter(&frame.audio, force_audio);
    }

    fn handle(&mut self, intent: UiIntent) -> anyhow::Result<()> {
        match intent {
            UiIntent::Play(intent) => {
                match presentation::gate(intent, self.typewriter.complete()) {
                    GatedIntent::Reveal => self.typewriter.finish(),
                    GatedIntent::Ignore => {}
                    GatedIntent::Dispatch(intent) => {
                        let result = controller::transition(&self.story, &self.session, intent)?;
                        if result.entered {
                            self.enter(result.session, false, false);
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
                    infos: self.saves.list(&self.story),
                    pending: None,
                });
            }
            UiIntent::History => self.overlay = Some(Overlay::History),
            UiIntent::Settings => self.overlay = Some(Overlay::Settings),
            UiIntent::Restart => self.enter(self.story.start(), false, true),
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
                            let session = self.saves.load(slot, &self.story)?;
                            self.enter(session, true, true);
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
                let session = self.saves.import_legacy(&self.story)?;
                self.enter(session, true, true);
                self.overlay = None;
                self.message = Some("已导入 v0.1 存档。旧档不含历史，仅恢复当前位置；原文件未改写。请手动保存到新槽位。".into());
            }
        }
        Ok(())
    }

    fn write_slot(&mut self, slot: SlotId) -> anyhow::Result<()> {
        self.saves.save(slot, &self.story, &self.session)?;
        if let Some(Overlay::Slots { infos, pending, .. }) = &mut self.overlay {
            *infos = self.saves.list(&self.story);
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
        self.typewriter
            .tick(self.clock.elapsed(now, paused), self.settings.text_speed);
        let mut intent = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
            .show(ctx, |ui| {
                let frame = self.story.frame(&self.session).expect("validated session");
                intent = view::draw(
                    ui,
                    &self.story,
                    frame,
                    &self.assets,
                    RenderState {
                        visible_text: self.typewriter.visible_text(),
                        complete: self.typewriter.complete(),
                        blocked: self.blocked(),
                    },
                );
            });
        let result = if let Some(intent) = intent {
            ctx.request_repaint();
            self.handle(intent)
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
        if !blocked && focused && !self.typewriter.complete() {
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
