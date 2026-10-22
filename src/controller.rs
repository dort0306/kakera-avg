use crate::model::{HistoryEntry, MAX_HISTORY, Route, Session, Story};
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

pub struct Transition {
    pub session: Session,
    pub entered: bool,
}

pub fn reduce(story: &Story, session: &Session, intent: Intent) -> Result<Session, ControlError> {
    transition(story, session, intent).map(|result| result.session)
}

pub fn transition(
    story: &Story,
    session: &Session,
    intent: Intent,
) -> Result<Transition, ControlError> {
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
    let choice_id = match &intent {
        Intent::Choose(id) => Some(id.clone()),
        _ => None,
    };
    let target = match (route, intent) {
        (Route::Next(target), Intent::Advance) => target,
        (Route::Choice(options), Intent::Choose(id)) => options
            .get(&id)
            .ok_or_else(|| ControlError(format!("无效选项：{id}")))?,
        (Route::Choice(_), Intent::Advance) | (Route::End, Intent::Advance) => {
            return Ok(Transition {
                session: session.clone(),
                entered: false,
            });
        }
        _ => return Err(ControlError("当前场景不能选择选项".into())),
    };
    if !story.frames.contains_key(target) {
        return Err(ControlError(format!("目标场景不存在：{target}")));
    }
    let mut next = session.clone();
    let last = next
        .history
        .last_mut()
        .ok_or_else(|| ControlError("会话历史为空".into()))?;
    if last.frame_id != session.frame_id || last.choice_id.is_some() {
        return Err(ControlError("会话历史末尾与当前场景不一致".into()));
    }
    last.choice_id = choice_id;
    next.frame_id = target.clone();
    next.history.push(HistoryEntry {
        frame_id: target.clone(),
        choice_id: None,
    });
    if next.history.len() > MAX_HISTORY {
        next.history.remove(0);
    }
    Ok(Transition {
        session: next,
        entered: true,
    })
}
