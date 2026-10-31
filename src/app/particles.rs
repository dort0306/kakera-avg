use super::*;

impl PlayerApp {
    /// Advances particle presentation on the UI clock. A story menu does not freeze
    /// decoration; focus loss, host prompts and debug pauses freeze it without
    /// accumulating time. Returns whether the clock is currently running so the caller
    /// can schedule redraws.
    pub(super) fn tick_particles(&mut self, now: Instant, focused: bool) -> bool {
        let paused = !focused || self.message.is_some() || self.debug_paused();
        let seconds = self.particle_clock.elapsed(now, paused).as_secs_f32();
        self.particles.tick(seconds);
        !paused
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        interface::Event,
        package,
        render::Renderer,
        runtime::{Bundle, LoadOptions},
        save::SaveStore,
    };
    use anyhow::Result;
    use std::path::Path;

    fn fixture() -> Result<(PlayerApp, tempfile::TempDir, tempfile::TempDir)> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/ui/particle-snow.png"),
            root.path().join("dot.png"),
        )?;
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo/assets/fonts/chinese.otf"),
            root.path().join("font.otf"),
        )?;
        std::fs::write(
            root.path().join("story.xml"),
            r#"<story schema="8" id="particles" content-version="11" entry="start"><scripts><script src="story.js"/></scripts><interface src="ui.xml"/><assets><font id="font" src="font.otf"/><image id="dot" src="dot.png"/></assets><characters><character id="a" name="甲"/></characters><visuals/><frames><frame id="start"><text>开始。</text></frame></frames><next end="true"/></story>"#,
        )?;
        std::fs::write(
            root.path().join("ui.xml"),
            r#"<ui schema="3" initial="hud"><variables><bool id="flag" initial="false"/></variables><screen id="hud" modal="false" pause="false"><scene id="scene" width="1280" height="720"/><particles id="rain" x="0" y="0" width="400" height="300" image="dot" rate="40" max="64" life="0.5" size="8" autostart="true"/><particles id="spark" x="0" y="0" width="400" height="300" image="dot" burst="6" max="32" life="0.5" size="8"/><button id="start" x="0" y="0" width="10" height="10" text="始" event="start"/><button id="stop" x="0" y="0" width="10" height="10" text="停" event="stop"/><button id="clear" x="0" y="0" width="10" height="10" text="清" event="clear"/><button id="burst" x="0" y="0" width="10" height="10" text="爆" event="burst"/><button id="noop" x="0" y="0" width="10" height="10" text="空" event="noop"/></screen></ui>"#,
        )?;
        std::fs::write(
            root.path().join("story.js"),
            "frame('start').end();event.register('start', ctx => ctx.particles('rain').start());event.register('stop', ctx => ctx.particles('rain').stop());event.register('clear', ctx => { ctx.particles('rain').clear(); ctx.particles('spark').clear(); });event.register('burst', ctx => ctx.particles('spark').burst());event.register('noop', ctx => ctx.setUi('flag', true));",
        )?;
        let b = Bundle::load(
            &package::Snapshot::directory(root.path())?,
            &LoadOptions {
                core: None,
                no_live2d: true,
            },
        )?;
        let mut app = PlayerApp::new_context(
            &egui::Context::default(),
            b.loaded,
            b.assets,
            SaveStore::at(saves.path().to_owned()),
            true,
            b.logic,
            Renderer::cpu(),
        )?;
        app.message = None;
        app.enter(app.story.start(), false, true)?;
        app.pending_auto = None;
        app.message = None;
        Ok((app, root, saves))
    }
    /// Advances the particle clock with a growing synthetic timestamp; the first call
    /// only releases the initial pause, exactly like a real update after start-up.
    fn advance(app: &mut PlayerApp, at: &mut Instant, focused: bool) {
        *at += Duration::from_millis(100);
        app.tick_particles(*at, focused);
    }
    #[test]
    fn autostart_emitters_follow_pages_and_js_control_is_explicit() -> Result<()> {
        let (mut app, _root, _saves) = fixture()?;
        let mut at = Instant::now();
        advance(&mut app, &mut at, true);
        advance(&mut app, &mut at, true);
        assert!(app.particles.live() > 0, "the page emitter must auto-start");
        // Unfocused updates never advance the decoration clock.
        let before = app.particles.live();
        for _ in 0..5 {
            advance(&mut app, &mut at, false);
        }
        assert_eq!(app.particles.live(), before);
        // stop keeps survivors, clear removes them, and neither re-arms on commit.
        app.custom_event(Event::ui("stop", "stop"))?;
        assert!(!app.particles.emitting("rain"));
        app.custom_event(Event::ui("clear", "clear"))?;
        assert_eq!(app.particles.live(), 0);
        app.custom_event(Event::ui("noop", "noop"))?;
        advance(&mut app, &mut at, true);
        advance(&mut app, &mut at, true);
        assert_eq!(
            app.particles.live(),
            0,
            "clear suppresses automatic emission"
        );
        // A burst-only emitter spawns on demand and never enters continuous emission.
        app.custom_event(Event::ui("burst", "burst"))?;
        advance(&mut app, &mut at, true);
        assert!(app.particles.live() > 0);
        assert!(!app.particles.emitting("spark"));
        // An explicit start re-arms the automatic emitter without reopening the page.
        assert!(!app.particles.emitting("rain"));
        app.custom_event(Event::ui("start", "start"))?;
        assert!(app.particles.emitting("rain"));
        advance(&mut app, &mut at, true);
        advance(&mut app, &mut at, true);
        assert!(app.particles.live() > 0);
        Ok(())
    }
}
