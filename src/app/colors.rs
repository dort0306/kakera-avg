//! Player color preferences: independent strict file, loaded before the first frame.
use super::*;

impl PlayerApp {
    pub(super) fn load_colors(
        saves: &SaveStore,
        story_id: &str,
        def: &crate::interface::Definition,
        state: &mut crate::interface::host::State,
        warnings: &mut Vec<String>,
    ) {
        let path = saves.directory().join("ui-colors.json");
        if !path.exists() {
            return;
        }
        let result = (|| -> anyhow::Result<crate::interface::palette::Colors> {
            let bytes =
                crate::save::read_bytes_limited(&path, crate::interface::palette::MAX_FILE_BYTES)?;
            crate::interface::palette::ColorsFile::load_bytes(&bytes, story_id, &def.palette)
        })();
        match result {
            Ok(colors) => {
                state.colors = colors;
                if let Err(error) = state.refresh(def) {
                    warnings.push(crate::i18n::format(
                        "host-app-colors-0001",
                        &[("arg0", format!("{:#}", error))],
                    ));
                    state.colors = crate::interface::palette::Colors::default();
                    let _ = state.refresh(def);
                }
            }
            Err(error) => {
                warnings.push(crate::i18n::format(
                    "host-app-colors-0002",
                    &[("arg0", format!("{:#}", error))],
                ));
            }
        }
    }

    pub(super) fn write_colors(
        &self,
        colors: &crate::interface::palette::Colors,
    ) -> anyhow::Result<()> {
        let file = crate::interface::palette::ColorsFile {
            schema_version: 1,
            story_id: self.story.id.clone(),
            overrides: crate::interface::palette::Overrides(colors.overrides.clone()),
        };
        crate::save::atomic_json_limited(
            &self.saves.directory().join("ui-colors.json"),
            &file,
            crate::interface::palette::MAX_FILE_BYTES,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        interface::Event,
        package::Snapshot,
        render::Renderer,
        runtime::{Bundle, LoadOptions},
    };
    use std::fs;

    const STORY: &str = r#"<story schema="8" id="colors-test" content-version="1" entry="opening"><scripts><script src="story.js"/></scripts><interface src="ui.xml"/><assets><font id="font" src="font.otf"/></assets><characters/><visuals/><frames><frame id="opening"><text>开始。</text></frame></frames><next end="true"/></story>"#;
    const UI: &str = r##"<ui schema="3" initial="hud"><variables><string id="draft" initial="#00FF00FF"/></variables><palette><color id="accent" initial="#10253BFF"/></palette><screen id="hud" modal="false" pause="false"><scene id="scene" width="1280" height="720"/><button id="apply" width="120" height="40" text="应用" event="apply"/><button id="clear" width="120" height="40" text="清除" event="clear"/><button id="reset" width="120" height="40" text="重置" event="reset"/><button id="mixed" width="120" height="40" text="混合" event="mixed"/></screen></ui>"##;
    const SCRIPT: &str = "frame('opening').end();(event.register('apply', ctx=>ctx.colorPreference('accent',ctx.ui.draft)), ui('apply').bind('apply'));(event.register('clear', ctx=>ctx.colorPreference('accent',null)), ui('clear').bind('clear'));(event.register('reset', ctx=>ctx.resetColors()), ui('reset').bind('reset'));(event.register('mixed', ctx=>{ctx.colorPreference('accent','#00FF00FF');ctx.setting('muted',true);}), ui('mixed').bind('mixed'));";

    struct Harness {
        app: PlayerApp,
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
        let ctx = eframe::egui::Context::default();
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
        app.enter(app.story.start(), false, true)?;
        app.pending_auto = None;
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
            Ok(Self { app, game, saves })
        }
        fn reopen(&self) -> anyhow::Result<PlayerApp> {
            open(self.game.path(), self.saves.path())
        }
        fn event(&mut self, name: &str) -> anyhow::Result<()> {
            self.app.custom_event(Event::ui(name, name))
        }
        fn file(&self) -> std::path::PathBuf {
            self.saves.path().join("ui-colors.json")
        }
        fn accent(&self) -> String {
            self.app.custom.state.resolved_colors["accent"].format()
        }
    }

