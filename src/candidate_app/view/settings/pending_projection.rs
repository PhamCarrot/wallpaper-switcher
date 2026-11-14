//! Settings-only projection of inert pending source data. No execution consumer.
use super::composed::draft::Value;
use crate::candidate_state::{
    frontend::{PendingOnly, PendingSetting},
    pending, CandidateState,
};

fn json(value: &Value) -> serde_json::Value {
    match value {
        Value::Boolean(value) => (*value).into(),
        Value::Number(value) => (*value).into(),
        Value::Text(value) => value.clone().into(),
    }
}
fn draft(value: serde_json::Value) -> Option<Value> {
    match value {
        serde_json::Value::Bool(value) => Some(Value::Boolean(value)),
        serde_json::Value::Number(value) => value.as_f64().map(Value::Number),
        serde_json::Value::String(value) => Some(Value::Text(value)),
        _ => None,
    }
}
pub(in crate::candidate_app) fn read(state: &CandidateState, path: &str) -> Option<Value> {
    let spec = pending::spec(path)?;
    let record = state
        .pending_frontend
        .settings
        .iter()
        .find(|entry| entry.source_path == path && entry.capability == spec.capability)?;
    let value: serde_json::Value = serde_json::from_str(&record.value_json).ok()?;
    let accepted = pending::normalize(spec.capability, path, value.clone()).ok()?;
    // Do not silently promote incompatible loaded values through normalizers.
    if accepted != value {
        return None;
    }
    draft(value)
}

pub(in crate::candidate_app) fn write(
    state: &mut CandidateState,
    path: &str,
    value: &Value,
) -> Result<bool, String> {
    let Some(spec) = pending::spec(path) else {
        return Ok(false);
    };
    let value = pending::normalize(spec.capability, path, json(value))?;
    put(state, spec.capability, path, value)?;
    Ok(true)
}

pub(in crate::candidate_app) fn put(
    state: &mut CandidateState,
    capability: crate::candidate_state::frontend::DeferredCapability,
    path: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    let value = pending::accepted(capability, path, value)?;
    let value_json =
        serde_json::to_string(&value).map_err(|_| "Could not encode pending value.")?;
    if let Some(entry) = state
        .pending_frontend
        .settings
        .iter_mut()
        .find(|entry| entry.capability == capability && entry.source_path == path)
    {
        entry.value_json = value_json;
    } else {
        state.pending_frontend.settings.push(PendingSetting {
            capability,
            source_path: path.into(),
            value_json,
            status: PendingOnly::Pending,
        });
    }
    Ok(())
}

pub(in crate::candidate_app) fn root(
    state: &CandidateState,
    path: &str,
) -> Option<serde_json::Value> {
    checked_root(state, path).ok().flatten()
}

pub(in crate::candidate_app) fn checked_root(
    state: &CandidateState,
    path: &str,
) -> Result<Option<serde_json::Value>, String> {
    let owner = pending::root_owner(path).ok_or("Unknown pending root owner.")?;
    let Some(entry) = state
        .pending_frontend
        .settings
        .iter()
        .find(|entry| entry.capability == owner && entry.source_path == path)
    else {
        return Ok(None);
    };
    let value = serde_json::from_str(&entry.value_json)
        .map_err(|_| "Stored pending data is invalid; its original record was retained.")?;
    pending::normalize_root(owner, path, value)
        .map(Some)
        .map_err(|_| {
            "Stored pending data is incompatible; its original record was retained.".into()
        })
}

pub(in crate::candidate_app) fn target_names(
    state: &CandidateState,
) -> Result<Vec<String>, String> {
    let Some(value) = checked_root(state, "theme.targets")? else {
        return Ok(Vec::new());
    };
    serde_json::from_value(value)
        .map_err(|_| "Stored Theme targets were retained without editing.".into())
}

