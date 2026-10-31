//! Shortcut capture and input ownership inside the App; persistence stays in custom.rs.
use super::*;
use crate::interface::{Event, host::CaptureRequest};
use crate::shortcuts::{Action, Keybindings, KeybindingsFile};
use serde_json::json;

#[derive(Clone)]
pub(super) struct Scope {
    generation: u64,
    page: String,
    frame: String,
    history: usize,
}

impl PlayerApp {
    pub(super) fn load_shortcut_bindings(
        saves: &crate::save::SaveStore,
        story_id: &str,
        warnings: &mut Vec<String>,
    ) -> Keybindings {
        let path = saves.directory().join("keybindings.json");
        if !path.exists() {
            return Keybindings::defaults();
        }
        let result = crate::save::read_json_bytes(&path).and_then(|bytes| {
            KeybindingsFile::load_bytes(&bytes, story_id).map_err(anyhow::Error::msg)
        });
        result.unwrap_or_else(|error| {
            warnings.push(crate::i18n::format(
                "host-app-shortcuts-0001",
                &[("arg0", format!("{:#}", error))],
            ));
            Keybindings::defaults()
        })
    }

    pub(super) fn collect_input(&mut self, ctx: &egui::Context) -> input::FrameInput {
        let mut frame = input::FrameInput::collect(
            ctx,
            &self.custom.state.keybindings,
            self.input_barrier,
            &mut self.held_keys,
        );
        frame.fast_forward_released = self.fast_forward_held_last && !frame.fast_forward_held_raw;
        self.fast_forward_held_last = frame.fast_forward_held_raw;
        if self.input_barrier && !frame.any_key_down {
            self.input_barrier = false;
        }
        if !ctx.input(|input| input.focused) {
            self.input_barrier = true;
            self.cancel_shortcut_capture();
            self.pickers = Default::default();
            // Focus loss can swallow release events. Drop stale physical state so the
            // release barrier can clear after the window comes back.
            ctx.input_mut(|input| {
                input.keys_down.clear();
                input.modifiers = egui::Modifiers::default();
            });
        }
        frame
    }

    pub(super) fn shortcut_capture_active(&self) -> bool {
        self.shortcut_capture.is_some()
    }

    pub(super) fn refresh_shortcut_data(&mut self) {
        let bindings = &self.custom.state.keybindings;
        let capture_active = self.shortcut_capture_active();
        let capture_text = self
            .shortcut_capture
            .as_ref()
            .map(|capture| capture.text())
            .unwrap_or("");
        self.custom.data.values["shortcuts"] = json!({
            "bindings": {
                "advance": bindings.advance.map(|binding| binding.canonical()),
                "fast_forward": bindings.fast_forward.map(|binding| binding.canonical()),
                "skip_sequence": bindings.skip_sequence.map(|binding| binding.canonical()),
            },
            "advanceLabel": bindings.display(Action::Advance),
            "fastForwardLabel": bindings.display(Action::FastForward),
            "skipSequenceLabel": bindings.display(Action::SkipSequence),
            "captureActive": capture_active,
            "captureText": capture_text,
        });
    }

    fn shortcut_scope(&self) -> Scope {
        Scope {
            generation: self.ui_generation,
            page: self.custom.state.stack.last().cloned().unwrap_or_default(),
            frame: self.session.frame_id.clone(),
            history: self.session.history.len(),
        }
    }

