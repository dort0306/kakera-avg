use super::*;
use anyhow::{Context, Result};
impl PlayerApp {
    pub fn monitor_project(&mut self, monitor: crate::development::ProjectMonitor) {
        self.project = Some(monitor);
    }
    pub fn rebuild_cpu_presentation(&mut self) -> Result<()> {
        self.debug_discard_candidate();
        if self.ui_media.video() {
            self.audio.video(None);
        }
        self.ui_media.cancel();
        let rebuilt = self
            .cpu_assets
            .clone()
            .install_render(&self.context, &self.renderer);
        let previous = std::mem::replace(&mut self.assets, rebuilt);
        self.assets.restore_streams_from(&previous);
        if let Some(locale) = self
            .custom
            .state
            .languages
            .text
            .as_ref()
            .and_then(|id| self.custom.definition.localization.texts.get(id))
        {
            self.cpu_assets.set_font_order(&self.context, &locale.fonts);
        }
        if !self.effects.has_entered {
            self.revision = self.revision.wrapping_add(1);
            return Ok(());
        }
        self.live2d = crate::live2d::Live2DPlayer::prepare_with(
            &self.story.frames[&self.session.frame_id],
            &self.prepared_live2d,
            self.renderer.clone(),
        )?;
        self.effects = Default::default();
        let frame = &self.story.frames[&self.session.frame_id];
        self.effects
            .enter_scene(None, frame, frame.transition, true);
        self.revision = self.revision.wrapping_add(1);
        if let Some((revision, _)) = &mut self.pending_auto {
            *revision = self.revision;
        }
        Ok(())
    }
    pub(super) fn replace_content(
        &mut self,
        bundle: crate::runtime::Bundle,
        session: Session,
    ) -> Result<()> {
        self.renderer.borrow_mut().clear();
        let crate::runtime::Bundle {
            loaded,
            logic,
            assets: runtime,
        } = bundle;
        let available = logic
            .options(&loaded.story, &session)
            .map_err(anyhow::Error::msg)?;
        let frame = loaded
            .story
            .frame(&session)
            .context(crate::i18n::text("host-app-development-0009"))?;
        let live2d = crate::live2d::Live2DPlayer::prepare_with(
            frame,
            &runtime.live2d,
            self.renderer.clone(),
        )?;
        let mut warnings = Vec::new();
        let colors = self.custom.state.colors.clone();
        anyhow::ensure!(
            colors
                .overrides
                .keys()
                .all(|id| loaded.interface.palette.contains_key(id)),
            "{}",
            crate::i18n::format("host-app-development-0001", &[])
        );
        let custom = CustomUi::new(
            loaded.interface,
            &loaded.story,
            &self.saves,
            &logic,
            &mut warnings,
        );
        let mut custom = custom;
        let retained_ui =
            self.custom.definition.localization.enabled()
                && custom.definition.localization.enabled()
                && self.custom.state.stack.iter().all(|id| {
                    custom.definition.screens.get(id).is_some_and(|screen| {
                        screen.modal == self.custom.definition.screens[id].modal
                    })
                })
                && self.custom.state.variables.iter().all(|(id, value)| {
                    custom.definition.variables.get(id).is_some_and(|decl| {
                        crate::model::rules::validate_value(decl, value).is_ok()
                    })
                });
        if retained_ui {
            custom.state.stack = self.custom.state.stack.clone();
            custom.state.slot_page = self.custom.state.slot_page;
            custom
                .state
                .variables
                .extend(self.custom.state.variables.clone());
        }
        custom.state.keybindings = self.custom.state.keybindings;
        custom.state.colors = colors;
        if !self.custom.definition.localization.enabled()
            && custom.definition.localization.enabled()
        {
            Self::load_languages(
                &self.saves,
                &loaded.story.id,
                &custom.definition,
                &mut custom.state,
                &mut warnings,
            )?;
        } else {
            custom.state.languages = custom.definition.localization.selection(
                self.custom.state.languages.requests.clone(),
                self.custom.state.languages.system.clone(),
            );
            if custom.definition.localization.enabled()
                && (custom.state.languages.text_source == "default"
                    && custom.state.languages.requests.text.is_some()
                    || custom.state.languages.voice_source == "default"
                        && custom.state.languages.requests.voice.is_some())
            {
                warnings.push(crate::i18n::text("host-language-fallback").into());
            }
        }
        custom.state.refresh(&custom.definition)?;
        let cpu_assets = runtime.images.clone();
        let mut candidate = Self {
            quit_requested: false,
            ui_media: super::ui_media::UiMedia::new(self.ui_media.prefs.clone()),
            debug: None,
            capture: None,
            revision: self.revision.wrapping_add(1),
            painted: (self.revision, false),
            pending_auto: None,
            context: self.context.clone(),
            renderer: self.renderer.clone(),
            cpu_assets,
            project: None,
            no_audio: self.no_audio,
            prepared_media: runtime.media,
            prepared_live2d: runtime.live2d,
            handoff: None,
            terminal_exit_done: self.terminal_exit_done,
            lifecycle_ready: true,
            live2d,
            auto_play: Default::default(),
            skip: Default::default(),
            fast_forward_held_last: false,
            input_barrier: true,
            held_keys: Default::default(),
            shortcut_capture: None,
            shortcut_scope: None,
            pickers: Default::default(),
            ui_generation: self.ui_generation.wrapping_add(1),
            effects: Default::default(),
            story: loaded.story,
            session,
            logic,
            available,
            assets: self.assets.clone(),
            saves: SaveStore::at(self.saves.directory().to_owned()),
            read: self.read.clone(),
            read_flush_at: Instant::now(),
            read_error_reported: self.read_error_reported,
            typewriter: Typewriter::new("", true),
            clock: PresentationClock::new(Instant::now()),
            // Candidate preparation uses a disabled device; old audio is untouched until commit.
            audio: AudioEngine::new(PreparedAudio::default(), true).0,
            settings: self.settings.clone(),
            message: None,
            custom,
        };
        let started = self.effects.has_entered;
        candidate.lifecycle_ready = started;
        let mut plan = if started && !retained_ui {
            candidate.prepare_ui_entry(&candidate.session, &candidate.available, true, None)?
        } else {
            candidate.empty_ui_plan()?
        };
        {
            // Hot reload refreshes UI locals, never grants persistent unlocks or replays transient actions.
            plan.profile = candidate.custom.profile.clone();
            plan.operation = crate::interface::host::Operation::None;
            plan.animations.clear();
            plan.portraits.clear();
            plan.live2d.clear();
            plan.transition = None;
        }
        // No fallible content validation or persistence follows font/texture replacement.
        for text in plan.state.localized_text.values() {
            candidate.cpu_assets.validate_text(text)?;
        }
        candidate
            .cpu_assets
            .validate_text(&plan.state.window_title)?;
        candidate.assets = runtime
            .images
            .install_render(&candidate.context, &candidate.renderer);
        let voice_changed =
            candidate.custom.state.languages.voice != self.custom.state.languages.voice;
        std::mem::swap(&mut candidate.audio, &mut self.audio);
        candidate.audio.replace_prepared(runtime.audio);
        if voice_changed {
            candidate.audio.stop_voice();
        }
        if let Some(locale) = candidate
            .custom
            .state
            .languages
            .text
            .as_ref()
            .and_then(|id| candidate.custom.definition.localization.texts.get(id))
        {
            candidate
                .cpu_assets
                .set_font_order(&candidate.context, &locale.fonts);
        }
        let frame = &candidate.story.frames[&candidate.session.frame_id];
        if started {
            candidate
                .effects
                .enter_scene(None, frame, frame.transition, true);
            candidate.typewriter = Typewriter::new(
                &candidate
                    .custom
                    .definition
                    .localization
                    .frame_text(frame, candidate.custom.state.languages.text.as_deref()),
                true,
            );
        }
        {
            candidate.commit_ui(plan, true);
        }
        if !warnings.is_empty() {
            eprintln!(
                "{}",
                crate::i18n::format(
                    "host-app-development-0002",
                    &[("arg0", warnings.join("\n").to_string())]
                )
            );
        }
        candidate.ui_media.scope = candidate.custom.state.stack.last().cloned();
        candidate.project = self.project.take();
        candidate.debug = self.debug.take();
        candidate.debug_changed();
        *self = candidate;
        println!(
            "{}",
            crate::i18n::format(
                "host-app-development-0003",
                &[
                    ("arg0", self.session.frame_id.to_string()),
                    ("arg1", format!("{}", self.session.history.len())),
                    ("arg2", self.renderer.borrow().name().to_string())
                ]
            )
        );
        Ok(())
    }
    pub(super) fn poll_project(&mut self, ctx: &egui::Context) -> bool {
        let mut reloaded = false;
        if let Some(warning) = self.renderer.borrow_mut().warning.take() {
            eprintln!("{warning}");
            self.message = Some(warning);
        }
        let Some(mut monitor) = self.project.take() else {
            return false;
        };
        if let Some(result) = monitor.poll() {
            match result {
                Ok(bundle) => match crate::development::compatible_session(
                    &self.story,
                    &self.session,
                    &bundle.loaded.story,
                    &bundle.logic,
                ) {
                    Ok(session) => match self.replace_content(bundle, session) {
                        Ok(()) => {
                            reloaded = true;
                            monitor.error = None;
                            monitor.status = crate::i18n::text("host-app-development-0010").into();
                        }
                        Err(e) => {
                            monitor.error = Some(crate::i18n::format(
                                "host-app-development-0004",
                                &[("arg0", format!("{:#}", e))],
                            ));
                        }
                    },
                    Err(e) => {
                        monitor.error = Some(crate::i18n::format(
                            "host-app-development-0005",
                            &[("arg0", format!("{:#}", e))],
                        ));
                        monitor.pending = Some(bundle);
                    }
                },
                Err(e) => {
                    monitor.error = Some(crate::i18n::format(
                        "host-app-development-0006",
                        &[("arg0", format!("{:#}", e))],
                    ));
                }
            }
        }
        let mut force = false;
        let mut retry = false;
        egui::Window::new(if monitor.error.is_some() {
            crate::i18n::text("host-app-development-0011")
        } else {
            crate::i18n::text("host-app-development-0012")
        })
        .id(egui::Id::new("project-monitor"))
        .default_pos(egui::pos2(8.0, 88.0))
        .default_width(360.0)
        .default_open(false)
        .show(ctx, |ui| {
            ui.label(&monitor.status);
            ui.label(crate::i18n::format(
                "host-app-development-0007",
                &[("arg0", self.renderer.borrow().name().to_string())],
            ));
            if let Some(error) = &monitor.error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
            retry = ui
                .button(crate::i18n::text("host-app-development-0013"))
                .clicked();
            force = ui
                .add_enabled(
                    monitor.pending.is_some(),
                    egui::Button::new(crate::i18n::text("host-app-development-0014")),
                )
                .clicked();
        });
        if force && let Some(bundle) = monitor.pending.take() {
            if bundle.loaded.story.id != self.story.id {
                monitor.error = Some(crate::i18n::text("host-app-development-0015").into());
            } else {
                let session = bundle.loaded.story.start();
                match self.replace_content(bundle, session) {
                    Ok(()) => {
                        reloaded = true;
                        monitor.error = None;
                        monitor.status = crate::i18n::text("host-app-development-0016").into();
                    }
                    Err(e) => {
                        monitor.error = Some(crate::i18n::format(
                            "host-app-development-0008",
                            &[("arg0", format!("{:#}", e))],
                        ))
                    }
                }
            }
        }
        if monitor.error.is_some() {
            monitor.status = crate::i18n::text("host-app-development-0017").into();
        }
        if retry {
            monitor.request();
        }
        self.project = Some(monitor);
        ctx.request_repaint_after(Duration::from_millis(50));
        reloaded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        package::Snapshot,
        runtime::{Bundle, LoadOptions},
    };
    use std::collections::BTreeMap;

