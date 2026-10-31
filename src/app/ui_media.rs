use super::*;
use crate::interface::{
    host::{self, Operation, Plan},
    presentation::{Action, Finish, Preferences, Step},
};
use anyhow::{Result, ensure};
use std::collections::BTreeMap;

pub(super) struct Running {
    id: String,
    index: usize,
    age: f32,
    entered: bool,
    video: crate::media::VideoPlayer,
    final_stack: Vec<String>,
}
pub(super) struct UiMedia {
    pub prefs: Preferences,
    running: Option<Running>,
    pub(super) scope: Option<String>,
    clock: PresentationClock,
    pub alpha: BTreeMap<String, f32>,
}
pub(super) struct Prepared {
    pub(super) prefs: Preferences,
    sounds: Vec<String>,
    running: Option<Running>,
}
impl UiMedia {
    #[cfg(test)]
    pub(super) fn elapse_for_test(&mut self, seconds: u64) {
        self.clock = PresentationClock::new(Instant::now() - Duration::from_secs(seconds));
        self.clock.set_paused(false);
    }
    pub fn new(prefs: Preferences) -> Self {
        Self {
            prefs,
            running: None,
            scope: None,
            clock: PresentationClock::new(Instant::now()),
            alpha: BTreeMap::new(),
        }
    }
    pub fn position(&self) -> f64 {
        self.running
            .as_ref()
            .map(|r| r.video.position)
            .unwrap_or(0.)
    }
    pub fn active(&self) -> bool {
        self.running.is_some()
    }
    pub fn video(&self) -> bool {
        self.running.as_ref().is_some_and(|r| r.video.cue.is_some())
    }
    pub fn cancel(&mut self) {
        self.running = None;
        self.alpha.clear();
        self.clock = PresentationClock::new(Instant::now());
    }
}
impl PlayerApp {
    pub(super) fn prepare_ui_media(&self, plan: &Plan) -> Result<Prepared> {
        for text in plan.state.localized_text.values() {
            self.cpu_assets.validate_text(text)?;
        }
        self.cpu_assets.validate_text(&plan.state.window_title)?;
        let catalog = &self.custom.definition.media;
        let mut prefs = self.ui_media.prefs.clone();
        let mut running = None;
        let mut sounds = Vec::new();
        for action in &plan.ui_actions {
            catalog.preference(&mut prefs, action)?;
        }
        for action in &plan.ui_actions {
            match action {
                Action::Sound(id) => {
                    if let Some(sound) = catalog.choose(id, &prefs)? {
                        sounds.push(sound);
                    }
                }
                Action::Sequence(id) => {
                    ensure!(
                        !self.ui_media.active() && self.pending_auto.is_none(),
                        "{}",
                        crate::i18n::format("host-app-ui-media-0001", &[])
                    );
                    let sequence = &catalog.sequences[id];
                    let commands = match &sequence.finish {
                        Finish::Return => vec![],
                        Finish::Open(id) => vec![host::Command::Open { screen: id.clone() }],
                        Finish::Close => vec![host::Command::Close],
                    };
                    let final_plan = host::prepare(
                        &self.custom.definition,
                        &plan.state,
                        &plan.settings,
                        &plan.profile,
                        commands,
                        false,
                    )?;
                    let duration = sequence
                        .steps
                        .iter()
                        .map(|s| match s {
                            Step::Fade { duration, .. } => f64::from(*duration),
                            Step::Video(id) => self.prepared_media.videos[id].duration,
                            _ => 0.,
                        })
                        .sum::<f64>();
                    ensure!(
                        duration <= 600.,
                        "{}",
                        crate::i18n::format("host-app-ui-media-0002", &[])
                    );
                    running = Some(Running {
                        id: id.clone(),
                        index: 0,
                        age: 0.,
                        entered: false,
                        video: Default::default(),
                        final_stack: final_plan.state.stack,
                    });
                }
                _ => {}
            }
        }
        ensure!(
            plan.ui_actions.is_empty()
                || matches!(
                    plan.operation,
                    Operation::None | Operation::UiAudio | Operation::Present
                ),
            "{}",
            crate::i18n::format("host-app-ui-media-0003", &[])
        );
        Ok(Prepared {
            prefs,
            sounds,
            running,
        })
    }
    pub(super) fn apply_ui_media(&mut self, prepared: Prepared) {
        self.ui_media.prefs = prepared.prefs;
        for sound in prepared.sounds {
            self.audio.play_ui(&sound);
        }
        if let Some(running) = prepared.running {
            self.ui_media.scope = self.custom.state.stack.last().cloned();
            self.ui_media.running = Some(running);
            self.ui_media.clock = PresentationClock::new(Instant::now());
            self.auto_play.set(false);
        }
    }
    pub(super) fn tick_ui_media(
        &mut self,
        ctx: &egui::Context,
        focused: bool,
        skip_key: bool,
    ) -> Result<bool> {
        let top = self.custom.state.stack.last().cloned();
        if self.ui_media.scope != top {
            if self.ui_media.video() {
                self.audio.video(None);
            }
            self.ui_media.cancel();
            self.effects.ui_alpha.clear();
            self.ui_media.scope = top.clone();
            if let Some(sequence) =
                top.and_then(|s| self.custom.definition.screens[&s].sequence.clone())
            {
                // A page-open trigger is a separate transaction after the page was committed.
                if self.pending_auto.is_some() {
                    self.ui_media.scope = None;
                    return Ok(false);
                }
                let plan = host::prepare(
                    &self.custom.definition,
                    &self.custom.state,
                    &self.settings,
                    &self.custom.profile,
                    vec![host::Command::Sequence { id: sequence }],
                    false,
                )?;
                let prepared = self.prepare_ui_media(&plan)?;
                self.apply_ui_media(prepared);
            }
        }
        let Some(mut running) = self.ui_media.running.take() else {
            return Ok(false);
        };
        let paused = !focused || self.message.is_some() || self.debug_paused();
        let seconds = self
            .ui_media
            .clock
            .elapsed(Instant::now(), paused)
            .as_secs_f32();
        let sequence = self.custom.definition.media.sequences[&running.id].clone();
        let skip = !paused
            && sequence.skippable
            && (skip_key || ctx.input(|i| i.pointer.primary_clicked()));
        if skip {
            for step in &sequence.steps {
                if let Step::Fade { target, to, .. } = step {
                    self.ui_media.alpha.insert(target.clone(), *to);
                }
            }
            if running.video.cue.is_some() {
                self.audio.video(None);
            }
            running.index = sequence.steps.len();
        } else if let Some(step) = sequence.steps.get(running.index) {
            let finished = match step {
                Step::Fade {
                    target,
                    from,
                    to,
                    duration,
                } => {
                    running.age = (running.age + seconds).min(*duration);
                    let t = running.age / duration;
                    let t = t * t * (3. - 2. * t);
                    self.ui_media
                        .alpha
                        .insert(target.clone(), from + (to - from) * t);
                    running.age >= *duration
                }
                Step::Sound(id) => {
                    if !paused {
                        if let Some(sound) = self
                            .custom
                            .definition
                            .media
                            .choose(id, &self.ui_media.prefs)?
                        {
                            self.audio.play_ui(&sound);
                        }
                        true
                    } else {
                        false
                    }
                }
                Step::Video(id) => {
                    if !running.entered {
                        running.video = crate::media::VideoPlayer::prepare(
                            Some(&crate::media::VideoCue {
                                asset: id.clone(),
                                skippable: sequence.skippable,
                            }),
                            &self.prepared_media,
                        )?;
                        self.audio.video(Some(id));
                        running.entered = true;
                    }
                    if let Err(error) =
                        running
                            .video
                            .tick_named(seconds, ctx, &mut self.assets, "$ui-video")
                    {
                        self.ui_media.running = Some(running);
                        return Err(error);
                    }
                    if running.video.ended {
                        self.audio.video(None);
                        true
                    } else {
                        false
                    }
                }
            };
            if finished {
                running.index += 1;
                running.age = 0.;
                running.entered = false;
                running.video = Default::default();
            }
        }
        if running.index == sequence.steps.len() {
            // Leaving a sequence requires a fresh press; a key still held for the skip
            // must not immediately cascade into story advance/fast-forward.
            self.input_barrier = true;
            let mut plan = host::prepare(
                &self.custom.definition,
                &self.custom.state,
                &self.settings,
                &self.custom.profile,
                vec![],
                false,
            )?;
            plan.state.stack = running.final_stack;
            let plan = self.page_lifecycle(plan, &self.session, None)?;
            self.commit_page_plan(plan)?;
        } else {
            self.ui_media.running = Some(running);
        }
        self.effects.ui_alpha = self.ui_media.alpha.clone();
        Ok(true)
    }
    pub(super) fn draw_ui_video(&self, ui: &mut egui::Ui) {
        if !self.ui_media.video() {
            return;
        }
        let bounds = ui.max_rect();
        ui.painter().rect_filled(bounds, 0., egui::Color32::BLACK);
        if let Some(texture) = self.assets.image("$ui-video") {
            let size = texture.size_vec2();
            let scale = (bounds.width() / size.x).min(bounds.height() / size.y);
            let rect = egui::Rect::from_center_size(bounds.center(), size * scale);
            ui.painter().image(
                texture.id(),
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                egui::Color32::WHITE,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        interface::Event,
        package,
        runtime::{Bundle, LoadOptions},
        save::{SaveStore, SlotId},
    };
    use std::path::Path;
    fn fixture(video: bool) -> Result<(PlayerApp, tempfile::TempDir, tempfile::TempDir)> {
        let root = tempfile::tempdir()?;
        let saves = tempfile::tempdir()?;
        let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/demo");
        std::fs::copy(
            demo.join("assets/fonts/chinese.otf"),
            root.path().join("font.otf"),
        )?;
        let media = if video {
            std::fs::copy(
                demo.join("assets/video/intro.mp4"),
                root.path().join("intro.mp4"),
            )?;
            "<video id=\"intro\" src=\"intro.mp4\"/>"
        } else {
            ""
        };
        std::fs::write(
            root.path().join("story.xml"),
            format!(
                r#"<story schema="8" id="presentation" content-version="8" entry="start"><scripts><script src="story.js"/></scripts><interface src="ui.xml"/><assets><font id="font" src="font.otf"/>{media}</assets><characters><character id="a" name="甲"/></characters><visuals/><frames><frame id="start"><text>开始。</text></frame></frames><next end="true"/></story>"#
            ),
        )?;
        let video = if video {
            "<sequence id=\"movie\" finish=\"open:hud\"><video asset=\"intro\"/></sequence>"
        } else {
            ""
        };
        std::fs::write(
            root.path().join("ui.xml"),
            format!(
                r#"<ui schema="3" initial="hud"><variables><string id="status" initial="old"/></variables><ui-media><sequence id="reveal"><fade target="title" duration="1000"/></sequence><sequence id="leave" finish="close"><fade target="title" from="1" to="0" duration="1000"/></sequence>{video}</ui-media><screen id="hud" modal="false" pause="false"><scene id="scene" width="1280" height="720"/><button id="leave" width="10" height="10" text="键" event="leave"/><button id="movie" width="10" height="10" text="键" event="movie"/><button id="role" width="10" height="10" text="键" event="role"/><button id="mixed" width="10" height="10" text="键" event="mixed"/></screen><screen id="menu" sequence="reveal"><label id="title" width="400" height="50" text="演出"/></screen></ui>"#
            ),
        )?;
        std::fs::write(
            root.path().join("story.js"),
            "frame('start').end();ui('leave').bind('leave',ctx=>ctx.sequence('leave'));ui('movie').bind('movie',ctx=>ctx.sequence('movie'));ui('role').bind('role',ctx=>{ctx.setUi('status','changed');ctx.soundActor('a',false);});ui('mixed').bind('mixed',ctx=>{ctx.sequence('reveal');ctx.restart();});",
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
            crate::render::Renderer::cpu(),
        )?;
        app.enter(app.story.start(), false, true)?;
        app.pending_auto = None;
        app.message = None;
        Ok((app, root, saves))
    }
    fn event(name: &str) -> Event {
        Event::ui(name, name)
    }
    fn elapse(app: &mut PlayerApp, seconds: f32) {
        app.ui_media.clock =
            PresentationClock::new(Instant::now() - Duration::from_secs_f32(seconds));
        app.ui_media.clock.set_paused(false);
    }
    fn space(app: &mut PlayerApp) -> Result<bool> {
        let ctx = app.context.clone();
        let mut result = Ok(false);
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Space,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
                ..Default::default()
            },
            |ctx| {
                result = app.tick_ui_media(ctx, true, true);
            },
        );
        result
    }
    #[test]
    fn automatic_fade_pauses_skips_once_and_clears_on_page_change() -> Result<()> {
        let (mut app, _root, _saves) = fixture(false)?;
        let before = app.session.clone();
        let ctx = app.context.clone();
        app.custom.state.stack.push("menu".into());
        assert!(app.tick_ui_media(&ctx, true, false)?);
        assert!(app.ui_media.active());
        assert_eq!(app.ui_media.alpha["title"], 0.);
        elapse(&mut app, 0.5);
        app.tick_ui_media(&ctx, true, false)?;
        assert!((app.ui_media.alpha["title"] - 0.5).abs() < 0.05);
        let alpha = app.ui_media.alpha["title"];
        elapse(&mut app, 3.);
        app.tick_ui_media(&ctx, false, false)?;
        assert_eq!(app.ui_media.alpha["title"], alpha);
        assert!(space(&mut app)?);
        assert!(!app.ui_media.active());
        assert_eq!(app.effects.ui_alpha["title"], 1.);
        assert!(!app.tick_ui_media(&ctx, true, false)?);
        assert_eq!(app.session, before);
        app.custom_event(event("leave"))?;
        assert!(app.save_current(SlotId::new(1)?).is_err());
        space(&mut app)?;
        assert_eq!(app.custom.state.stack, vec!["hud"]);
        assert!(!app.ui_media.active());
        app.tick_ui_media(&ctx, true, false)?;
        assert!(app.effects.ui_alpha.is_empty());
        assert_eq!(app.session, before);
        Ok(())
    }
    #[test]
    fn ui_preference_io_failure_and_mixed_story_commands_roll_back() -> Result<()> {
        let (mut app, _root, saves) = fixture(false)?;
        let before = app.session.clone();
        std::fs::create_dir(saves.path().join("ui-audio.json"))?;
        std::fs::write(saves.path().join("ui-audio.json/marker"), b"old")?;
        assert!(app.custom_event(event("role")).is_err());
        assert_eq!(app.custom.state.variables["status"].to_string(), "old");
        assert!(app.ui_media.prefs.disabled_actors.is_empty());
        assert_eq!(
            std::fs::read(saves.path().join("ui-audio.json/marker"))?,
            b"old"
        );
        assert!(app.custom_event(event("mixed")).is_err());
        assert!(!app.ui_media.active());
        assert_eq!(app.session, before);
        Ok(())
    }
    #[test]
    fn ui_video_prepares_and_skips_to_target_without_story_transition() -> Result<()> {
        let (mut app, _root, _saves) = fixture(true)?;
        let ctx = app.context.clone();
        let before = app.session.clone();
        app.ui_media.scope = Some("menu".into());
        app.custom.state.stack.push("menu".into());
        app.custom_event(event("movie"))?;
        app.tick_ui_media(&ctx, true, false)?;
        assert!(app.ui_media.video());
        space(&mut app)?;
        assert!(!app.ui_media.active());
        assert!(!app.ui_media.video());
        assert_eq!(app.custom.state.stack, vec!["hud"]);
        assert_eq!(app.session, before);
        assert!(!app.tick_ui_media(&ctx, true, false)?);
        Ok(())
    }
}
