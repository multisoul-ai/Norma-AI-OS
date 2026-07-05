use super::{
    build_hook_command, hook_events_for_source, hook_source_arg, AgentHookError, HookInstallPaths,
    HookSource, HookTarget, AGENT_HOOK_BINARY_NAME,
};
use super::{default_install_paths, resolve_agent_hook_command_path};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub fn install_agent_hooks(target: HookTarget) -> Result<PathBuf, AgentHookError> {
    let command_path = resolve_agent_hook_command_path().ok_or_else(|| {
        AgentHookError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{AGENT_HOOK_BINARY_NAME} binary not found next to Norma OS or on PATH"),
        ))
    })?;
    install_hook_config(target, &command_path, &default_install_paths())?;
    Ok(command_path)
}

pub fn uninstall_agent_hooks(target: HookTarget) -> Result<(), AgentHookError> {
    uninstall_hook_config(target, &default_install_paths())
}

pub fn agent_hooks_installed(target: HookTarget, paths: &HookInstallPaths) -> bool {
    match target {
        HookTarget::Claude => agent_hooks_installed_for_source(HookSource::Claude, paths),
        HookTarget::Codex => agent_hooks_installed_for_source(HookSource::Codex, paths),
        HookTarget::All => {
            agent_hooks_installed_for_source(HookSource::Claude, paths)
                && agent_hooks_installed_for_source(HookSource::Codex, paths)
        }
    }
}

pub fn agent_hooks_installed_for_source(source: HookSource, paths: &HookInstallPaths) -> bool {
    agent_hooks_installed_at(config_path_for_source(source, paths), source)
}

pub fn agent_hooks_installed_at(path: &Path, source: HookSource) -> bool {
    let Ok(root) = read_json_or_empty(path) else {
        return false;
    };
    hook_events_for_source(source)
        .iter()
        .any(|event_name| event_has_multisoul_hook(&root, event_name, source))
}

pub fn uninstall_hook_config(
    target: HookTarget,
    paths: &HookInstallPaths,
) -> Result<(), AgentHookError> {
    if matches!(target, HookTarget::Claude | HookTarget::All) {
        uninstall_for_source(paths.claude_settings.as_path(), HookSource::Claude)?;
    }
    if matches!(target, HookTarget::Codex | HookTarget::All) {
        uninstall_for_source(paths.codex_hooks.as_path(), HookSource::Codex)?;
    }
    Ok(())
}

pub fn install_hook_config(
    target: HookTarget,
    command_path: &Path,
    paths: &HookInstallPaths,
) -> Result<(), AgentHookError> {
    if matches!(target, HookTarget::Claude | HookTarget::All) {
        install_for_source(
            paths.claude_settings.as_path(),
            command_path,
            HookSource::Claude,
        )?;
    }
    if matches!(target, HookTarget::Codex | HookTarget::All) {
        install_for_source(paths.codex_hooks.as_path(), command_path, HookSource::Codex)?;
    }
    Ok(())
}

fn install_for_source(
    path: &Path,
    command_path: &Path,
    source: HookSource,
) -> Result<(), AgentHookError> {
    let mut root = read_json_or_empty(path)?;
    let command = build_hook_command(command_path, source);
    for event_name in hook_events_for_source(source) {
        add_command_hook(&mut root, event_name, &command);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(&root)?)?;
    Ok(())
}

fn uninstall_for_source(path: &Path, source: HookSource) -> Result<(), AgentHookError> {
    if !path.exists() {
        return Ok(());
    }
    let mut root = read_json_or_empty(path)?;
    let mut changed = false;
    for event_name in hook_events_for_source(source) {
        if remove_multisoul_hooks_for_event(&mut root, event_name, source) {
            changed = true;
        }
    }
    if changed {
        cleanup_empty_hooks(&mut root);
        fs::write(path, serde_json::to_string_pretty(&root)?)?;
    }
    Ok(())
}

fn config_path_for_source(source: HookSource, paths: &HookInstallPaths) -> &Path {
    match source {
        HookSource::Claude => paths.claude_settings.as_path(),
        HookSource::Codex => paths.codex_hooks.as_path(),
    }
}

fn event_has_multisoul_hook(root: &Value, event_name: &str, source: HookSource) -> bool {
    root.get("hooks")
        .and_then(|hooks| hooks.get(event_name))
        .and_then(Value::as_array)
        .is_some_and(|groups| {
            groups
                .iter()
                .any(|group| group_has_multisoul_hook(group, source))
        })
}

fn group_has_multisoul_hook(group: &Value, source: HookSource) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|handlers| {
            handlers.iter().any(|handler| {
                handler
                    .get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|command| is_multisoul_hook_command(command, source))
            })
        })
}

fn is_multisoul_hook_command(command: &str, source: HookSource) -> bool {
    command.contains(AGENT_HOOK_BINARY_NAME)
        && command.contains(&format!("forward --source {}", hook_source_arg(source)))
}

fn remove_multisoul_hooks_for_event(
    root: &mut Value,
    event_name: &str,
    source: HookSource,
) -> bool {
    ensure_object(root);
    let hooks = match root.get_mut("hooks") {
        Some(value) => value,
        None => return false,
    };
    ensure_object(hooks);
    let event_hooks = match hooks.get_mut(event_name) {
        Some(value) => value,
        None => return false,
    };
    if !event_hooks.is_array() {
        return false;
    }
    let groups = event_hooks.as_array_mut().expect("array ensured");
    let mut changed = false;
    for group in groups.iter_mut() {
        if let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) {
            let before = handlers.len();
            handlers.retain(|handler| {
                !handler
                    .get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|command| is_multisoul_hook_command(command, source))
            });
            if handlers.len() != before {
                changed = true;
            }
        }
    }
    let before_groups = groups.len();
    groups.retain(|group| {
        group
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|handlers| !handlers.is_empty())
    });
    changed || groups.len() != before_groups
}

fn cleanup_empty_hooks(root: &mut Value) {
    let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) else {
        return;
    };
    hooks.retain(|_, event_hooks| {
        event_hooks
            .as_array()
            .is_some_and(|groups| !groups.is_empty())
    });
    if hooks.is_empty() {
        if let Some(obj) = root.as_object_mut() {
            obj.remove("hooks");
        }
    }
}

fn add_command_hook(root: &mut Value, event_name: &str, command: &str) {
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
            "matcher": "*",
            "hooks": [{
                "type": "command",
                "command": command,
                "timeout": 5
            }]
        }));
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

fn read_json_or_empty(path: &Path) -> Result<Value, AgentHookError> {
    if !path.exists() {
        return Ok(json!({}));
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn ensure_object(value: &mut Value) {
    if !value.is_object() {
        *value = json!({});
    }
}

fn ensure_array(value: &mut Value) {
    if !value.is_array() {
        *value = json!([]);
    }
}