    #[test]
    fn hot_replacement_is_transactional_and_never_writes_story_or_profile_files() -> Result<()> {
        let game = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        std::fs::create_dir(game.path().join("assets"))?;
        std::fs::copy(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("games/demo/assets/fonts/chinese.otf"),
            game.path().join("assets/font.otf"),
        )?;
        let xml = r#"<story schema="8" id="reload-transaction" content-version="1" entry="opening"><scripts><script src="story.js"/></scripts><interface src="ui.xml"/><assets><font id="font" src="assets/font.otf"/></assets><characters/><visuals/><frames><frame id="opening"><text>入口</text></frame><frame id="question"><text>旧台词</text></frame></frames><next end="true"/></story>"#;
        std::fs::write(game.path().join("story.xml"), xml)?;
        std::fs::write(
            game.path().join("ui.xml"),
            r#"<ui schema="3" width="1280" height="720" initial="hud"><variables><string id="status" initial=""/></variables><screen id="hud" modal="false" pause="false"><scene id="scene" width="1280" height="720"/></screen></ui>"#,
        )?;
        let script = "frame('opening').advance(()=> 'question');frame('question').end();ui('$game').bind('game.enter',ctx=>{ctx.setUi('status','原状态');});";
        std::fs::write(game.path().join("story.js"), script)?;
        let options = LoadOptions {
            core: None,
            no_live2d: true,
        };
        let load = || Bundle::load(&Snapshot::directory(game.path())?, &options);
        let bundle = load()?;
        let ctx = egui::Context::default();
        let mut app = PlayerApp::new_context(
            &ctx,
            bundle.loaded,
            bundle.assets,
            SaveStore::at(saves.path().to_owned()),
            true,
            bundle.logic,
            crate::render::Renderer::cpu(),
        )?;
        let next = controller::transition_with(
            &app.story,
            &app.session,
            controller::Intent::Advance,
            &app.logic,
        )
        .map_err(anyhow::Error::msg)?
        .session;
        app.enter(next, false, false)?;
        let before = app.session.clone();
        let visible = app.typewriter.visible_text().to_owned();
        let files = || -> Result<BTreeMap<std::path::PathBuf, Vec<u8>>> {
            std::fs::read_dir(saves.path())?
                .map(|entry| {
                    let path = entry?.path();
                    Ok((path.clone(), std::fs::read(path)?))
                })
                .collect()
        };
        let disk = files()?;
        std::fs::write(
            game.path().join("story.xml"),
            xml.replace("旧台词", "即时新台词"),
        )?;
        std::fs::write(
            game.path().join("story.js"),
            script.replace(
                "ctx.setUi",
                "if(ctx.game.frameId==='question')ctx.live2d('center','missing');ctx.setUi",
            ),
        )?;
        let invalid = load()?;
        assert!(app.replace_content(invalid, before.clone()).is_err());
        assert_eq!(app.session, before);
        assert_eq!(app.story.frames["question"].text, "旧台词");
        assert_eq!(app.typewriter.visible_text(), visible);
        assert_eq!(files()?, disk);
        std::fs::write(
            game.path().join("story.js"),
            script.replace("原状态", "新状态"),
        )?;
        let valid = load()?;
        let retained = crate::development::compatible_session(
            &app.story,
            &app.session,
            &valid.loaded.story,
            &valid.logic,
        )?;
        app.replace_content(valid, retained)?;
        assert_eq!(app.session, before);
        assert_eq!(app.story.frames["question"].text, "即时新台词");
        assert!(app.typewriter.complete());
        assert!(app.effects.has_entered);
        assert!(!app.effects.active());
        assert!(app.effects.previous.is_none());
        app.rebuild_cpu_presentation()?;
        assert!(app.effects.has_entered);
        assert!(!app.effects.active());
        assert_eq!(app.custom.state.variables["status"].to_string(), "新状态");
        assert_eq!(files()?, disk);
        Ok(())
    }
}
