//! Pure, deterministic domain rules. No UI, IO, script runtime or application dependencies.
use super::{Effect, MAX_STRING_BYTES, Story, VariableDecl, VariableValue, Variables};

pub fn validate_value(
    decl: &VariableDecl,
    value: &VariableValue,
) -> Result<(), crate::model::Diagnostic> {
    match (&decl.initial, value) {
        (VariableValue::Int(_), VariableValue::Int(value)) => {
            if decl.min.is_some_and(|min| *value < min) || decl.max.is_some_and(|max| *value > max)
            {
                return Err(crate::model::Diagnostic::new(
                    "host-model-rules-0001",
                    &[("arg0", value.to_string())],
                ));
            }
        }
        (VariableValue::Bool(_), VariableValue::Bool(_)) => {}
        (VariableValue::String(_), VariableValue::String(value))
            if value.len() <= MAX_STRING_BYTES => {}
        _ => return Err(crate::model::Diagnostic::new("host-model-rules-0008", &[])),
    }
    Ok(())
}

pub fn validate_variables(
    story: &Story,
    variables: &Variables,
) -> Result<(), crate::model::Diagnostic> {
    if variables.len() != story.variables.len() {
        return Err(crate::model::Diagnostic::new("host-model-rules-0009", &[]));
    }
    for (id, decl) in &story.variables {
        let value = variables.get(id).ok_or_else(|| {
            crate::model::Diagnostic::new("host-model-rules-0002", &[("arg0", id.to_string())])
        })?;
        validate_value(decl, value).map_err(|error| {
            crate::model::Diagnostic::new(
                "host-model-rules-0003",
                &[("arg0", id.to_string()), ("arg1", error.to_string())],
            )
        })?;
    }
    Ok(())
}

pub fn apply_effects(
    story: &Story,
    variables: &Variables,
    effects: &[Effect],
) -> Result<Variables, crate::model::Diagnostic> {
    validate_variables(story, variables)?;
    let mut updated = variables.clone();
    for effect in effects {
        let (id, value) = match effect {
            Effect::Set { variable, value } => (variable, value.clone()),
            Effect::Add { variable, amount } => {
                let Some(VariableValue::Int(value)) = updated.get(variable) else {
                    return Err(crate::model::Diagnostic::new(
                        "host-model-rules-0004",
                        &[("arg0", variable.to_string())],
                    ));
                };
                let value = value.checked_add(*amount).ok_or_else(|| {
                    crate::model::Diagnostic::new(
                        "host-model-rules-0005",
                        &[("arg0", variable.to_string())],
                    )
                })?;
                (variable, VariableValue::Int(value))
            }
        };
        let decl = story.variables.get(id).ok_or_else(|| {
            crate::model::Diagnostic::new("host-model-rules-0006", &[("arg0", id.to_string())])
        })?;
        validate_value(decl, &value).map_err(|error| {
            crate::model::Diagnostic::new(
                "host-model-rules-0007",
                &[("arg0", id.to_string()), ("arg1", error.to_string())],
            )
        })?;
        updated.insert(id.clone(), value);
    }
    Ok(updated)
}
