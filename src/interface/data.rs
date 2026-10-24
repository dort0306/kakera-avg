use super::{
    Data, Definition,
    host::{Profile, State},
};
use crate::{
    model::{Route, ScriptRoute, Session, Story, VariableValue, Variables},
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
            values: json!({"game":{"frameId":frame.id,"speaker":speaker,"text":frame.text},"settings":settings,"ui":values(&state.variables)}),
            lists: Default::default(),
        };
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
        data.lists.insert("slots".into(),slots.iter().map(|info|{
            let (preview,exists,loadable)=match &info.state{SlotState::Empty=>("空槽".into(),false,false),SlotState::Ready{frame_id,preview,..}=>(format!("{frame_id} · {preview}"),true,true),SlotState::Invalid(error)=>(format!("损坏或不兼容：{error}"),true,false)};
            json!({"slot":info.slot.number(),"title":format!("槽位 {}",info.slot.number()),"preview":preview,"exists":exists,"loadable":loadable})
        }).collect());
        data.lists.insert("gallery".into(),definition.gallery.iter().map(|(id,cg)|json!({"id":id,"title":cg.title,"image":cg.image,"unlocked":profile.unlocked.contains(id)})).collect());
        data.lists.insert("variables".into(),story.variables.iter().map(|(id,decl)|json!({"id":id,"label":decl.label,"value":session.variables[id].to_string()})).collect());
        data.lists.insert("history".into(),session.history.iter().enumerate().map(|(index,entry)|{
            let frame=&story.frames[&entry.frame_id];let speaker=frame.speaker.as_ref().and_then(|id|story.characters.get(id)).map(String::as_str).unwrap_or("旁白");
            let choice=entry.choice_id.as_ref().and_then(|id|frame.options.iter().find(|o|&o.id==id)).map(|o|format!("选择：{}",o.text)).unwrap_or_default();
            json!({"id":format!("{} · {}",index+1,frame.id),"speaker":speaker,"text":frame.text,"choice":choice})
        }).collect());
        data
    }
    pub fn progress(&mut self, complete: bool, route: &Route, empty: bool) {
        self.values["game"]["transitioning"] = json!(false);
        self.values["game"]["complete"] = json!(complete);
        self.values["game"]["canAdvance"] = json!(
            !complete
                || matches!(
                    route,
                    Route::Next(_) | Route::Branch { .. } | Route::Script(ScriptRoute::Advance)
                )
        );
        self.values["game"]["ended"] =
            json!(complete && matches!(route, Route::End | Route::Script(ScriptRoute::End)));
        self.values["game"]["noChoices"] = json!(
            complete
                && empty
                && matches!(route, Route::Choice(_) | Route::Script(ScriptRoute::Choice))
        );
        self.values["game"]["continueText"] = json!(if complete { "继续" } else { "补全" });
    }
}
