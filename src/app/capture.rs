use super::PlayerApp;
use crate::{
    save::{SlotId, Snapshot},
    save_preview::Screenshot,
};
use anyhow::{Context, Result};

impl PlayerApp {
    pub fn attach_capture(&mut self, cache: crate::capture::Shared) {
        self.capture = Some(cache);
    }
    pub fn painted_capture(&self) -> (u64, bool) {
        self.painted
    }
    fn screenshot(&self, revision: u64) -> Result<Option<Screenshot>> {
        let cache = self
            .capture
            .as_ref()
            .context(crate::i18n::text("host-app-capture-0004"))?;
        let frame = cache.borrow().frame(revision)?;
        frame
            .map(|frame| Screenshot::capture((frame.read)()?))
            .transpose()
    }
    pub(super) fn save_current(&self, slot: SlotId) -> Result<()> {
        anyhow::ensure!(
            self.effects.has_entered && self.handoff.is_none() && !self.ui_media.active(),
            "{}",
            crate::i18n::format("host-app-capture-0001", &[])
        );
        let image = self
            .screenshot(self.revision)?
            .context(crate::i18n::text("host-app-capture-0005"))?;
        let mut snapshot = Snapshot::capture_committed(&self.story, &self.session)?;
        snapshot.screenshot = Some(image);
        self.saves
            .write_snapshot(&self.saves.slot_path(slot), &snapshot)
    }
    pub(super) fn queue_autosave(&mut self) {
        if !self.effects.has_entered {
            return;
        }
        match Snapshot::capture_committed(&self.story, &self.session) {
            Ok(snapshot) => self.pending_auto = Some((self.revision, snapshot)),
            Err(error) => self.autosave_error(error),
        }
    }
    fn autosave_error(&mut self, error: anyhow::Error) {
        self.pending_auto = None;
        self.settings.autosave = false;
        let message =
            crate::i18n::format("host-app-capture-0002", &[("arg0", format!("{:#}", error))]);
        self.message = Some(
            self.message
                .take()
                .map(|old| format!("{old}\n{message}"))
                .unwrap_or(message),
        );
    }
    pub(super) fn finish_autosave(&mut self) {
        let Some((revision, _)) = self.pending_auto.as_ref() else {
            return;
        };
        match self.screenshot(*revision) {
            Ok(None) => {}
            Ok(Some(image)) => {
                let (_, mut snapshot) = self.pending_auto.take().expect("pending autosave");
                snapshot.screenshot = Some(image);
                if let Err(error) = self.saves.autosave_snapshot(snapshot) {
                    self.autosave_error(error);
                }
            }
            Err(error) => self.autosave_error(error),
        }
    }
    pub(super) fn sync_save_previews(&mut self) {
        self.assets.clear_save_previews();
        {
            let custom = &mut self.custom;
            for (id, image) in std::mem::take(&mut custom.preview_images) {
                match image.decode() {
                    Ok(image) => self.assets.update_stream(
                        &self.context,
                        &id,
                        eframe::egui::ColorImage::from_rgba_unmultiplied(
                            [image.width() as usize, image.height() as usize],
                            image.as_raw(),
                        ),
                    ),
                    Err(error) => {
                        self.message = Some(crate::i18n::format(
                            "host-app-capture-0003",
                            &[("arg0", format!("{:#}", error))],
                        ))
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        capture::{Cache, Frame},
        controller,
        interface::Event,
        package,
        runtime::{Bundle, LoadOptions},
        save::SaveStore,
    };
    use eframe::egui::{self, Color32, ColorImage};
    use std::{
        cell::{Cell, RefCell},
        path::Path,
        rc::Rc,
    };

    #[test]
    fn screenshot_save_is_atomic_and_waits_for_matching_presented_revision() -> Result<()> {
        let root = tempfile::tempdir()?;
        let directory = tempfile::tempdir()?;
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/fonts/chinese.otf"),
            root.path().join("font.otf"),
        )?;
        std::fs::write(
            root.path().join("story.xml"),
            r#"<story schema="8" id="capture" content-version="1" entry="opening"><interface src="ui.xml"/><scripts><script src="script.js"/></scripts><assets><font id="font" src="font.otf"/></assets><characters/><visuals/><frames><frame id="opening"><text>start</text></frame><frame id="ending"><text>end</text></frame></frames><next end="true"/></story>"#,
        )?;
        std::fs::write(
            root.path().join("ui.xml"),
            r#"<ui schema="3" width="1280" height="720" initial="hud"><variables><string id="status" initial=""/></variables><screen id="hud" modal="false" pause="false"><scene id="scene" width="1280" height="720"/><button id="saveButton" width="120" height="40" text="保存" event="save"/></screen></ui>"#,
        )?;
        std::fs::write(
            root.path().join("script.js"),
            "frame('opening').advance('ending');frame('ending').end();ui('saveButton').bind('save',ctx=>{ctx.setUi('status','saved');ctx.save(1,true);});",
        )?;
        let bundle = Bundle::load(
            &package::Snapshot::directory(root.path())?,
            &LoadOptions {
                core: None,
                no_live2d: true,
            },
        )?;
        let mut app = PlayerApp::new_context(
            &egui::Context::default(),
            bundle.loaded,
            bundle.assets,
            SaveStore::at(directory.path().to_owned()),
            true,
            bundle.logic,
            crate::render::Renderer::cpu(),
        )?;
        app.enter(app.story.start(), false, true)?;
        let cache = Rc::new(RefCell::new(Cache::default()));
        app.attach_capture(cache.clone());
        let reads = Rc::new(Cell::new(0));
        let frame = |revision, color| {
            let reads = reads.clone();
            Frame {
                revision,
                read: Rc::new(move || {
                    reads.set(reads.get() + 1);
                    Ok(ColorImage::filled([16, 9], color))
                }),
            }
        };
        cache.borrow_mut().game = Some(frame(app.revision - 1, Color32::RED));
        app.finish_autosave();
        assert!(app.pending_auto.is_some());
        assert_eq!(reads.get(), 0);
        assert!(!app.saves.auto_path(1)?.exists());
        cache.borrow_mut().game = Some(frame(app.revision, Color32::GREEN));
        app.finish_autosave();
        assert!(app.pending_auto.is_none());
        assert_eq!(reads.get(), 1);
        let auto = std::fs::read(app.saves.auto_path(1)?)?;
        let slot = SlotId::new(1)?;
        app.custom_event(Event::ui("saveButton", "save"))?;
        let bytes = std::fs::read(app.saves.slot_path(slot))?;
        let snapshot: Snapshot = serde_json::from_slice(&bytes)?;
        assert_eq!(
            snapshot.screenshot.unwrap().decode()?.get_pixel(160, 90).0,
            [0, 255, 0, 255]
        );
        assert!(snapshot.saved_at_unix_ms > 0);
        app.assets.update_stream(
            &app.context,
            "$save:manual:1",
            ColorImage::filled([16, 9], Color32::GREEN),
        );
        app.rebuild_cpu_presentation()?;
        assert!(app.assets.has_stream("$save:manual:1"));
        assert!(cache.borrow().frame(app.revision)?.is_none());
        let before = app.session.clone();
        app.custom.state.variables.insert(
            "status".into(),
            crate::model::VariableValue::String(String::new()),
        );
        cache.borrow_mut().error = Some("injected readback error".into());
        assert!(app.custom_event(Event::ui("saveButton", "save")).is_err());
        assert_eq!(app.session, before);
        assert_eq!(std::fs::read(app.saves.slot_path(slot))?, bytes);
        assert_eq!(app.custom.state.variables["status"].to_string(), "");
        cache.borrow_mut().error = None;
        cache.borrow_mut().game = Some(frame(app.revision - 1, Color32::GREEN));
        cache.borrow_mut().latest = Some(frame(app.revision, Color32::RED));
        app.save_current(slot)?;
        let saved: Snapshot = serde_json::from_slice(&std::fs::read(app.saves.slot_path(slot))?)?;
        assert_eq!(
            saved.screenshot.unwrap().decode()?.get_pixel(160, 90).0,
            [255, 0, 0, 255]
        );
        let result = controller::transition_with(
            &app.story,
            &app.session,
            controller::Intent::Advance,
            &app.logic,
        )
        .map_err(anyhow::Error::msg)?;
        app.enter(result.session, false, false)?;
        app.finish_autosave();
        assert!(app.pending_auto.is_some());
        let blocked = app.saves.auto_path(2)?;
        std::fs::create_dir(&blocked)?;
        std::fs::write(blocked.join("marker"), b"old")?;
        cache.borrow_mut().game = Some(frame(app.revision, Color32::BLUE));
        app.finish_autosave();
        assert!(!app.settings.autosave);
        assert!(app.pending_auto.is_none());
        assert_eq!(app.session.frame_id, "ending");
        assert_eq!(std::fs::read(blocked.join("marker"))?, b"old");
        assert_eq!(std::fs::read(app.saves.auto_path(1)?)?, auto);
        Ok(())
    }
}
