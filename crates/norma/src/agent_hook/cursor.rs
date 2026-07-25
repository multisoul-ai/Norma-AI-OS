use super::atomic_file;
use super::install::{ensure_array, ensure_object, is_multisoul_hook_command, read_json_or_empty};
use super::{build_hook_command, hook_events_for_source, AgentHookError, HookSource};
use serde_json::{json, Value};
use std::path::Path;

pub(super) fn install_cursor_hooks(path: &Path, command_path: &Path) -> Result<(), AgentHookError> {
    let mut root = read_json_or_empty(path)?;
    ensure_object(&mut root);
    root.as_object_mut()
        .expect("object ensured")
        .entry("version")
        .or_insert_with(|| json!(1));
    let command = build_hook_command(command_path, HookSource::Cursor);
    for event_name in hook_events_for_source(HookSource::Cursor) {
        add_cursor_command_hook(&mut root, event_name, &command);
    }
    atomic_file::replace(path, serde_json::to_string_pretty(&root)?.as_bytes())?;
    Ok(())
}

pub(super) fn uninstall_cursor_hooks(path: &Path) -> Result<(), AgentHookError> {
    if !path.exists() {
        return Ok(());
    }
    let mut root = read_json_or_empty(path)?;
    let mut changed = false;
    for event_name in hook_events_for_source(HookSource::Cursor) {
        if remove_multisoul_cursor_hooks_for_event(&mut root, event_name) {
            changed = true;
        }
    }
    if changed {
        cleanup_empty_cursor_hooks(&mut root);
        atomic_file::replace(path, serde_json::to_string_pretty(&root)?.as_bytes())?;
    }
    Ok(())
}

pub(super) fn cursor_hooks_installed_at(path: &Path) -> bool {
    let Ok(root) = read_json_or_empty(path) else {
        return false;
    };
    hook_events_for_source(HookSource::Cursor)
        .iter()
        .any(|event_name| cursor_event_has_multisoul_hook(&root, event_name))
}

fn cursor_event_has_multisoul_hook(root: &Value, event_name: &str) -> bool {
    root.get("hooks")
        .and_then(|hooks| hooks.get(event_name))
        .and_then(Value::as_array)
        .is_some_and(|handlers| {
            handlers.iter().any(|handler| {
                handler
                    .get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|command| is_multisoul_hook_command(command, HookSource::Cursor))
            })
        })
}

fn cleanup_empty_cursor_hooks(root: &mut Value) {
    let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) else {
        return;
    };
    hooks.retain(|_, event_hooks| {
        event_hooks
            .as_array()
            .is_some_and(|handlers| !handlers.is_empty())
    });
    if hooks.is_empty() {
        if let Some(obj) = root.as_object_mut() {
            obj.remove("hooks");
        }
    }
}

fn add_cursor_command_hook(root: &mut Value, event_name: &str, command: &str) {
    ensure_object(root);
    let hooks = root
        .as_object_mut()
        .expect("object ensured")
        .entry("hooks")
        .or_insert_with(|| json!({}));
    ensure_object(hooks);
    let event_hooks = hooks
        .as_object_mut()
        .expect("object ensured")
        .entry(event_name)
        .or_insert_with(|| json!([]));
    ensure_array(event_hooks);
    let handlers = event_hooks.as_array_mut().expect("array ensured");
    if handlers
        .iter()
        .any(|handler| handler.get("command").and_then(Value::as_str) == Some(command))
    {
        return;
    }
    handlers.push(json!({"command": command, "timeout": 5}));
}

fn remove_multisoul_cursor_hooks_for_event(root: &mut Value, event_name: &str) -> bool {
    let Some(handlers) = root
        .get_mut("hooks")
        .and_then(|hooks| hooks.get_mut(event_name))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let before = handlers.len();
    handlers.retain(|handler| {
        !handler
            .get("command")
            .and_then(Value::as_str)
            .is_some_and(|command| is_multisoul_hook_command(command, HookSource::Cursor))
    });
    handlers.len() != before
}
