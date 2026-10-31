use super::{
    Data, Definition, Kind,
    host::{Profile, State},
    pagination,
};
use crate::{
    model::{Route, Session, Story, VariableValue, Variables},
    save::{SlotInfo, SlotState},
    settings::Settings,
};
use serde_json::{Map, Value, json};

pub struct Input<'a> {
    pub definition: &'a Definition,
    pub story: &'a Story,
    pub session: &'a Session,
    pub available: &'a [String],
    pub state: &'a State,
    pub profile: &'a Profile,
    pub slots: &'a [SlotInfo],
    pub settings: &'a Settings,
    pub complete: bool,
    /// Rows already loaded for each explicitly paginated `slots` repeat.
    pub slot_pages: &'a [(String, Vec<SlotInfo>)],
    /// Auto-save metadata (already ordered and localized by the host).
    pub autosaves: &'a [Value],
}
pub fn values(vars: &Variables) -> Value {
    Value::Object(
        vars.iter()
            .map(|(id, value)| {
                (
                    id.clone(),
                    match value {
                        VariableValue::Int(v) => json!(v),
                        VariableValue::Bool(v) => json!(v),
                        VariableValue::String(v) => json!(v),
                    },
                )
            })
            .collect(),
    )
}
fn display_value(value: &VariableValue) -> String {
    match value {
        VariableValue::Bool(value) => crate::i18n::text(if *value {
            "host-value-true"
        } else {
            "host-value-false"
        })
        .into(),
        value => value.to_string(),
    }
}
fn slot_row(
    story: &Story,
    info: &SlotInfo,
    catalog: &crate::localization::Catalog,
    locale: Option<&str>,
) -> Value {
    // Slot metadata never exposes internal frame ids; previews are text only.
    let (preview, exists, loadable, saved_at, has_image) = match &info.state {
        SlotState::Empty => (
            crate::i18n::text("host-interface-data-0007").into(),
            false,
            false,
            String::new(),
            false,
        ),
        SlotState::Ready {
            preview,
            saved_at_unix_ms,
            screenshot,
            frame_id,
        } => {
            let text = story
                .frames
                .get(frame_id)
                .map(|frame| catalog.frame_text(frame, locale))
                .unwrap_or_else(|| preview.clone());
            (
                text,
                true,
                true,
                crate::save_preview::saved_at(*saved_at_unix_ms),
                screenshot.is_some(),
            )
        }
        SlotState::Invalid(error) => (
            crate::i18n::format("host-interface-data-0003", &[("arg0", error.to_string())]),
            true,
            false,
            String::new(),
            false,
        ),
    };
    json!({"slot":info.slot.number(),"title":crate::i18n::format("host-interface-data-0004",&[("arg0",format!("{}",info.slot.number()))]),"excerpt":crate::save_preview::excerpt(&preview),"preview":preview,"exists":exists,"loadable":loadable,"savedAt":saved_at,"hasScreenshot":has_image,"image":if has_image {crate::save_preview::image_id(info.slot.number(),false)} else {String::new()}})
}
impl Data {
    /// Hide the internal validation checkpoint until a story has actually been entered.
    pub fn awaiting_start(&mut self, definition: &Definition) {
        self.values["game"]["frameId"] = json!("");
        self.values["game"]["cgId"] = json!("");
        self.values["game"]["speaker"] = json!("");
        self.values["game"]["text"] = json!("");
        self.values["game"]["canAdvance"] = json!(false);
        self.values["game"]["complete"] = json!(false);
        self.values["game"]["ended"] = json!(false);
        self.values["game"]["continueText"] = json!(crate::i18n::text("host-interface-data-0006"));
        self.lists.insert("choices".into(), Vec::new());
        self.lists.insert("history".into(), Vec::new());
        // Paginated copies of the story-derived sources are hidden with their originals.
        pagination::for_each(definition, |node| {
            if let Kind::Repeat {
                source,
                pagination: Some(_),
                ..
            } = &node.kind
                && matches!(source.as_str(), "choices" | "history")
            {
                self.lists
                    .insert(pagination::list_key(&node.id), Vec::new());
                if let Some(meta) = self
                    .values
                    .get_mut("repeat")
                    .and_then(|repeat| repeat.get_mut(&node.id))
                {
                    meta["page"] = json!(1);
                    meta["pages"] = json!(1);
                    meta["total"] = json!(0);
                    meta["start"] = json!(0);
                    meta["end"] = json!(0);
                    meta["canPrev"] = json!(false);
                    meta["canNext"] = json!(false);
                    meta["pageText"] = json!(crate::i18n::format(
                        "host-interface-pagination-0001",
                        &[("arg0", "1".to_string()), ("arg1", "1".to_string())]
                    ));
                }
            }
        });
    }
    pub fn build(input: Input<'_>) -> Self {
        let Input {
            definition,
            story,
            session,
            available,
            state,
            profile,
            slots,
            settings,
            complete,
            slot_pages,
            autosaves,
        } = input;
        let frame = &story.frames[&session.frame_id];
        let speaker = frame
            .speaker
            .as_ref()
            .and_then(|id| story.characters.get(id))
            .map(String::as_str)
            .unwrap_or("");
        let totals = pagination::Totals {
            choices: available.len(),
            history: session.history.len(),
            gallery: definition.gallery.len(),
            variables: story.variables.len(),
            text_locales: definition.localization.texts.len(),
            voice_locales: definition.localization.voices.len(),
            autosaves: autosaves.len(),
            slots: crate::save::SLOT_COUNT as usize,
        };
        let (plans, normalized) = pagination::plans(definition, &state.variables, &totals);
        let mut variables = state.variables.clone();
        for (name, value) in &normalized {
            variables.insert(name.clone(), VariableValue::Int(*value));
        }
        let mut data = Self {
            values: json!({"game":{"frameId":frame.reference(),"speaker":speaker,"text":frame.text},"settings":settings,"ui":values(&variables)}),
            lists: Default::default(),
            pages: normalized,
            page_limits: plans
                .iter()
                .map(|plan| (plan.variable.clone(), plan.pages))
                .collect(),
        };
        data.values["game"]["cgId"] = json!(
            frame
                .visual
                .background
                .as_ref()
                .filter(|id| story.assets[*id].kind == crate::model::AssetKind::Cg)
                .cloned()
                .unwrap_or_default()
        );
        let total = definition.gallery.len();
        let unlocked = definition
            .gallery
            .keys()
            .filter(|id| profile.unlocked.contains(*id))
            .count();
        data.values["cg"] = json!({"total":total,"unlocked":unlocked,"progress":crate::i18n::format("host-interface-data-0001",&[("arg0",format!("{}",unlocked)),("arg1",format!("{}",total))])});
        let keybindings = &state.keybindings;
        data.values["shortcuts"] = json!({
            "bindings": {
                "advance": keybindings.advance.map(|binding| binding.canonical()),
                "fast_forward": keybindings.fast_forward.map(|binding| binding.canonical()),
                "skip_sequence": keybindings.skip_sequence.map(|binding| binding.canonical()),
            },
            "advanceLabel": keybindings.display(crate::shortcuts::Action::Advance),
            "fastForwardLabel": keybindings.display(crate::shortcuts::Action::FastForward),
            "skipSequenceLabel": keybindings.display(crate::shortcuts::Action::SkipSequence),
            "captureActive": false,
            "captureText": "",
        });
        data.values["colors"] = json!(
            state
                .resolved_colors
                .iter()
                .map(|(id, color)| (id.clone(), color.format()))
                .collect::<std::collections::BTreeMap<_, _>>()
        );
        data.values["saves"] = json!({"page":state.slot_page,"pages":crate::save::PAGE_COUNT,
            "pageText":crate::i18n::format("host-interface-data-0002",&[("arg0",format!("{}",state.slot_page)),("arg1",format!("{}",crate::save::PAGE_COUNT))]),
            "canPrev":state.slot_page>1,"canNext":state.slot_page<crate::save::PAGE_COUNT});
        data.progress(complete, &story.routes[&frame.id], available.is_empty());
        data.lists.insert(
            "choices".into(),
            frame
                .options
                .iter()
                .filter(|option| available.contains(&option.id))
                .map(|option| json!({"id":option.id,"text":option.text}))
                .collect(),
        );
        let locale = state.languages.text.as_deref();
        data.lists.insert(
            "slots".into(),
            slots
                .iter()
                .map(|info| slot_row(story, info, &definition.localization, locale))
                .collect(),
        );
        data.lists.insert("autosaves".into(), autosaves.to_vec());
        data.lists.insert("gallery".into(),definition.gallery.iter().map(|(id,cg)|json!({"id":id,"title":cg.title,"image":cg.image,"unlocked":profile.unlocked.contains(id)})).collect());
        data.lists.insert("variables".into(),story.variables.iter().map(|(id,decl)|json!({"id":id,"label":decl.label,"value":display_value(&session.variables[id])})).collect());
        data.lists.insert("history".into(),session.history.iter().enumerate().map(|(index,entry)|{
            let frame=&story.frames[&entry.frame_id];let speaker=frame.speaker.as_ref().and_then(|id|story.characters.get(id)).map(String::as_str).unwrap_or(crate::i18n::text("host-interface-data-0008"));
            let choice=entry.choice_id.as_ref().and_then(|id|frame.options.iter().find(|o|&o.id==id)).map(|o|crate::i18n::format_literal("host-interface-data-0005",&[("arg0",o.text.to_string())])).unwrap_or_default();
            json!({"id":format!("{} · {}",index+1,frame.reference()),"speaker":speaker,"text":frame.text,"choice":choice})
        }).collect());
        crate::localization::project_data(&mut data, definition, state, story, session, slots);
        data.apply_pagination(definition, story, &plans, slot_pages);
        data
    }
    fn apply_pagination(
        &mut self,
        definition: &Definition,
        story: &Story,
        plans: &[pagination::Plan],
        slot_pages: &[(String, Vec<SlotInfo>)],
    ) {
        let mut metadata = Map::new();
        for plan in plans {
            let rows: Vec<Value> = if plan.source == "slots" {
                let locale = self.values["locale"]["text"].as_str().map(str::to_owned);
                slot_pages
                    .iter()
                    .find(|(id, _)| id == &plan.repeat)
                    .map(|(_, infos)| {
                        infos
                            .iter()
                            .map(|info| {
                                slot_row(story, info, &definition.localization, locale.as_deref())
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                let source = self.lists.get(&plan.source).cloned().unwrap_or_default();
                match plan.range() {
                    Some((start, end)) => source
                        .iter()
                        .skip(start.saturating_sub(1))
                        .take(end.saturating_sub(start).saturating_add(1))
                        .cloned()
                        .collect(),
                    None => Vec::new(),
                }
            };
            self.lists.insert(pagination::list_key(&plan.repeat), rows);
            metadata.insert(
                plan.repeat.clone(),
                json!({
                    "page": plan.page,
                    "pages": plan.pages,
                    "pageSize": plan.page_size,
                    "total": plan.total,
                    "start": plan.start,
                    "end": plan.end,
                    "canPrev": plan.can_prev(),
                    "canNext": plan.can_next(),
                    "pageText": plan.page_text()
                }),
            );
        }
        self.values["repeat"] = Value::Object(metadata);
    }
    pub fn progress(&mut self, complete: bool, route: &Route, empty: bool) {
        self.values["game"]["transitioning"] = json!(false);
        self.values["game"]["complete"] = json!(complete);
        self.values["game"]["canAdvance"] = json!(!complete || matches!(route, Route::Advance));
        self.values["game"]["ended"] = json!(complete && matches!(route, Route::End));
        self.values["game"]["noChoices"] =
            json!(complete && empty && matches!(route, Route::Choice));
        self.values["game"]["continueText"] = json!(if complete {
            crate::i18n::text("host-interface-data-0009")
        } else {
            crate::i18n::text("host-interface-data-0010")
        });
    }
}
