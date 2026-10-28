use crate::model::{HistoryEntry, Logic, MAX_HISTORY, Session, Story, rules};
use std::fmt;

#[derive(Clone, Debug)]
pub enum Intent {
    Advance,
    Choose(String),
    Resume(String),
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

pub fn transition_with(
    story: &Story,
    session: &Session,
    intent: Intent,
    logic: &dyn Logic,
) -> Result<Transition, ControlError> {
    if story.frame(session).is_none() {
        return Err(ControlError(format!(
            "当前场景不存在：{}",
            session.frame_id
        )));
    }
    story
        .validate_session_with(session, logic)
        .map_err(ControlError)?;
    let choice_id = match &intent {
        Intent::Choose(id) => Some(id.clone()),
        _ => None,
    };
    let rule = if let Intent::Resume(target) = &intent {
        if !story.frames.contains_key(target) {
            return Err(ControlError("UI 返回目标不存在".into()));
        }
        Some(crate::model::TransitionRule::plain(target.clone()))
    } else {
        logic
            .resolve(
                story,
                &session.frame_id,
                choice_id.as_deref(),
                &session.variables,
            )
            .map_err(ControlError)?
    };
    let Some(rule) = rule else {
        return Ok(Transition {
            session: session.clone(),
            entered: false,
        });
    };
    let target = &rule.to;
    if !story.frames.contains_key(target) {
        return Err(ControlError(format!("目标场景不存在：{target}")));
    }
    let mut next = session.clone();
    next.variables =
        rules::apply_effects(story, &session.variables, &rule.effects).map_err(ControlError)?;
    let last = next
        .history
        .last_mut()
        .ok_or_else(|| ControlError("会话历史为空".into()))?;
    if last.frame_id != session.frame_id || (last.choice_id.is_some() || last.resume_to.is_some()) {
        return Err(ControlError("会话历史末尾与当前场景不一致".into()));
    }
    last.choice_id = choice_id;
    last.resume_to = if matches!(intent, Intent::Resume(_)) {
        Some(target.clone())
    } else {
        None
    };
    next.frame_id = target.clone();
    next.history.push(HistoryEntry {
        resume_to: None,
        frame_id: target.clone(),
        choice_id: None,
    });
    if next.history.len() > MAX_HISTORY {
        let first = &next.history[0];
        let dropped = if let Some(target) = &first.resume_to {
            crate::model::TransitionRule::plain(target.clone())
        } else {
            logic
                .resolve(
                    story,
                    &first.frame_id,
                    first.choice_id.as_deref(),
                    &next.history_base_variables,
                )
                .map_err(ControlError)?
                .ok_or_else(|| ControlError("历史检查点缺少有效流转".into()))?
        };
        next.history_base_variables =
            rules::apply_effects(story, &next.history_base_variables, &dropped.effects)
                .map_err(ControlError)?;
        next.history.remove(0);
    }
    logic.options(story, &next).map_err(ControlError)?;
    Ok(Transition {
        session: next,
        entered: true,
    })
}
