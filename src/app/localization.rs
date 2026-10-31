use super::*;
use crate::interface::{
    Definition,
    host::{Plan, State},
};
use crate::localization::{Preferences, Requests};
use anyhow::{Result, ensure};

impl PlayerApp {
    pub(super) fn load_languages(
        saves: &SaveStore,
        story: &str,
        def: &Definition,
        state: &mut State,
        warnings: &mut Vec<String>,
    ) -> Result<()> {
        if !def.localization.enabled() {
            return Ok(());
        }
        let path = saves.directory().join("language.json");
        let requests = if path.exists() {
            crate::save::read_bytes_limited(&path, 16384)
                .and_then(|bytes| Preferences::load(&bytes, story))
                .unwrap_or_else(|error| {
                    warnings.push(crate::i18n::format(
                        "host-language-invalid",
                        &[("reason", format!("{error:#}"))],
                    ));
                    Requests::default()
                })
        } else {
            Requests::default()
        };
        let system = crate::localization::system_languages().unwrap_or_else(|error| {
            warnings.push(crate::i18n::format(
                "host-language-system",
                &[("reason", error.to_string())],
            ));
            Vec::new()
        });
        state.languages = def.localization.selection(requests, system);
        if state
            .languages
            .requests
            .text
            .as_ref()
            .is_some_and(|id| !def.localization.texts.contains_key(id))
            || state
                .languages
                .requests
                .voice
                .as_ref()
                .is_some_and(|id| !def.localization.voices.contains_key(id))
        {
            warnings.push(crate::i18n::text("host-language-fallback").into());
        }
        state.refresh(def)
    }
    pub(super) fn commit_language_plan(&mut self, plan: Plan) -> Result<()> {
        ensure!(
            self.handoff.is_none()
                && !self.ui_media.active()
                && !self.effects.active()
                && !self.debug_paused()
                && self.pending_auto.is_none(),
            "{}",
            crate::i18n::text("host-i18n-detail-0067")
        );
        let prepared = self.prepare_ui_media(&plan)?;
        let catalog = &self.custom.definition.localization;
        let text_changed = plan.state.languages.text != self.custom.state.languages.text;
        let voice_changed = plan.state.languages.voice != self.custom.state.languages.voice;
        for text in plan.state.localized_text.values() {
            self.cpu_assets.validate_text(text)?;
        }
        self.cpu_assets.validate_text(&plan.state.window_title)?;
        let new_text = catalog.frame_text(
            &self.story.frames[&self.session.frame_id],
            plan.state.languages.text.as_deref(),
        );
        let fonts = plan
            .state
            .languages
            .text
            .as_ref()
            .and_then(|id| catalog.texts.get(id))
            .map(|l| l.fonts.clone())
            .unwrap_or_default();
        let file = Preferences {
            schema_version: 1,
            story_id: self.story.id.clone(),
            text: plan.state.languages.requests.text.clone(),
            voice: plan.state.languages.requests.voice.clone(),
        };
        let font_definitions = text_changed.then(|| self.cpu_assets.font_definitions(&fonts));
        crate::save::atomic_json_limited(
            &self.saves.directory().join("language.json"),
            &file,
            16384,
        )?;
        if text_changed {
            self.context
                .set_fonts(font_definitions.expect("prepared text language fonts"));
            self.typewriter = Typewriter::new(&new_text, true);
            self.revision = self.revision.wrapping_add(1);
        }
        if voice_changed {
            self.audio.stop_voice();
        }
        self.commit_ui(plan, true);
        self.apply_ui_media(prepared);
        self.auto_play.set(false);
        self.skip = Default::default();
        self.cancel_shortcut_capture();
        self.pickers = Default::default();
        self.input_barrier = true;
        self.context.memory_mut(|memory| {
            if let Some(id) = memory.focused() {
                memory.surrender_focus(id)
            }
        });
        self.context.request_repaint();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        content::DirectorySource,
        interface::host,
        runtime::{Bundle, LoadOptions},
    };

