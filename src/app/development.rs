use super::*;
use anyhow::{Context, Result};
impl PlayerApp {
    pub fn monitor_project(&mut self, monitor: crate::development::ProjectMonitor) {
        self.project = Some(monitor);
    }
    pub fn rebuild_cpu_presentation(&mut self) -> Result<()> {
        self.debug_discard_candidate();
        let rebuilt = self
            .cpu_assets
            .clone()
            .install_render(&self.context, &self.renderer);
        let previous = std::mem::replace(&mut self.assets, rebuilt);
        self.assets.restore_streams_from(&previous);
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
    fn replace_content(&mut self, bundle: crate::runtime::Bundle, session: Session) -> Result<()> {
        self.renderer.borrow_mut().clear();
        let crate::runtime::Bundle {
            loaded,
            logic,
            assets: runtime,
        } = bundle;
        let available = logic
            .options(&loaded.story, &session)
            .map_err(anyhow::Error::msg)?;
        let frame = loaded.story.frame(&session).context("热更新场景")?;
        let video = crate::media::VideoPlayer::prepare(frame.video.as_ref(), &runtime.media)?;
        let live2d = crate::live2d::Live2DPlayer::prepare_with(
            frame,
            &runtime.live2d,
            self.renderer.clone(),
        )?;
        let mut warnings = Vec::new();
        let custom = CustomUi::new(
            loaded.interface,
            &loaded.story,
            &self.saves,
            &logic,
            &mut warnings,
        );
        let cpu_assets = runtime.images.clone();
        let mut candidate = Self {
            debug: None,
            capture: self.capture.clone(),
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
            video,
            live2d,
            auto_play: Default::default(),
            skip: Default::default(),
            ctrl_held: self.ctrl_held,
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
        let mut plan =
            candidate.prepare_ui_entry(&candidate.session, &candidate.available, true, None)?;
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
        candidate.assets = runtime
            .images
            .install_render(&candidate.context, &candidate.renderer);
        let (audio, warning) = AudioEngine::new(runtime.audio, candidate.no_audio);
        candidate.audio = audio;
        if let Some(warning) = warning {
            warnings.push(warning);
        }
        let frame = &candidate.story.frames[&candidate.session.frame_id];
        candidate
            .effects
            .enter_scene(None, frame, frame.transition, true);
        candidate.typewriter = Typewriter::new(&frame.text, true);
        candidate.audio.enter(&frame.audio, true);
        candidate
            .audio
            .video(frame.video.as_ref().map(|v| v.asset.as_str()));
        {
            candidate.commit_ui(plan, true);
        }
        if !warnings.is_empty() {
            eprintln!("工程更新：{}", warnings.join("\n"));
        }
        candidate.project = self.project.take();
        candidate.debug = self.debug.take();
        candidate.debug_changed();
        *self = candidate;
        println!(
            "工程热更新成功：场景 {}，历史 {} 条，后端 {}",
            self.session.frame_id,
            self.session.history.len(),
            self.renderer.borrow().name()
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
                            monitor.status = "热更新成功，已保留当前位置与历史".into();
                        }
                        Err(e) => {
                            monitor.error = Some(format!("候选进入失败，保留旧状态：{e:#}"));
                        }
                    },
                    Err(e) => {
                        monitor.error = Some(format!("当前剧情与修改不兼容：{e:#}"));
                        monitor.pending = Some(bundle);
                    }
                },
                Err(e) => {
                    monitor.error = Some(format!("资源/脚本更新失败，保留旧状态：{e:#}"));
                }
            }
        }
        let mut force = false;
        let mut retry = false;
        egui::Window::new(if monitor.error.is_some() {
            "工程状态 · 更新失败"
        } else {
            "工程状态"
        })
        .id(egui::Id::new("project-monitor"))
        .default_pos(egui::pos2(8.0, 88.0))
        .default_width(360.0)
        .default_open(false)
        .show(ctx, |ui| {
            ui.label(&monitor.status);
            ui.label(format!("像素后端：{}", self.renderer.borrow().name()));
            if let Some(error) = &monitor.error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
            retry = ui.button("重新验证资源").clicked();
            force = ui
                .add_enabled(
                    monitor.pending.is_some(),
                    egui::Button::new("从入口重新载入（丢弃当前进度）"),
                )
                .clicked();
        });
        if force && let Some(bundle) = monitor.pending.take() {
            if bundle.loaded.story.id != self.story.id {
                monitor.error = Some("story_id 已变化，请重启并使用独立存档目录".into());
            } else {
                let session = bundle.loaded.story.start();
                match self.replace_content(bundle, session) {
                    Ok(()) => {
                        reloaded = true;
                        monitor.error = None;
                        monitor.status = "已从修改后的入口重新载入".into();
                    }
                    Err(e) => monitor.error = Some(format!("重载失败：{e:#}")),
                }
            }
        }
        if monitor.error.is_some() {
            monitor.status = "更新失败，继续使用旧内容".into();
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
        let script = "on.advance('opening',()=> 'question');on.end('question');on.ui('game.enter',ctx=>{ctx.setUi('status','原状态');});";
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
