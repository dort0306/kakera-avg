//! Ctrl fast-forward pacing, read marks and their auxiliary file.
use super::*;

/// Read marks are flushed at most this often; shutdown always flushes.
const READ_FLUSH_INTERVAL: Duration = Duration::from_secs(3);

impl PlayerApp {
    pub(super) fn load_read_history(
        saves: &SaveStore,
        story_id: &str,
        warnings: &mut Vec<String>,
    ) -> crate::read_history::ReadHistory {
        let path = saves.read_path();
        if !path.exists() {
            return crate::read_history::ReadHistory::new(story_id);
        }
        crate::read_history::ReadHistory::load(&path, story_id).unwrap_or_else(|error| {
            warnings.push(format!("已读进度无效，按未读处理：{error:#}"));
            crate::read_history::ReadHistory::new(story_id)
        })
    }

    /// A fully displayed sentence counts as read; videos hide the dialogue and never mark it.
    pub(super) fn mark_current_read(&mut self) {
        if !self.effects.has_entered || self.ui_media.video() || !self.typewriter.complete() {
            return;
        }
        let frame_id = self.session.frame_id.clone();
        self.read.mark(&frame_id);
    }

    pub(super) fn maybe_flush_read(&mut self, now: Instant) {
        if self.read.dirty()
            && now.saturating_duration_since(self.read_flush_at) >= READ_FLUSH_INTERVAL
        {
            self.flush_read();
        }
    }

    /// Writes read marks atomically. Failures keep them dirty for a later retry and never
    /// interrupt the story; only the first failure is logged.
    pub(super) fn flush_read(&mut self) {
        self.read_flush_at = Instant::now();
        if !self.read.dirty() {
            return;
        }
        if let Err(error) = self.read.save(&self.saves.read_path())
            && !self.read_error_reported
        {
            eprintln!("已读进度保存失败，将自动重试：{error:#}");
            self.read_error_reported = true;
        }
    }

    /// Only advance routes can be fast-forwarded; choices and endings halt the hold.
    pub(super) fn may_fast_forward(&self) -> bool {
        let route = &self.story.routes[&self.session.frame_id];
        matches!(route, crate::model::Route::Advance)
            && (self.settings.skip_all || self.read.contains(&self.session.frame_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        package::Snapshot,
        render::Renderer,
        runtime::{Bundle, LoadOptions},
    };
    use eframe::egui;
    use std::path::Path;

    const STORY: &str = r#"<story schema="8" id="skip-test" content-version="1" entry="opening"><interface src="ui.xml"/><scripts><script src="story.js"/></scripts>
  <assets><font id="font" src="font.otf"/></assets>
  <characters/>
  <visuals/>
  <frames>
    <frame id="opening"><text>第一句已经读过。</text></frame>
    <frame id="second"><text>第二句也已经读过。</text></frame>
    <frame id="third"><text>第三句还没有读过，应当正常逐字显示而不是被快进跳过，所以这里写得长一些。</text></frame>
    <frame id="question"><text>要选择吗？</text>
      <options><option id="accept">接受</option><option id="finish">结束</option></options>
    </frame>
    <frame id="accept"><text>接受。</text></frame>
    <frame id="finish"><text>结束。</text></frame>
  </frames>
<next end="true"/></story>"#;
    const SCRIPT: &str = "on.advance('opening',()=> 'second');on.advance('second',()=> 'third');on.advance('third',()=> 'question');on.choice('question','accept',()=> 'accept');on.choice('question','finish',()=> 'finish');on.end('accept');on.end('finish');";
    const UI: &str =
        r#"<ui schema="3" initial="hud"><screen id="hud" modal="false" pause="false"/></ui>"#;

    struct Harness {
        app: PlayerApp,
        ctx: egui::Context,
        saves: tempfile::TempDir,
        _game: tempfile::TempDir,
    }

    impl Harness {
        fn new(read_json: Option<&str>) -> anyhow::Result<Self> {
            let game = tempfile::tempdir()?;
            std::fs::write(game.path().join("story.xml"), STORY)?;
            std::fs::write(game.path().join("story.js"), SCRIPT)?;
            std::fs::write(game.path().join("ui.xml"), UI)?;
            std::fs::copy(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/fonts/chinese.otf"),
                game.path().join("font.otf"),
            )?;
            let saves = tempfile::tempdir()?;
            if let Some(text) = read_json {
                std::fs::write(saves.path().join("read.json"), text)?;
            }
            Settings {
                autosave: false,
                ..Settings::default()
            }
            .save(&saves.path().join("settings.json"))?;
            let bundle = Bundle::load(
                &Snapshot::directory(game.path())?,
                &LoadOptions {
                    core: None,
                    no_live2d: true,
                },
            )?;
            let ctx = egui::Context::default();
            let mut app = PlayerApp::new_context(
                &ctx,
                bundle.loaded,
                bundle.assets,
                SaveStore::at(saves.path().to_owned()),
                true,
                bundle.logic,
                Renderer::cpu(),
            )?;
            app.enter(app.story.start(), false, true)?;
            Ok(Self {
                app,
                ctx,
                saves,
                _game: game,
            })
        }

        fn update(&mut self, ctrl: bool) {
            if ctrl {
                std::thread::sleep(Duration::from_millis(50));
            }
            let Self { app, ctx, .. } = self;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 720.0),
                    )),
                    focused: true,
                    modifiers: egui::Modifiers {
                        ctrl,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                |ctx| app.update_ui(ctx),
            );
        }