    fn example() -> Result<tempfile::TempDir> {
        let game = tempfile::tempdir()?;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for entry in std::fs::read_dir(root.join("doc/localization-example"))? {
            let entry = entry?;
            std::fs::copy(entry.path(), game.path().join(entry.file_name()))?;
        }
        std::fs::create_dir(game.path().join("assets"))?;
        for (source, name) in [
            ("fonts/chinese.otf", "chinese.otf"),
            ("audio/lin.wav", "lin.wav"),
            ("audio/knock.wav", "knock.wav"),
        ] {
            std::fs::copy(
                root.join("games/demo/assets").join(source),
                game.path().join("assets").join(name),
            )?;
        }
        Ok(game)
    }
    fn open(game: &std::path::Path, saves: &std::path::Path) -> Result<PlayerApp> {
        let b = Bundle::load(
            &DirectorySource::new(game)?,
            &LoadOptions {
                core: None,
                no_live2d: true,
            },
        )?;
        PlayerApp::new_context(
            &egui::Context::default(),
            b.loaded,
            b.assets,
            SaveStore::at(saves.to_owned()),
            true,
            b.logic,
            crate::render::Renderer::cpu(),
        )
    }
    fn plan(app: &PlayerApp, channel: &str, locale: Option<&str>) -> Result<Plan> {
        host::prepare(
            &app.custom.definition,
            &app.custom.state,
            &app.settings,
            &app.custom.profile,
            vec![host::Command::Locale {
                channel: channel.into(),
                locale: locale.map(Into::into),
            }],
            false,
        )
    }
    #[test]
    fn text_switch_preserves_story_and_preferences_are_independent_of_slots() -> Result<()> {
        let game = example()?;
        let saves = tempfile::tempdir()?;
        let mut app = open(game.path(), saves.path())?;
        app.custom_event(crate::interface::Event::ui("start", "start"))?;
        app.settings.autosave = false;
        app.pending_auto = None;
        let original = app.session.clone();
        let revision = app.revision;
        let old_voice = app.custom.state.languages.voice.clone();
        app.commit_language_plan(plan(&app, "text", Some("en-US"))?)?;
        assert_eq!(app.session, original);
        assert_eq!(app.custom.state.languages.voice, old_voice);
        assert!(app.typewriter.complete());
        assert_eq!(app.revision, revision.wrapping_add(1));
        assert_eq!(
            app.custom.data.values["game"]["text"],
            "Text and voice are selected independently. Changing text completes this line; voice changes take effect at the next scene entry."
        );
        let language = std::fs::read(saves.path().join("language.json"))?;
        let revision = app.revision;
        app.commit_language_plan(plan(&app, "voice", Some("ja-JP"))?)?;
        assert_eq!(
            app.revision, revision,
            "voice changes never invalidate scene pixels"
        );
        assert_eq!(app.session, original);
        app.commit_language_plan(plan(&app, "text", Some("en-US"))?)?;
        assert_eq!(
            app.revision, revision,
            "same effective text never resets the sentence"
        );
        assert_eq!(
            Preferences::load(&language, "localization-example")?
                .text
                .as_deref(),
            Some("en-US")
        );
        assert_eq!(
            open(game.path(), saves.path())?.window_title(),
            "KAKERA Localization Example"
        );
        Ok(())
    }
    #[test]
    fn blocked_boundary_and_atomic_failure_preserve_every_active_state() -> Result<()> {
        let game = example()?;
        let saves = tempfile::tempdir()?;
        let mut app = open(game.path(), saves.path())?;
        let original = app.custom.state.clone();
        let session = app.session.clone();
        let revision = app.revision;
        app.pending_auto = Some((
            0,
            crate::save::Snapshot::capture_committed(&app.story, &app.session)?,
        ));
        assert!(
            app.commit_language_plan(plan(&app, "text", Some("en-US"))?)
                .is_err()
        );
        app.pending_auto = None;
        assert!(!saves.path().join("language.json").exists());
        std::fs::create_dir(saves.path().join("language.json"))?;
        assert!(
            app.commit_language_plan(plan(&app, "text", Some("en-US"))?)
                .is_err()
        );
        assert_eq!(app.custom.state, original);
        assert_eq!(app.session, session);
        assert_eq!(app.revision, revision);
        Ok(())
    }
    #[test]
    fn compatible_translation_reload_keeps_current_page_variables_and_session() -> Result<()> {
        let game = example()?;
        let saves = tempfile::tempdir()?;
        let mut app = open(game.path(), saves.path())?;
        app.custom_event(crate::interface::Event::ui("start", "start"))?;
        app.pending_auto = None;
        app.custom
            .state
            .variables
            .insert("count".into(), crate::model::VariableValue::Int(7));
        let session = app.session.clone();
        let stack = app.custom.state.stack.clone();
        let file = game.path().join("zh.ftl");
        std::fs::write(
            &file,
            std::fs::read_to_string(&file)?
                .replace("这段文字与语音分别选择语言。", "翻译热更保留页面。"),
        )?;
        let bundle = Bundle::load(
            &DirectorySource::new(game.path())?,
            &LoadOptions {
                core: None,
                no_live2d: true,
            },
        )?;
        app.replace_content(bundle, session.clone())?;
        assert_eq!(app.session, session);
        assert_eq!(app.custom.state.stack, stack);
        assert_eq!(
            app.custom.state.variables["count"],
            crate::model::VariableValue::Int(7)
        );
        assert!(
            app.custom.data.values["game"]["text"]
                .as_str()
                .unwrap()
                .contains("翻译热更保留页面")
        );
        Ok(())
    }
    #[test]
    fn bad_or_removed_preferences_are_preserved_and_disabled_games_never_read_them() -> Result<()> {
        let game = example()?;
        let saves = tempfile::tempdir()?;
        let path = saves.path().join("language.json");
        std::fs::write(&path, b"broken")?;
        let app = open(game.path(), saves.path())?;
        assert!(app.message.is_some());
        assert_eq!(std::fs::read(&path)?, b"broken");
        std::fs::write(&path,br#"{"schema_version":1,"story_id":"localization-example","text":"fr-FR","voice":"ja-JP"}"#)?;
        let app = open(game.path(), saves.path())?;
        assert_eq!(
            app.custom.state.languages.requests.text.as_deref(),
            Some("fr-FR")
        );
        assert_eq!(app.custom.state.languages.text_source, "default");
        assert_eq!(app.custom.state.languages.text.as_deref(), Some("zh-CN"));
        assert_eq!(app.custom.state.languages.voice.as_deref(), Some("ja-JP"));
        let mut state = app.custom.state.clone();
        let mut def = app.custom.definition.clone();
        def.localization = Default::default();
        std::fs::write(&path, b"broken")?;
        let mut warnings = Vec::new();
        PlayerApp::load_languages(
            &app.saves,
            "localization-example",
            &def,
            &mut state,
            &mut warnings,
        )?;
        assert!(warnings.is_empty());
        assert_eq!(std::fs::read(path)?, b"broken");
        Ok(())
    }
}
