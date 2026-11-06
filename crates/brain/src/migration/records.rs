use crate::preferences::AskRule;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io;
use std::path::Path;

pub(super) fn invalid(path: &Path, reason: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{}: {reason}", path.display()),
    )
}

pub(super) fn read_json(path: &Path) -> io::Result<Option<Value>> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| invalid(path, error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn set_scope(value: &mut Value, fallback: Option<&str>, path: &Path) -> io::Result<String> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| invalid(path, "expected an object"))?;
    if object.get("workspace_id").is_some_and(|id| !id.is_string()) {
        return Err(invalid(path, "expected string workspace identity"));
    }
    let present = object
        .get("workspace_id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty());
    if let (Some(present), Some(fallback)) = (present, fallback) {
        if present != fallback {
            return Err(invalid(path, "conflicting workspace identity"));
        }
    }
    let id = present
        .or(fallback)
        .ok_or_else(|| invalid(path, "missing workspace identity"))?
        .to_owned();
    object.insert("workspace_id".into(), Value::String(id.clone()));
    Ok(id)
}

pub(super) fn goals(path: &Path, fallback: Option<&str>) -> io::Result<Vec<Value>> {
    let Some(value) = read_json(path)? else {
        return Ok(Vec::new());
    };
    let mut items = value
        .as_array()
        .ok_or_else(|| invalid(path, "expected goals array"))?
        .clone();
    for item in &mut items {
        serde_json::from_value::<crate::goal::Goal>(item.clone()).map_err(|e| invalid(path, e))?;
        set_scope(item, fallback, path)?;
    }
    Ok(items)
}

pub(super) fn decisions(path: &Path, fallback: Option<&str>) -> io::Result<Vec<Value>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut item = serde_json::from_str(line).map_err(|e| invalid(path, e))?;
            set_scope(&mut item, fallback, path)?;
            Ok(item)
        })
        .collect()
}

pub(super) fn proposals(path: &Path, fallback: Option<&str>) -> io::Result<Option<Value>> {
    #[derive(Deserialize)]
    struct Saved {
        next_id: u64,
        items: Vec<Item>,
    }
    #[derive(Deserialize)]
    struct Item {
        id: u64,
        agent: String,
        title: String,
        task: String,
        why: String,
        at: u64,
    }
    let Some(mut value) = read_json(path)? else {
        return Ok(None);
    };
    let saved: Saved = serde_json::from_value(value.clone()).map_err(|e| invalid(path, e))?;
    let unique: BTreeSet<_> = saved.items.iter().map(|item| item.id).collect();
    if unique.len() != saved.items.len() {
        return Err(invalid(path, "duplicate proposal ID in one source file"));
    }
    let highest = saved
        .items
        .iter()
        .map(|item| item.id)
        .max()
        .unwrap_or(0)
        .max(saved.next_id);
    if highest == u64::MAX {
        return Err(invalid(path, "proposal ID exhausted"));
    }
    let id = set_scope(&mut value, fallback, path)?;
    for item in &saved.items {
        // Deserialize every required field without trimming, validating copy, or losing legacy text.
        let _body = (&item.agent, &item.title, &item.task, &item.why);
        if std::time::SystemTime::UNIX_EPOCH
            .checked_add(std::time::Duration::from_secs(item.at))
            .is_none()
        {
            return Err(invalid(path, "proposal timestamp out of range"));
        }
    }
    for item in value["items"].as_array_mut().expect("validated items") {
        set_scope(item, Some(&id), path)?;
    }
    value["next_id"] = json!(highest);
    Ok(Some(value))
}

pub(super) fn merge_proposals(target: &mut Value, incoming: Value, path: &Path) -> io::Result<()> {
    let mut next = target["next_id"]
        .as_u64()
        .unwrap()
        .max(incoming["next_id"].as_u64().unwrap());
    let items = target["items"].as_array_mut().unwrap();
    for mut item in incoming["items"].as_array().unwrap().iter().cloned() {
        if items.contains(&item) {
            continue;
        }
        if items.iter().any(|existing| existing["id"] == item["id"]) {
            next = next
                .checked_add(1)
                .filter(|id| *id < u64::MAX)
                .ok_or_else(|| invalid(path, "proposal ID exhausted"))?;
            item["legacy_id"] = item["id"].clone();
            item["id"] = json!(next);
        }
        items.push(item);
    }
    target["next_id"] = json!(next);
    Ok(())
}

pub(super) fn settings(path: &Path) -> io::Result<Option<Value>> {
    let value = read_json(path)?;
    if value.as_ref().is_some_and(|v| !v.is_object()) {
        return Err(invalid(path, "expected settings object"));
    }
    if let Some(value) = &value {
        for key in ["enabled", "parallel_approvals", "speak"] {
            if value.get(key).is_some_and(|flag| !flag.is_boolean()) {
                return Err(invalid(path, format!("expected boolean {key}")));
            }
        }
        if let Some(autonomy) = value.get("autonomy") {
            if !autonomy.is_object() {
                return Err(invalid(path, "expected autonomy object"));
            }
            for key in ["readonly_commands", "project_writes", "assign_followups"] {
                if autonomy.get(key).is_some_and(|flag| !flag.is_boolean()) {
                    return Err(invalid(path, format!("expected boolean autonomy.{key}")));
                }
            }
        }
        if value
            .get("always_ask")
            .is_some_and(|rules| !rules.is_array())
        {
            return Err(invalid(path, "expected always_ask array"));
        }
    }
    Ok(value)
}

pub(super) fn rules(
    value: &Value,
    scope: Option<&str>,
    path: &Path,
) -> io::Result<BTreeSet<AskRule>> {
    if value.is_null() {
        return Ok(BTreeSet::new());
    }
    let items = value
        .as_array()
        .ok_or_else(|| invalid(path, "expected always_ask array"))?;
    items
        .iter()
        .map(|item| {
            let mut rule = if let Some(key) = item.as_str() {
                AskRule::from(key)
            } else {
                serde_json::from_value::<AskRule>(item.clone())
                    .map_err(|error| invalid(path, error))?
            };
            if rule.key.trim().is_empty()
                || rule
                    .workspace_id
                    .as_ref()
                    .is_some_and(|id| id.trim().is_empty())
            {
                return Err(invalid(path, "empty confirmation rule"));
            }
            if let Some(scope) = scope {
                if rule.workspace_id.as_deref().is_some_and(|id| id != scope) {
                    return Err(invalid(path, "conflicting confirmation rule scope"));
                }
                rule.workspace_id = Some(scope.to_owned());
            }
            Ok(rule)
        })
        .collect()
}
