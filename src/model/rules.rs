//! Pure, deterministic domain rules. No UI, IO, script runtime or application dependencies.
use super::{Effect, MAX_STRING_BYTES, Story, VariableDecl, VariableValue, Variables};

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
