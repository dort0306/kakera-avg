use crate::model::{Route, Session, Story};
use std::fmt;

#[derive(Clone, Debug)]
pub enum Intent {
    Advance,
    Choose(String),
}

#[derive(Debug, PartialEq, Eq)]
pub struct ControlError(pub String);

impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ControlError {}

pub fn reduce(story: &Story, session: &Session, intent: Intent) -> Result<Session, ControlError> {
    if story.frame(session).is_none() {
        return Err(ControlError(format!(
            "当前场景不存在：{}",
            session.frame_id
        )));
    }
    let route = story
        .routes
        .get(&session.frame_id)
        .ok_or_else(|| ControlError(format!("场景缺少流转：{}", session.frame_id)))?;
    let target = match (route, intent) {
        (Route::Next(target), Intent::Advance) => target,
        (Route::Choice(options), Intent::Choose(id)) => options
            .get(&id)
            .ok_or_else(|| ControlError(format!("无效选项：{id}")))?,
        (Route::Choice(_), Intent::Advance) | (Route::End, Intent::Advance) => {
            return Ok(session.clone());
        }
        _ => return Err(ControlError("当前场景不能选择选项".into())),
    };
    if !story.frames.contains_key(target) {
        return Err(ControlError(format!("目标场景不存在：{target}")));
    }
    Ok(Session {
        frame_id: target.clone(),
    })
}
