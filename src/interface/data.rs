use super::{
    Data, Definition,
    host::{Profile, State},
};
use crate::{
    model::{Route, Session, Story, VariableValue, Variables},
    save::{SlotInfo, SlotState},
    settings::Settings,
};
use serde_json::{Value, json};

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
impl Data {
    /// Hide the internal validation checkpoint until a story has actually been entered.
    pub fn awaiting_start(&mut self) {
        self.values["game"]["frameId"] = json!("");
        self.values["game"]["cgId"] = json!("");
        self.values["game"]["speaker"] = json!("");
        self.values["game"]["text"] = json!("");
        self.values["game"]["canAdvance"] = json!(false);
        self.values["game"]["complete"] = json!(false);
        self.values["game"]["ended"] = json!(false);
        self.values["game"]["continueText"] = json!("请选择故事起点");
        self.lists.insert("choices".into(), Vec::new());
        self.lists.insert("history".into(), Vec::new());
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
        } = input;
        let frame = &story.frames[&session.frame_id];
        let speaker = frame
            .speaker
            .as_ref()
            .and_then(|id| story.characters.get(id))
            .map(String::as_str)
            .unwrap_or("");
        let mut data = Self {
            values: json!({"game":{"frameId":frame.reference(),"speaker":speaker,"text":frame.text},"settings":settings,"ui":values(&state.variables)}),
            lists: Default::default(),
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
        data.values["cg"] = json!({"total":total,"unlocked":unlocked,"progress":format!("已解锁 {unlocked} / {total}")});
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
            "pageText":format!("第 {} / {} 页 · 200 手动槽",state.slot_page,crate::save::PAGE_COUNT),
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
        data.lists.insert(
            "slots".into(),
            slots
                .iter()
                .map(|info| {
                    // Slot metadata never exposes internal frame ids; previews are text only.
                    let (preview, exists, loadable, saved_at, has_image) = match &info.state {
                        SlotState::Empty => ("空槽".into(), false, false, String::new(), false),
                        SlotState::Ready {
                            preview,
                            saved_at_unix_ms,
                            screenshot,
                            ..
                        } => (
                            preview.clone(),
                            true,
                            true,
                            crate::save_preview::saved_at(*saved_at_unix_ms),
                            screenshot.is_some(),
                        ),
                        SlotState::Invalid(error) => (
                            format!("损坏或不兼容：{error}"),
                            true,
                            false,
                            String::new(),
                            false,
                        ),
                    };
                    json!({"slot":info.slot.number(),"title":format!("槽位 {}",info.slot.number()),"excerpt":crate::save_preview::excerpt(&preview),"preview":preview,"exists":exists,"loadable":loadable,"savedAt":saved_at,"hasScreenshot":has_image,"image":if has_image {crate::save_preview::image_id(info.slot.number(),false)} else {String::new()}})
                })
                .collect(),
        );
        data.lists.insert("gallery".into(),definition.gallery.iter().map(|(id,cg)|json!({"id":id,"title":cg.title,"image":cg.image,"unlocked":profile.unlocked.contains(id)})).collect());
        data.lists.insert("variables".into(),story.variables.iter().map(|(id,decl)|json!({"id":id,"label":decl.label,"value":session.variables[id].to_string()})).collect());
        data.lists.insert("history".into(),session.history.iter().enumerate().map(|(index,entry)|{
            let frame=&story.frames[&entry.frame_id];let speaker=frame.speaker.as_ref().and_then(|id|story.characters.get(id)).map(String::as_str).unwrap_or("旁白");
            let choice=entry.choice_id.as_ref().and_then(|id|frame.options.iter().find(|o|&o.id==id)).map(|o|format!("选择：{}",o.text)).unwrap_or_default();
            json!({"id":format!("{} · {}",index+1,frame.reference()),"speaker":speaker,"text":frame.text,"choice":choice})
        }).collect());
        data
    }
    pub fn progress(&mut self, complete: bool, route: &Route, empty: bool) {
        self.values["game"]["transitioning"] = json!(false);
        self.values["game"]["complete"] = json!(complete);
        self.values["game"]["canAdvance"] = json!(!complete || matches!(route, Route::Advance));
        self.values["game"]["ended"] = json!(complete && matches!(route, Route::End));
        self.values["game"]["noChoices"] =
            json!(complete && empty && matches!(route, Route::Choice));
        self.values["game"]["continueText"] = json!(if complete { "继续" } else { "补全" });
    }
}
