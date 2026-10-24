//! Pure, deterministic domain rules. No UI, IO, script runtime or application dependencies.
use super::{
    CompareOp, Condition, Effect, MAX_STRING_BYTES, Route, Story, TransitionRule, VariableDecl,
    VariableValue, Variables,
};

pub fn validate_value(decl: &VariableDecl, value: &VariableValue) -> Result<(), String> {
    match (&decl.initial, value) {
        (VariableValue::Int(_), VariableValue::Int(value)) => {
            if decl.min.is_some_and(|min| *value < min) || decl.max.is_some_and(|max| *value > max)
            {
                return Err(format!("整数值超出声明范围：{value}"));
            }
        }
        (VariableValue::Bool(_), VariableValue::Bool(_)) => {}
        (VariableValue::String(_), VariableValue::String(value))
            if value.len() <= MAX_STRING_BYTES => {}
        _ => return Err("变量类型不匹配或字符串超过 4096 字节".into()),
    }
    Ok(())
}

pub fn validate_variables(story: &Story, variables: &Variables) -> Result<(), String> {
    if variables.len() != story.variables.len() {
        return Err("变量表与故事声明的键不一致".into());
    }
    for (id, decl) in &story.variables {
        let value = variables.get(id).ok_or_else(|| format!("缺少变量：{id}"))?;
        validate_value(decl, value).map_err(|error| format!("变量 {id}：{error}"))?;
    }
    Ok(())
}

pub fn evaluate(condition: &Condition, variables: &Variables) -> Result<bool, String> {
    match condition {
        Condition::Compare {
            variable,
            op,
            value,
        } => {
            let actual = variables
                .get(variable)
                .ok_or_else(|| format!("未知变量：{variable}"))?;
            match (actual, value) {
                (VariableValue::Int(a), VariableValue::Int(b)) => Ok(match op {
                    CompareOp::Eq => a == b,
                    CompareOp::Ne => a != b,
                    CompareOp::Lt => a < b,
                    CompareOp::Le => a <= b,
                    CompareOp::Gt => a > b,
                    CompareOp::Ge => a >= b,
                }),
                (VariableValue::Bool(_), VariableValue::Bool(_))
                | (VariableValue::String(_), VariableValue::String(_)) => match op {
                    CompareOp::Eq => Ok(actual == value),
                    CompareOp::Ne => Ok(actual != value),
                    _ => Err(format!("变量 {variable} 不支持排序比较")),
                },
                _ => Err(format!("变量 {variable} 的比较类型不匹配")),
            }
        }
        Condition::All(conditions) => {
            for condition in conditions {
                if !evaluate(condition, variables)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Condition::Any(conditions) => {
            for condition in conditions {
                if evaluate(condition, variables)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Condition::Not(condition) => Ok(!evaluate(condition, variables)?),
    }
}

pub fn matches(condition: Option<&Condition>, variables: &Variables) -> Result<bool, String> {
    condition
        .map(|condition| evaluate(condition, variables))
        .unwrap_or(Ok(true))
}

pub fn apply_effects(
    story: &Story,
    variables: &Variables,
    effects: &[Effect],
) -> Result<Variables, String> {
    validate_variables(story, variables)?;
    let mut updated = variables.clone();
    for effect in effects {
        let (id, value) = match effect {
            Effect::Set { variable, value } => (variable, value.clone()),
            Effect::Add { variable, amount } => {
                let Some(VariableValue::Int(value)) = updated.get(variable) else {
                    return Err(format!("add 需要整数变量：{variable}"));
                };
                let value = value
                    .checked_add(*amount)
                    .ok_or_else(|| format!("整数溢出：{variable}"))?;
                (variable, VariableValue::Int(value))
            }
        };
        let decl = story
            .variables
            .get(id)
            .ok_or_else(|| format!("未知变量：{id}"))?;
        validate_value(decl, &value).map_err(|error| format!("变量 {id}：{error}"))?;
        updated.insert(id.clone(), value);
    }
    Ok(updated)
}

pub fn resolve<'a>(
    story: &'a Story,
    frame_id: &str,
    choice: Option<&str>,
    variables: &Variables,
) -> Result<Option<&'a TransitionRule>, String> {
    let route = story
        .routes
        .get(frame_id)
        .ok_or_else(|| format!("场景缺少流转：{frame_id}"))?;
    match (route, choice) {
        (Route::Script(_), _) => Err("脚本路由需要注入逻辑执行器".into()),
        (Route::Next(rule), None) => Ok(Some(rule)),
        (Route::Branch { cases, otherwise }, None) => {
            for rule in cases {
                if matches(rule.when.as_ref(), variables)? {
                    return Ok(Some(rule));
                }
            }
            Ok(Some(otherwise))
        }
        (Route::Choice(options), Some(id)) => {
            let rule = options.get(id).ok_or_else(|| format!("无效选项：{id}"))?;
            if !matches(rule.when.as_ref(), variables)? {
                return Err(format!("选项条件不满足：{id}"));
            }
            Ok(Some(rule))
        }
        (Route::Choice(_), None) | (Route::End, None) => Ok(None),
        _ => Err("当前场景不能选择选项".into()),
    }
}