pub(in crate::candidate_app) fn scope(state: &CandidateState, path: &str) -> Option<Value> {
    let shader = path.strip_prefix("transition.shaderScopes.")?;
    if !super::composed::SHADERS.contains(&shader) {
        return None;
    }
    let scopes = root(state, "transition.shaderScopes")?;
    let value = scopes.get(shader)?.as_str()?;
    ["all", "primary"]
        .contains(&value)
        .then(|| Value::Text(value.into()))
}
pub(in crate::candidate_app) fn background(state: &CandidateState, path: &str) -> Option<Value> {
    for name in ["display.backgroundModes", "display.backgroundColors"] {
        if let Some(output) = path
            .strip_prefix(name)
            .and_then(|tail| tail.strip_prefix('.'))
        {
            return root(state, name)?
                .get(output)?
                .as_str()
                .map(|s| Value::Text(s.into()));
        }
    }
    None
}

pub(in crate::candidate_app) const COLLECTION_ROOTS: [&str; 3] =
    ["postProcessing", "integrations", "semantic.models"];

pub(in crate::candidate_app) fn collection_owner(path: &str) -> Option<&'static str> {
    COLLECTION_ROOTS.into_iter().find(|root| {
        path == *root
            || path
                .strip_prefix(root)
                .is_some_and(|tail| tail.starts_with('.'))
    })
}

pub(in crate::candidate_app) fn collection_editable(
    state: &CandidateState,
    root: &str,
) -> Result<(), String> {
    let owner = pending::root_owner(root).ok_or("Unknown collection owner.")?;
    if let Some(entry) = state
        .pending_frontend
        .settings
        .iter()
        .find(|entry| entry.source_path == root && entry.capability == owner)
    {
        let value = serde_json::from_str(&entry.value_json)
            .map_err(|_| "Stored pending collection is invalid; its original data was retained.")?;
        pending::normalize_root(owner, root, value).map_err(|_| "Stored pending collection is unsupported; its original data was retained. Use a supported record before editing this collection.")?;
    }
    Ok(())
}

pub(in crate::candidate_app) fn hydrate_collections(
    state: &CandidateState,
    lists: &mut super::composed::collections::Collections,
) {
    if let Some(value) = root(state, "postProcessing") {
        if let Ok(entries) = serde_json::from_value(value) {
            lists.commands = entries;
        }
    }
    if let Some(value) = root(state, "integrations") {
        if let Ok(entries) = serde_json::from_value(value) {
            lists.integrations = entries;
        }
    }
    if let Some(value) = root(state, "semantic.models") {
        if let Ok(entries) = serde_json::from_value(value) {
            lists.semantic_models = entries;
        }
    }
}

pub(in crate::candidate_app) fn write_collection(
    state: &mut CandidateState,
    lists: &super::composed::collections::Collections,
    root: &str,
) -> Result<(), String> {
    collection_editable(state, root)?;
    let value = match root {
        "postProcessing" => serde_json::to_value(&lists.commands),
        "integrations" => serde_json::to_value(&lists.integrations),
        "semantic.models" => serde_json::to_value(&lists.semantic_models),
        _ => return Err("Unknown collection owner.".into()),
    }
    .map_err(|_| "Could not encode pending collection.")?;
    put(
        state,
        pending::root_owner(root).ok_or("Unknown collection owner.")?,
        root,
        value,
    )
}

pub(in crate::candidate_app) fn collection_value(
    lists: &super::composed::collections::Collections,
    path: &str,
) -> Option<Value> {
    let tail = path.strip_prefix("integrations.")?;
    let (index, field) = tail.split_once('.')?;
    let entry = lists.integrations.get(index.parse::<usize>().ok()?)?;
    Some(match field {
        "enabled" | "livePreview" => Value::Boolean(
            (if field == "enabled" {
                entry.enabled
            } else {
                entry.live_preview
            })
            .unwrap_or_else(|| super::composed::schema::boolean_default(path).unwrap_or(false)),
        ),
        _ => return None,
    })
}

pub(in crate::candidate_app) fn collection_action(
    action: super::composed::ActionId,
) -> Option<&'static str> {
    use super::composed::ActionId::*;
    Some(match action {
        AddPostCommand | RemovePostCommand(_) => "postProcessing",
        AddIntegration | RemoveIntegration(_) => "integrations",
        AddSemanticModel | RemoveSemanticModel(_) => "semantic.models",
        _ => return None,
    })
}
