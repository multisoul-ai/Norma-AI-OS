use super::install::{ensure_array, ensure_object};
use super::HookSource;
use serde_json::{json, Value};

pub(super) fn add_command_hook(
    root: &mut Value,
    event_name: &str,
    command: &str,
    source: HookSource,
) {
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
    if command_exists(event_hooks, command) {
        return;
    }
    ensure_array(event_hooks);
    let hook_group = if source == HookSource::Grok {
        json!({
            "hooks": [{
                "type": "command",
                "command": command,
                "timeout": 5
            }]
        })
    } else {
        json!({
            "matcher": "*",
            "hooks": [{
                "type": "command",
                "command": command,
                "timeout": 5
            }]
        })
    };
    event_hooks
        .as_array_mut()
        .expect("array ensured")
        .push(hook_group);
}

/// Question hooks are a separate matcher group so lifecycle forwards stay at timeout 5.
pub(super) fn add_matched_command_hook(
    root: &mut Value,
    event_name: &str,
    command: &str,
    matcher: &str,
    timeout: u64,
) {
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
    if command_exists(event_hooks, command) {
        return;
    }
    ensure_array(event_hooks);
    event_hooks
        .as_array_mut()
        .expect("array ensured")
        .push(json!({
            "matcher": matcher,
            "hooks": [{
                "type": "command",
                "command": command,
                "timeout": timeout
            }]
        }));
}

pub(super) fn question_hook_install(source: HookSource) -> Option<(&'static str, u64)> {
    match source {
        // Grok maps the Claude tool name onto ask_user_question.
        HookSource::Grok => Some(("AskUserQuestion", 125)),
        // InfCode's live tool is `question`, not AskUserQuestion.
        HookSource::InfCode => Some(("question", 125)),
        _ => None,
    }
}

fn command_exists(event_hooks: &Value, command: &str) -> bool {
    event_hooks.as_array().is_some_and(|groups| {
        groups.iter().any(|group| {
            group
                .get("hooks")
                .and_then(Value::as_array)
                .is_some_and(|handlers| {
                    handlers.iter().any(|handler| {
                        handler.get("command").and_then(Value::as_str) == Some(command)
                    })
                })
        })
    })
}
