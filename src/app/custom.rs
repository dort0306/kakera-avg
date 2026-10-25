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
                warnings.push(format!("CG 进度无效，已使用初值：{error:#}"));
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
                &Event {
                    name: "game.enter".into(),
                    item: Value::Null,
                    value: Value::Null,
                },
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
        if let Some(enabled) = plan.auto_play {
            self.auto_play.set(enabled);
        }
        if self.custom.state.stack != plan.state.stack {
            self.effects.pages_changed();
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
                let (autosaves, mut images) = self.saves.auto_metadata(&self.story);
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
            custom
                .data
                .lists
                .insert("autosaves".into(), custom.autosaves.clone());
        }
        if refresh_slots {
            self.sync_save_previews();
        }
    }
    pub(super) fn custom_event(&mut self, event: Event) -> Result<()> {
        let custom = &self.custom;
        ensure!(
            event.name != "game.enter",
            "生命周期事件不能作为用户事件触发"
        );
        let commands = self.logic.ui_event(
            &event,
            &self.session,
            &custom.state.variables,
            &custom.data.values,
        )?;
        let plan = host::prepare(
            &custom.definition,
            &custom.state,
            &self.settings,
            &custom.profile,
            commands,
            false,
        )?;
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
                | Operation::Restart
                | Operation::SkipVideo
                | Operation::Advance
                | Operation::Choose(_)
        ) {
            validate_current()?;
        }
        let refresh = plan.state.stack != custom.state.stack
            || plan.state.slot_page != custom.state.slot_page;
        match &plan.operation {
            Operation::SkipVideo => {
                return self.skip_video(Some(plan));
            }
            Operation::Save(slot, overwrite) => {
                ensure!(
                    *overwrite || !self.saves.slot_path(*slot).exists(),
                    "槽位已有存档，请通过确认页显式覆盖"
                );
                self.save_current(*slot)?;
            }
            Operation::Load(slot) => {
                let session = self.saves.load_with(*slot, &self.story, &self.logic)?;
                return self.enter_planned(session, true, true, Some(plan));
            }
            Operation::LoadAuto(slot) => {
                let session = self.saves.load_auto(*slot, &self.story, &self.logic)?;
                return self.enter_planned(session, true, true, Some(plan));
            }
            Operation::Restart => {
                return self.enter_planned(self.story.start(), false, true, Some(plan));
            }
            Operation::Advance | Operation::Choose(_) => {
                self.auto_play.set(false);
                if self.video.playing() {
                    return if matches!(plan.operation, Operation::Advance) {
                        self.skip_video(Some(plan))
                    } else {
                        Ok(())
                    };
                }
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
                        }
                        if result.entered {
                            return self.enter_planned(result.session, false, false, Some(plan));
                        }
                    }
                }
            }
            Operation::Settings => plan.settings.save(&self.saves.settings_path())?,
            Operation::Profile => plan
                .profile
                .save(&self.saves.directory().join("profile.json"))?,
            Operation::None => {}
        }
        let saved = matches!(plan.operation, Operation::Save(..));
        self.commit_ui(plan, refresh || saved);
        Ok(())
    }
}