    pub(super) fn start_shortcut_capture(&mut self, request: CaptureRequest) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.shortcut_capture.is_none(),
            "{}",
            crate::i18n::format("host-app-shortcuts-0002", &[])
        );
        self.pickers = Default::default();
        let scope = self.shortcut_scope();
        self.shortcut_capture = Some(input::Capture::new(request.action, request.result_event));
        self.shortcut_scope = Some(scope);
        self.input_barrier = true;
        Ok(())
    }

    pub(super) fn cancel_shortcut_capture(&mut self) {
        if self.shortcut_capture.take().is_some() {
            self.input_barrier = true;
        }
        self.shortcut_scope = None;
    }

    /// Returns one ordinary UI event after a successful capture. Stale scopes are dropped
    /// without delivering a callback.
    pub(super) fn tick_shortcut_capture(&mut self, frame: &input::FrameInput) -> Option<Event> {
        let mut capture = self.shortcut_capture.take()?;
        let current = self.shortcut_scope();
        let valid = self.shortcut_scope.as_ref().is_some_and(|scope| {
            scope.generation == current.generation
                && scope.page == current.page
                && scope.frame == current.frame
                && scope.history == current.history
                && frame.focused
        });
        if !valid {
            self.shortcut_scope = None;
            self.input_barrier = true;
            return None;
        }
        match capture.tick(frame) {
            input::CaptureTick::Running => {
                self.shortcut_capture = Some(capture);
                None
            }
            input::CaptureTick::Cancelled => {
                self.shortcut_scope = None;
                self.input_barrier = true;
                None
            }
            input::CaptureTick::Complete(binding) => {
                self.shortcut_scope = None;
                self.input_barrier = true;
                let mut event = Event::host(capture.result_event.clone());
                event.item = json!({"action": capture.action.id()});
                event.value = json!(binding.canonical());
                Some(event)
            }
        }
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
    use std::fs;

    const STORY: &str = r#"<story schema="8" id="shortcut-test" content-version="1" entry="opening"><scripts><script src="story.js"/></scripts><interface src="ui.xml"/><assets><font id="font" src="font.otf"/></assets><characters/><visuals/><frames><frame id="opening"><text>开始。</text></frame><frame id="second"><text>第二句。</text></frame></frames><next end="true"/></story>"#;
    const UI: &str = r#"<ui schema="3" initial="hud">
  <variables><string id="draft" initial=""/></variables>
  <ui-media><sequence id="fade"><fade target="menuCaption" duration="1000"/></sequence></ui-media>
  <screen id="hud" modal="false" pause="false"><scene id="scene" width="1280" height="720"/></screen>
  <screen id="shortcuts" modal="true">
    <label id="captureHint" width="600" height="40" bind="shortcuts.captureText" visible="shortcuts.captureActive"/>
    <label id="draftLabel" width="600" height="40" bind="ui.draft"/>
    <button id="captureAdvance" width="120" height="40" text="捕获" event="capture-advance"/>
    <button id="cancelCapture" width="120" height="40" text="取消" event="cancel-capture"/>
    <button id="applyDraft" width="120" height="40" text="应用" event="apply-draft"/>
    <button id="apply-enter" width="10" height="10" text="键" event="apply-enter"/>
    <button id="apply-ff-tab" width="10" height="10" text="键" event="apply-ff-tab"/>
    <button id="apply-swap" width="10" height="10" text="键" event="apply-swap"/>
    <button id="apply-conflict" width="10" height="10" text="键" event="apply-conflict"/>
    <button id="reset-bindings" width="10" height="10" text="键" event="reset-bindings"/>
    <button id="apply-skip-enter" width="10" height="10" text="键" event="apply-skip-enter"/>
    <button id="apply-invalid" width="10" height="10" text="键" event="apply-invalid"/>
    <button id="apply-mixed" width="10" height="10" text="键" event="apply-mixed"/>
  </screen>
  <screen id="menu" modal="true" pause="true" sequence="fade"><label id="menuCaption" width="600" height="60" text="演出"/></screen>
</ui>"#;
    const SCRIPT: &str = "frame('opening').advance(()=> 'second');frame('second').end();(event.register('capture-advance', ctx=>ctx.captureShortcut('advance','captured')), ui('captureAdvance').bind('capture-advance'));event.register('captured', ctx=>ctx.setUi('draft',ctx.value));(event.register('cancel-capture', ctx=>ctx.cancelShortcutCapture()), ui('cancelCapture').bind('cancel-capture'));(event.register('apply-draft', ctx=>ctx.keybinding('advance',ctx.ui.draft.length===0?null:ctx.ui.draft)), ui('applyDraft').bind('apply-draft'));(event.register('apply-enter', ctx=>ctx.keybinding('advance','Enter')), ui('apply-enter').bind('apply-enter'));(event.register('apply-ff-tab', ctx=>ctx.keybinding('fast_forward','Tab')), ui('apply-ff-tab').bind('apply-ff-tab'));(event.register('apply-swap', ctx=>{ctx.keybinding('advance','Enter');ctx.keybinding('fast_forward','Tab');}), ui('apply-swap').bind('apply-swap'));(event.register('apply-conflict', ctx=>{ctx.keybinding('advance','Tab');ctx.keybinding('fast_forward','Tab');}), ui('apply-conflict').bind('apply-conflict'));(event.register('reset-bindings', ctx=>ctx.resetKeybindings()), ui('reset-bindings').bind('reset-bindings'));(event.register('apply-skip-enter', ctx=>ctx.keybinding('skip_sequence','Enter')), ui('apply-skip-enter').bind('apply-skip-enter'));(event.register('apply-invalid', ctx=>ctx.keybinding('advance','NotAKey')), ui('apply-invalid').bind('apply-invalid'));(event.register('apply-mixed', ctx=>{ctx.keybinding('advance','Enter');ctx.setting('muted',true);}), ui('apply-mixed').bind('apply-mixed'));";

    struct Harness {
        app: PlayerApp,
        ctx: egui::Context,
        game: tempfile::TempDir,
        saves: tempfile::TempDir,
    }
    fn open(game: &std::path::Path, saves: &std::path::Path) -> anyhow::Result<PlayerApp> {
        let bundle = Bundle::load(
            &Snapshot::directory(game)?,
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
            SaveStore::at(saves.to_owned()),
            true,
            bundle.logic,
            Renderer::cpu(),
        )?;
        app.settings.autosave = false;
        app.message = None;
        app.enter(app.story.start(), false, true)?;
        app.pending_auto = None;
        app.typewriter.finish();
        Ok(app)
    }
    impl Harness {
        fn new() -> anyhow::Result<Self> {
            let game = tempfile::tempdir()?;
            let saves = tempfile::tempdir()?;
            fs::write(game.path().join("story.xml"), STORY)?;
            fs::write(game.path().join("ui.xml"), UI)?;
            fs::write(game.path().join("story.js"), SCRIPT)?;
            fs::copy(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("games/demo/assets/fonts/chinese.otf"),
                game.path().join("font.otf"),
            )?;
            crate::settings::Settings {
                autosave: false,
                ..Default::default()
            }
            .save(&saves.path().join("settings.json"))?;
            let app = open(game.path(), saves.path())?;
            Ok(Self {
                app,
                ctx: egui::Context::default(),
                game,
                saves,
            })
        }
        fn reopen(&self) -> anyhow::Result<Self> {
            let app = open(self.game.path(), self.saves.path())?;
            Ok(Self {
                app,
                ctx: egui::Context::default(),
                game: tempfile::tempdir()?,
                saves: tempfile::tempdir()?,
            })
        }
        fn event(&mut self, name: &str) -> anyhow::Result<()> {
            let target = match name {
                "capture-advance" => "captureAdvance",
                "cancel-capture" => "cancelCapture",
                "apply-draft" => "applyDraft",
                other => other,
            };
            self.app.custom_event(Event::ui(target, name))
        }
        fn update(&mut self, events: Vec<egui::Event>, focused: bool, modifiers: egui::Modifiers) {
            let _ = self.ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 720.0),
                    )),
                    focused,
                    modifiers,
                    events,
                    ..Default::default()
                },
                |ctx| self.app.update_ui(ctx),
            );
        }
        fn settle(&mut self) {
            self.update(Vec::new(), true, egui::Modifiers::default());
        }
        fn press(&mut self, key: egui::Key) {
            self.update(
                vec![egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                }],
                true,
                egui::Modifiers::default(),
            );
        }
        fn release(&mut self, key: egui::Key) {
            self.update(
                vec![egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                }],
                true,
                egui::Modifiers::default(),
            );
        }
        fn tap(&mut self, key: egui::Key) {
            self.press(key);
            self.release(key);
        }
        fn open_shortcuts(&mut self) {
            self.app.custom.state.stack.push("shortcuts".into());
            self.settle();
        }
        fn start_capture(&mut self) -> anyhow::Result<()> {
            self.open_shortcuts();
            self.event("capture-advance")?;
            assert!(self.app.shortcut_capture_active());
            self.settle();
            Ok(())
        }
        fn file(&self) -> std::path::PathBuf {
            self.saves.path().join("keybindings.json")
        }
    }

    fn saved_bindings(h: &Harness) -> crate::shortcuts::Keybindings {
        let bytes = fs::read(h.file()).unwrap();
        crate::shortcuts::KeybindingsFile::load_bytes(&bytes, "shortcut-test").unwrap()
    }

    #[test]
    fn rebinding_advance_to_enter_and_restart_keeps_it() -> anyhow::Result<()> {
        let mut h = Harness::new()?;
        h.tap(egui::Key::Space);
        assert_eq!(h.app.session.frame_id, "second");
        let _ = h.reopen()?;

        let mut h = Harness::new()?;
        h.event("apply-enter")?;
        h.settle();
        assert_eq!(
            h.app.custom.state.keybindings.advance.unwrap().canonical(),
            "Enter"
        );
        h.tap(egui::Key::Space);
        assert_eq!(
            h.app.session.frame_id, "opening",
            "old Space binding is gone"
        );
        h.tap(egui::Key::Enter);
        assert_eq!(h.app.session.frame_id, "second");
        let reopened = h.reopen()?;
        assert_eq!(
            reopened
                .app
                .custom
                .state
                .keybindings
                .advance
                .unwrap()
                .canonical(),
            "Enter"
        );
        assert_eq!(saved_bindings(&h).advance.unwrap().canonical(), "Enter");
        Ok(())
    }

    #[test]
    fn capture_waits_for_release_and_cancel_paths_do_not_write() -> anyhow::Result<()> {
        let mut h = Harness::new()?;
        h.start_capture()?;
        assert!(h.app.shortcut_capture_active());
        h.press(egui::Key::Space);
        assert!(h.app.shortcut_capture_active(), "capture waits for release");
        assert_eq!(
            h.app.session.frame_id, "opening",
            "capture blocks story input"
        );
        h.release(egui::Key::Space);
        assert!(!h.app.shortcut_capture_active());
        assert_eq!(h.app.custom.state.variables["draft"].to_string(), "Space");
        assert_eq!(h.app.session.frame_id, "opening");
        assert!(
            !h.file().exists(),
            "capture alone must not write preferences"
        );

        h.app.custom.state.stack.pop();
        h.start_capture()?;
        h.press(egui::Key::Escape);
        h.release(egui::Key::Escape);
        assert!(!h.app.shortcut_capture_active());
        assert_eq!(h.app.custom.state.variables["draft"].to_string(), "Space");
        Ok(())
    }

    #[test]
    fn batch_conflicts_mixed_operations_and_io_failure_keep_old_mapping() -> anyhow::Result<()> {
        let mut h = Harness::new()?;
        h.event("apply-swap")?;
        h.settle();
        let swapped = h.app.custom.state.keybindings;
        assert_eq!(swapped.advance.unwrap().canonical(), "Enter");
        assert_eq!(swapped.fast_forward.unwrap().canonical(), "Tab");
        assert!(h.event("apply-conflict").is_err());
        assert_eq!(h.app.custom.state.keybindings, swapped);
        assert!(h.event("apply-mixed").is_err());
        assert_eq!(h.app.custom.state.keybindings, swapped);
        assert!(h.event("apply-invalid").is_err());
        assert_eq!(h.app.custom.state.keybindings, swapped);

        fs::remove_file(h.file())?;
        fs::create_dir(h.file())?;
        fs::write(h.file().join("marker"), b"old")?;
        assert!(h.event("apply-enter").is_err());
        assert_eq!(h.app.custom.state.keybindings, swapped);
        assert_eq!(fs::read(h.file().join("marker"))?, b"old");
        fs::remove_dir_all(h.file())?;
        h.event("apply-enter")?;
        assert_eq!(
            h.app.custom.state.keybindings.advance.unwrap().canonical(),
            "Enter"
        );
        Ok(())
    }

    #[test]
    fn reset_and_bad_files_use_defaults_without_overwriting() -> anyhow::Result<()> {
        let mut h = Harness::new()?;
        h.event("apply-enter")?;
        h.settle();
        h.event("reset-bindings")?;
        h.settle();
        assert_eq!(
            h.app.custom.state.keybindings,
            crate::shortcuts::Keybindings::defaults()
        );
        assert_eq!(
            saved_bindings(&h),
            crate::shortcuts::Keybindings::defaults()
        );

        let bad = b"{\"schema_version\":1,\"story_id\":\"shortcut-test\",\"bindings\":{\"advance\":\"Escape\"}}";
        fs::write(h.file(), bad)?;
        let reopened = h.reopen()?;
        assert_eq!(
            reopened.app.custom.state.keybindings,
            crate::shortcuts::Keybindings::defaults()
        );
        assert_eq!(fs::read(h.file())?, bad, "a bad file is preserved");
        Ok(())
    }

    #[test]
    fn fast_forward_uses_the_new_hold_binding_only() -> anyhow::Result<()> {
        let mut h = Harness::new()?;
        h.app.settings.skip_all = true;
        h.event("apply-ff-tab")?;
        h.settle();
        for _ in 0..6 {
            std::thread::sleep(std::time::Duration::from_millis(45));
            h.update(
                Vec::new(),
                true,
                egui::Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            );
        }
        assert_eq!(
            h.app.session.frame_id, "opening",
            "old Ctrl no longer fast-forwards"
        );
        h.update(Vec::new(), true, egui::Modifiers::default());
        h.press(egui::Key::Tab);
        for _ in 0..8 {
            std::thread::sleep(std::time::Duration::from_millis(45));
            h.update(Vec::new(), true, egui::Modifiers::default());
        }
        h.release(egui::Key::Tab);
        assert_ne!(h.app.session.frame_id, "opening");
        Ok(())
    }

    #[test]
    fn skip_sequence_uses_its_own_key_and_does_not_leak_to_story() -> anyhow::Result<()> {
        let mut h = Harness::new()?;
        h.event("apply-skip-enter")?;
        h.settle();
        h.app.custom.state.stack.push("menu".into());
        h.settle();
        assert!(h.app.ui_media.active());
        h.press(egui::Key::Space);
        h.release(egui::Key::Space);
        assert!(
            h.app.ui_media.active(),
            "old skip key must not end the sequence"
        );
        h.press(egui::Key::Enter);
        h.release(egui::Key::Enter);
        assert!(!h.app.ui_media.active(), "the new skip key ends it once");
        assert_eq!(h.app.session.frame_id, "opening");
        Ok(())
    }

    #[test]
    fn captures_cancel_on_page_change_and_focus_loss() -> anyhow::Result<()> {
        let mut h = Harness::new()?;
        h.start_capture()?;
        h.app.custom.state.stack.pop();
        h.settle();
        assert!(!h.app.shortcut_capture_active());

        h.start_capture()?;
        h.update(Vec::new(), false, egui::Modifiers::default());
        assert!(!h.app.shortcut_capture_active());
        Ok(())
    }

    #[test]
    fn focus_loss_and_rebinding_never_replay_a_held_key() -> anyhow::Result<()> {
        let mut h = Harness::new()?;
        h.app.typewriter.finish();
        // A press while unfocused does not advance; refocusing without a new press does not replay it.
        h.update(
            vec![egui::Event::Key {
                key: egui::Key::Space,
                physical_key: Some(egui::Key::Space),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            }],
            false,
            egui::Modifiers::default(),
        );
        assert_eq!(h.app.session.frame_id, "opening");
        h.update(Vec::new(), true, egui::Modifiers::default());
        h.release(egui::Key::Space);
        h.update(Vec::new(), true, egui::Modifiers::default());
        assert_eq!(h.app.session.frame_id, "opening");
        assert!(
            !h.app.input_barrier,
            "focus loss barrier must clear after release"
        );
        assert!(h.app.typewriter.complete());
        assert!(h.app.message.is_none());
        assert!(h.app.custom.state.keyboard(&h.app.custom.definition));
        h.tap(egui::Key::Space);
        assert_eq!(h.app.session.frame_id, "second");

        // Applying Enter while Enter is physically held must wait for release and a fresh press.
        let mut h = Harness::new()?;
        h.app.typewriter.finish();
        h.app.held_keys.insert(crate::shortcuts::Key::Enter);
        h.event("apply-enter")?;
        h.settle();
        for _ in 0..3 {
            h.update(Vec::new(), true, egui::Modifiers::default());
        }
        assert_eq!(h.app.session.frame_id, "opening");
        h.release(egui::Key::Enter);
        assert_eq!(h.app.session.frame_id, "opening");
        h.tap(egui::Key::Enter);
        assert_eq!(h.app.session.frame_id, "second");
        Ok(())
    }
}