    #[test]
    fn apply_reload_clear_reset_and_bad_file_are_atomic() -> anyhow::Result<()> {
        let mut h = Harness::new()?;
        assert_eq!(h.accent(), "#10253BFF");
        h.event("apply")?;
        assert_eq!(h.accent(), "#00FF00FF");
        let bytes = fs::read(h.file())?;
        let colors = crate::interface::palette::ColorsFile::load_bytes(
            &bytes,
            "colors-test",
            &h.app.custom.definition.palette,
        )
        .unwrap();
        assert_eq!(colors.overrides["accent"].format(), "#00FF00FF");
        let reopened = h.reopen()?;
        assert_eq!(
            reopened.custom.state.resolved_colors["accent"].format(),
            "#00FF00FF"
        );
        drop(reopened);

        h.event("clear")?;
        assert_eq!(h.accent(), "#10253BFF");
        assert!(h.app.custom.state.colors.overrides.is_empty());
        assert!(h.file().exists(), "clearing one override keeps the file");
        h.event("apply")?;
        h.event("reset")?;
        assert!(h.app.custom.state.colors.overrides.is_empty());
        let bytes = fs::read(h.file())?;
        let colors = crate::interface::palette::ColorsFile::load_bytes(
            &bytes,
            "colors-test",
            &h.app.custom.definition.palette,
        )
        .unwrap();
        assert!(colors.overrides.is_empty());

        fs::write(h.file(), b"{bad json")?;
        let warnings = h.reopen().unwrap().message.unwrap_or_default();
        assert!(warnings.contains("配色偏好无效"), "{warnings}");
        assert_eq!(fs::read(h.file())?, b"{bad json");

        fs::remove_file(h.file())?;
        fs::create_dir(h.file())?;
        fs::write(h.file().join("marker"), b"old")?;
        let before = h.accent();
        assert!(h.event("apply").is_err());
        assert_eq!(h.accent(), before);
        assert_eq!(fs::read(h.file().join("marker"))?, b"old");
        Ok(())
    }

    #[test]
    fn color_operations_cannot_mix_with_settings() -> anyhow::Result<()> {
        let mut h = Harness::new()?;
        let before = h.app.custom.state.colors.clone();
        assert!(h.event("mixed").is_err());
        assert_eq!(h.app.custom.state.colors, before);
        assert!(!h.file().exists());
        Ok(())
    }

    #[test]
    fn picker_gap_blocks_same_frame_fast_forward_and_autoplay() -> anyhow::Result<()> {
        use eframe::egui;
        let h = Harness::new()?;
        fs::write(
            h.game.path().join("story.xml"),
            STORY.replace(
                "</frames>",
                "<frame id=\"second\"><text>第二句。</text></frame></frames>",
            ),
        )?;
        fs::write(h.game.path().join("ui.xml"), UI.replace(
            "</screen>", "<color-picker id=\"picker\" x=\"100\" y=\"100\" width=\"256\" height=\"282\" value=\"ui.draft\" event=\"pick\"/></screen>",
        ))?;
        fs::write(h.game.path().join("story.js"), SCRIPT.replace(
            "frame('opening').end();", "frame('opening').advance(()=> 'second');frame('second').end();(event.register('pick', ctx=>ctx.setUi('draft',ctx.value)), ui('picker').bind('pick'));",
        ))?;
        for fast_forward in [false, true] {
            let mut app = h.reopen()?;
            app.message = None;
            app.settings.skip_all = true;
            app.typewriter.finish();
            let ctx = app.context.clone();
            let update = |app: &mut PlayerApp, events, modifiers| {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1280.0, 720.0),
                        )),
                        focused: true,
                        events,
                        modifiers,
                        ..Default::default()
                    },
                    |ctx| app.update_ui(ctx),
                );
            };
            update(&mut app, vec![], egui::Modifiers::default());
            if !fast_forward {
                app.auto_play.set(true);
                app.settings.auto_delay = 0.0;
            }
            let modifiers = egui::Modifiers {
                ctrl: fast_forward,
                ..Default::default()
            };
            // 256x282 alpha=true: D=256, centre (228, 228), ring radius 122. This point
            // sits inside the control but outside the ring, square and alpha row.
            let point = egui::pos2(140.0, 340.0);
            let before = app.session.clone();
            update(
                &mut app,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers,
                    },
                ],
                modifiers,
            );
            assert_eq!(app.session, before, "fast_forward={fast_forward}");
            assert!(app.input_barrier);
        }
        Ok(())
    }
}
