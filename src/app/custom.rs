use super::PlayerApp;
use crate::{
    controller::{self, Intent},
    interface::{
        Data, Definition, Event, data,
        host::{self, Operation, Plan, Profile, State},
    },
    model::{Session, Story},
    presentation::{self, GatedIntent},
    save::{SaveStore, SlotInfo},
    script::ScriptLogic,
};
use anyhow::{Result, ensure};
use serde_json::Value;

pub(super) struct CustomUi {
    pub definition: Definition,
    pub state: State,
    pub profile: Profile,
    pub data: Data,
    pub slots: Vec<SlotInfo>,
    pub autosaves: Vec<Value>,
    pub preview_images: Vec<(String, crate::save_preview::Screenshot)>,
}
impl CustomUi {
    pub fn new(
        definition: Definition,
        story: &Story,
        saves: &SaveStore,
        logic: &ScriptLogic,
        warnings: &mut Vec<String>,
    ) -> Self {
        let path = saves.directory().join("profile.json");
        let profile = if path.exists() {
            Profile::load(&path, &story.id, &definition).unwrap_or_else(|error| {
                warnings.push(crate::i18n::format(
                    "host-app-custom-0001",
                    &[("arg0", format!("{:#}", error))],
                ));
                Profile::new(&story.id, &definition)
            })
        } else {
            Profile::new(&story.id, &definition)
        };
        let state = State::new(&definition);
        let slots = saves.list_page(story, 1).expect("valid page");
        let (autosaves, mut preview_images) = saves.auto_metadata(story);
        for info in &slots {
            if let crate::save::SlotState::Ready {
                screenshot: Some(image),
                ..
            } = &info.state
            {
                preview_images.push((
                    crate::save_preview::image_id(info.slot.number(), false),
                    image.clone(),
                ));
            }
        }
        let _ = logic;
        Self {
            definition,
            state,
            profile,
            data: Data::default(),
            slots,
            autosaves,
            preview_images,
        }
    }
}
impl PlayerApp {
    pub(super) fn prepare_ui_entry(
        &self,
        session: &Session,
        available: &[String],
        restored: bool,
        base: Option<Plan>,
    ) -> Result<Plan> {
        let custom = &self.custom;
        let base = base.unwrap_or(host::prepare(
            &custom.definition,
            &custom.state,
            &self.settings,
            &custom.profile,
            Vec::new(),
            false,
        )?);
        let snapshot = Data::build(data::Input {
            definition: &custom.definition,
            story: &self.story,
            session,
            available,
            state: &base.state,
            profile: &base.profile,
            slots: &custom.slots,
            settings: &base.settings,
            complete: restored || base.settings.text_speed == 0.0,
        });
        let commands = if custom.definition.enter {
            self.logic.ui_event(
                &crate::interface::Event::game(),
                session,
                &base.state.variables,
                &snapshot.values,
            )?
        } else {
            Vec::new()
        };
        let mut plan = host::prepare(
            &custom.definition,
            &base.state,
            &base.settings,
            &base.profile,
            commands,
            true,
        )?;
        plan.command_count += base.command_count;
        ensure!(
            plan.command_count <= 32,
            "{}",
            crate::i18n::format("host-app-custom-0002", &[])
        );
        plan.ui_actions.splice(0..0, base.ui_actions);
        if matches!(plan.operation, Operation::None) {
            plan.operation = base.operation;
        } else {
            ensure!(
                matches!(base.operation, Operation::None),
                "{}",
                crate::i18n::format("host-app-custom-0003", &[])
            );
        }
        plan.message = plan.message.or(base.message);
        plan.animations.splice(0..0, base.animations);
        plan.portraits.splice(0..0, base.portraits);
        plan.live2d.splice(0..0, base.live2d);
        self.prepared_live2d
            .validate_commands(&self.story.frames[&session.frame_id], &plan.live2d)?;
        crate::effects::validate_portraits(&self.story.frames[&session.frame_id], &plan.portraits)
            .map_err(anyhow::Error::msg)?;
        Ok(plan)
    }
    pub(super) fn commit_ui(&mut self, plan: Plan, refresh_slots: bool) {
        self.debug_changed();
        if let Some(enabled) = plan.auto_play {
            self.auto_play.set(enabled);
        }
        if self.custom.state.stack != plan.state.stack {
            self.effects.pages_changed();
            self.input_barrier = true;
            self.cancel_shortcut_capture();
            self.pickers = Default::default();
            self.context.memory_mut(|memory| {
                if let Some(id) = memory.focused() {
                    memory.surrender_focus(id);
                }
            });
        }
        for (target, animation) in plan.animations {
            self.effects.animate(target, animation);
        }
        for (slot, motion) in plan.live2d {
            self.live2d.play(slot, motion);
        }
        for (slot, animation) in plan.portraits {
            self.effects.portrait(slot, animation);
        }
        self.settings = plan.settings;
        if self.settings.text_speed == 0.0 {
            self.typewriter.finish();
        }
        if let Some(message) = plan.message {
            self.message = Some(message);
        }
        {
            let custom = &mut self.custom;
            custom.state = plan.state;
            custom.profile = plan.profile;
            if refresh_slots {
                custom.slots = self
                    .saves
                    .list_page(&self.story, custom.state.slot_page)
                    .expect("validated page");
                let (autosaves, mut images) = self.saves.auto_metadata_with(&self.story, &|id| {
                    custom.definition.localization.frame_text(
                        &self.story.frames[id],
                        custom.state.languages.text.as_deref(),
                    )
                });
                custom.autosaves = autosaves;
                for info in &custom.slots {
                    if let crate::save::SlotState::Ready {
                        screenshot: Some(image),
                        ..
                    } = &info.state
                    {
                        images.push((
                            crate::save_preview::image_id(info.slot.number(), false),
                            image.clone(),
                        ));
                    }
                }
                custom.preview_images = images;
            }
            custom.data = Data::build(data::Input {
                definition: &custom.definition,
                story: &self.story,
                session: &self.session,
                available: &self.available,
                state: &custom.state,
                profile: &custom.profile,
                slots: &custom.slots,
                settings: &self.settings,
                complete: self.typewriter.complete(),
            });
            if !self.effects.has_entered {
                custom.data.awaiting_start();
            }
            custom
                .data
                .lists
                .insert("autosaves".into(), custom.autosaves.clone());
        }
        self.refresh_shortcut_data();
        if refresh_slots {
            self.sync_save_previews();
        }
    }
    pub(super) fn custom_event(&mut self, event: Event) -> Result<()> {
        let custom = &self.custom;
        // Route-only and sound-only events never reach the interpreter. Everything that
        // does must carry a real ordinary event name; reserved lifecycle names cannot be
        // granted by an incoming string.
        let scripted =
            event.route.is_none() && !(event.key.event.is_empty() && event.sound.is_some());
        ensure!(
            !scripted || !crate::interface::reserved_event(&event.key.event),
            "{}",
            crate::i18n::format("host-app-custom-0004", &[])
        );
        let mut commands = if let Some(route) = &event.route {
            vec![route.command()]
        } else if event.key.event.is_empty() && event.sound.is_some() {
            Vec::new()
        } else {
            self.logic.ui_event(
                &event,
                &self.session,
                &custom.state.variables,
                &custom.data.values,
            )?
        };
        if let Some(id) = &event.sound {
            commands.push(host::Command::PlaySound { id: id.clone() });
        }
        let plan = host::prepare(
            &custom.definition,
            &custom.state,
            &self.settings,
            &custom.profile,
            commands,
            false,
        )?;
        let plan = self.page_lifecycle(plan, &self.session, None)?;
        for text in plan.state.localized_text.values() {
            self.cpu_assets.validate_text(text)?;
        }
        self.cpu_assets.validate_text(&plan.state.window_title)?;
        let refresh = plan.state.stack != custom.state.stack
            || plan.state.slot_page != custom.state.slot_page;
        // While a capture waits for its key, ordinary UI clicks must not act on the game.
        // A cancel/restart command from the author's UI is the only allowed callback.
        if self.shortcut_capture_active() && plan.capture.is_some() {
            self.cancel_shortcut_capture();
        }
        if self.shortcut_capture_active() && !plan.cancel_capture {
            self.cancel_shortcut_capture();
            self.pickers = Default::default();
            return Ok(());
        }
        if plan.cancel_capture {
            self.commit_ui(plan, false);
            self.cancel_shortcut_capture();
            return Ok(());
        }
        if let Some(request) = plan.capture.clone() {
            self.commit_ui(plan, false);
            self.start_shortcut_capture(request)?;
            return Ok(());
        }
        ensure!(
            self.effects.has_entered
                || (!matches!(
                    plan.operation,
                    Operation::Save(..)
                        | Operation::Advance
                        | Operation::Choose(_)
                        | Operation::Restart(None)
                ) && plan.auto_play.is_none()
                    && plan.portraits.is_empty()
                    && plan.live2d.is_empty()),
            "{}",
            crate::i18n::format("host-app-custom-0005", &[])
        );
        if matches!(plan.operation, Operation::Present | Operation::Resume(_)) {
            return self.commit_page_plan(plan);
        }
        ensure!(
            self.handoff.is_none() || matches!(plan.operation, Operation::None | Operation::Quit),
            "{}",
            crate::i18n::format("host-app-custom-0006", &[])
        );
        let validate_current = || {
            self.prepared_live2d
                .validate_commands(&self.story.frames[&self.session.frame_id], &plan.live2d)?;
            crate::effects::validate_portraits(
                &self.story.frames[&self.session.frame_id],
                &plan.portraits,
            )
            .map_err(anyhow::Error::msg)
        };
        if !matches!(
            plan.operation,
            Operation::Load(_)
                | Operation::LoadAuto(_)
                | Operation::Restart(_)
                | Operation::Advance
                | Operation::Choose(_)
        ) {
            validate_current()?;
        }
        if matches!(plan.operation, Operation::None) {
            return self.commit_page_plan(plan);
        }
        if matches!(plan.operation, Operation::Locales) {
            return self.commit_language_plan(plan);
        }
        let ui_prepared = self.prepare_ui_media(&plan)?;
        let rebound = matches!(plan.operation, Operation::Keybindings);
        match &plan.operation {
            Operation::Locales => unreachable!("handled language transaction"),
            Operation::Quit => {}
            Operation::Save(slot, overwrite) => {
                ensure!(
                    *overwrite || !self.saves.slot_path(*slot).exists(),
                    "{}",
                    crate::i18n::format("host-app-custom-0007", &[])
                );
                self.save_current(*slot)?;
            }
            Operation::Load(slot) => {
                self.input_barrier = true;
                let session = self.saves.load_with(*slot, &self.story, &self.logic)?;
                return self.enter_planned(session, true, true, Some(plan));
            }
            Operation::LoadAuto(slot) => {
                self.input_barrier = true;
                let session = self.saves.load_auto(*slot, &self.story, &self.logic)?;
                return self.enter_planned(session, true, true, Some(plan));
            }
            Operation::Restart(target) => {
                self.input_barrier = true;
                let target = target.as_deref().or_else(|| {
                    (self.story.entries.len() == 1).then_some(self.story.entry.as_str())
                });
                let session = self
                    .story
                    .start_at(target.ok_or_else(|| {
                        anyhow::anyhow!("{}", crate::i18n::format("host-app-custom-0008", &[]))
                    })?)
                    .map_err(anyhow::Error::msg)?;
                return self.enter_planned(session, false, true, Some(plan));
            }
            Operation::Advance | Operation::Choose(_) => {
                self.auto_play.set(false);
                let intent = match &plan.operation {
                    Operation::Choose(id) => Intent::Choose(id.clone()),
                    _ => Intent::Advance,
                };
                if self.effects.active() {
                    validate_current()?;
                    if matches!(intent, Intent::Advance) {
                        self.effects.finish();
                    }
                    self.commit_ui(plan, false);
                    return Ok(());
                }
                match presentation::gate(intent, self.typewriter.complete()) {
                    GatedIntent::Reveal => {
                        validate_current()?;
                        self.typewriter.finish();
                    }
                    GatedIntent::Ignore => {
                        validate_current()?;
                    }
                    GatedIntent::Dispatch(intent) => {
                        let result = controller::transition_with(
                            &self.story,
                            &self.session,
                            intent,
                            &self.logic,
                        )?;
                        if !result.entered {
                            validate_current()?;
                            return self.exit_terminal(Some(plan));
                        }
                        if result.entered {
                            return self.enter_planned(result.session, false, false, Some(plan));
                        }
                    }
                }
            }
            Operation::Settings => plan.settings.save(&self.saves.settings_path())?,
            Operation::Keybindings => {
                let file = crate::shortcuts::KeybindingsFile {
                    schema_version: 1,
                    story_id: self.story.id.clone(),
                    bindings: plan.state.keybindings,
                };
                crate::save::atomic_json_limited(
                    &self.saves.directory().join("keybindings.json"),
                    &file,
                    16 * 1024,
                )?;
            }
            Operation::Profile => plan
                .profile
                .save(&self.saves.directory().join("profile.json"))?,
            Operation::Colors => self.write_colors(&plan.state.colors)?,
            Operation::UiAudio => {
                ui_prepared
                    .prefs
                    .save(&self.saves.directory().join("ui-audio.json"))?;
            }
            Operation::Present | Operation::Resume(_) => unreachable!("handled flow operations"),
            Operation::None => {}
        }
        let saved = matches!(plan.operation, Operation::Save(..));
        let quitting = matches!(plan.operation, Operation::Quit);
        if !quitting {
            self.persist_page_plan(&plan)?;
        }
        self.commit_ui(plan, refresh || saved);
        self.apply_ui_media(ui_prepared);
        if rebound {
            self.input_barrier = true;
            self.context.request_repaint();
        }
        if quitting {
            self.quit_requested = true;
            self.context.request_repaint();
        }
        Ok(())
    }
}