        fn hold(&mut self, steps: usize) {
            for _ in 0..steps {
                self.update(true);
            }
        }

        fn update_after(&mut self, ctrl: bool, focused: bool, elapsed: f32) {
            self.app.clock =
                PresentationClock::new(Instant::now() - Duration::from_secs_f32(elapsed));
            self.app.clock.set_paused(false);
            let Self { app, ctx, .. } = self;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 720.0),
                    )),
                    focused,
                    modifiers: egui::Modifiers {
                        ctrl,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                |ctx| app.update_ui(ctx),
            );
        }

        fn open_unpaused_menu(&mut self) -> anyhow::Result<()> {
            self.app.settings.autosave = false;
            let definition = crate::interface::parse(
                r#"<ui schema="3" initial="hud"><screen id="hud" modal="false" pause="false"/><screen id="menu" modal="true" pause="false"/></ui>"#,
                "ui.xml",
                &self.app.story,
                &Default::default(),
            )?;
            self.app.custom = CustomUi::new(
                definition,
                &self.app.story,
                &self.app.saves,
                &self.app.logic,
                &mut Vec::new(),
            );
            self.app.enter(self.app.session.clone(), true, true)?;
            self.app.custom.state.stack.push("menu".into());
            Ok(())
        }
    }

    #[test]
    fn ctrl_skips_read_lines_and_halts_at_the_first_unread_one() -> anyhow::Result<()> {
        let mut h = Harness::new(Some(
            r#"{"schema_version":1,"story_id":"skip-test","frames":["opening","second"]}"#,
        ))?;
        assert_eq!(h.app.session.frame_id, "opening");
        assert!(h.app.read.contains("opening"));
        h.hold(7);
        assert_eq!(h.app.session.frame_id, "third");
        assert!(
            !h.app.typewriter.complete(),
            "an unread sentence must keep typing instead of being skipped"
        );
        for _ in 0..3 {
            h.update(false);
        }
        assert_eq!(
            h.app.session.frame_id, "third",
            "releasing Ctrl stops the fast-forward"
        );
        Ok(())
    }

    #[test]
    fn skip_all_passes_unread_lines_but_still_stops_at_choices() -> anyhow::Result<()> {
        let mut h = Harness::new(None)?;
        h.app.settings.skip_all = true;
        h.hold(16);
        assert_eq!(h.app.session.frame_id, "question");
        let options = h.app.available.clone();
        assert_eq!(options, vec!["accept".to_owned(), "finish".to_owned()]);
        h.hold(6);
        assert_eq!(
            h.app.session.frame_id, "question",
            "fast-forward must never pick a branch"
        );
        assert_eq!(h.app.available, options);
        Ok(())
    }

    #[test]
    fn read_marks_survive_shutdown_and_bad_files_fall_back_to_unread() -> anyhow::Result<()> {
        let mut h = Harness::new(None)?;
        h.app.settings.skip_all = true;
        h.hold(16);
        assert_eq!(h.app.session.frame_id, "question");
        h.app.shutdown();
        let read =
            crate::read_history::ReadHistory::load(&h.saves.path().join("read.json"), "skip-test")?;
        for frame in ["opening", "second", "third"] {
            assert!(read.contains(frame), "{frame} must be marked read");
        }
        drop(h);

        let mut broken = Harness::new(Some("not json"))?;
        assert!(!broken.app.read.contains("opening"));
        assert!(
            broken
                .app
                .message
                .as_deref()
                .is_some_and(|message| message.contains("已读进度无效")),
            "a corrupt read.json warns once and still starts"
        );
        broken.update(false);
        Ok(())
    }

    #[test]
    fn held_ctrl_suppresses_autoplay_when_an_unread_line_completes_during_a_long_frame()
    -> anyhow::Result<()> {
        let mut h = Harness::new(None)?;
        h.app.settings.text_speed = 80.0;
        h.app.settings.auto_delay = 0.2;
        h.app.auto_play.set(true);
        h.update_after(true, true, 0.25);
        assert!(h.app.typewriter.complete());
        assert!(h.app.read.contains("opening"));
        assert_eq!(h.app.session.frame_id, "opening");
        assert_eq!(h.app.session.history.len(), 1);
        assert!(h.app.auto_play.enabled);
        Ok(())
    }

    #[test]
    fn releasing_ctrl_requires_a_fresh_autoplay_delay() -> anyhow::Result<()> {
        let mut h = Harness::new(None)?;
        h.app.settings.text_speed = 80.0;
        h.app.settings.auto_delay = 0.2;
        h.app.auto_play.set(true);
        h.update_after(true, true, 0.15);
        assert!(h.app.typewriter.complete());
        assert_eq!(h.app.session.frame_id, "opening");
        // Even a long release frame belongs to the hold interval, not the new auto wait.
        h.update_after(false, true, 1.0);
        assert_eq!(h.app.session.frame_id, "opening");
        h.update_after(false, true, 0.1);
        assert_eq!(h.app.session.frame_id, "opening");
        h.update_after(false, true, 0.11);
        assert_eq!(h.app.session.frame_id, "second");
        assert_eq!(h.app.session.history.len(), 2);
        Ok(())
    }

    #[test]
    fn unpaused_modal_pages_block_ctrl_and_autoplay_but_resume_after_closing() -> anyhow::Result<()>
    {
        let mut h = Harness::new(None)?;
        h.app.settings.skip_all = true;
        h.open_unpaused_menu()?;
        h.app.auto_play.set(true);
        h.app.settings.auto_delay = 0.2;
        let custom = &h.app.custom;
        assert!(!custom.state.paused(&custom.definition));
        assert!(!custom.state.keyboard(&custom.definition));
        h.update_after(true, true, 0.25);
        h.update_after(false, true, 0.25);
        h.update_after(false, true, 0.25);
        assert_eq!(h.app.session.frame_id, "opening");
        assert_eq!(h.app.session.history.len(), 1);
        h.app.custom.state.stack.pop();
        h.update_after(true, true, 0.05);
        assert_eq!(h.app.session.frame_id, "second");
        assert_eq!(h.app.session.history.len(), 2);
        Ok(())
    }

    #[test]
    fn prompts_focus_loss_and_pending_capture_block_ctrl_and_reset_auto_wait() -> anyhow::Result<()>
    {
        let mut h = Harness::new(None)?;
        h.app.typewriter.finish();
        h.app.settings.skip_all = true;
        h.app.settings.auto_delay = 0.2;
        h.app.auto_play.set(true);
        h.update_after(false, true, 0.1);
        h.app.message = Some("暂停".into());
        h.update_after(true, true, 0.3);
        h.app.message = None;
        h.update_after(true, false, 0.3);
        let cache = std::rc::Rc::new(std::cell::RefCell::new(crate::capture::Cache::default()));
        h.app.attach_capture(cache);
        h.app.queue_autosave();
        h.update_after(true, true, 0.3);
        assert!(h.app.pending_auto.is_some());
        assert_eq!(h.app.session.frame_id, "opening");
        assert_eq!(h.app.session.history.len(), 1);
        h.app.pending_auto = None;
        h.update_after(false, true, 0.3);
        h.update_after(false, true, 0.1);
        assert_eq!(h.app.session.frame_id, "opening");
        h.update_after(false, true, 0.11);
        assert_eq!(h.app.session.frame_id, "second");
        assert_eq!(h.app.session.history.len(), 2);
        Ok(())
    }
}
