use super::atomic_file;
use super::cursor::{cursor_hooks_installed_at, install_cursor_hooks, uninstall_cursor_hooks};
use super::hermes::{
    hermes_plugin_installed_at, install_hermes_plugin, spawn_disable_and_remove_hermes_plugin,
    spawn_enable_hermes_plugin, uninstall_hermes_plugin,
};
use super::json_config::add_command_hook;
use super::kimi::{install_kimi_hooks, kimi_hooks_installed_at, uninstall_kimi_hooks};
use super::opencode::{install_opencode_plugin, uninstall_opencode_plugin};
use super::pi::{install_pi_extension, uninstall_pi_extension};
use super::zcode::{install_zcode_hooks, uninstall_zcode_hooks, zcode_hooks_installed_at};
use super::{
    build_hook_command, hook_events_for_source, hook_source_arg, AgentHookError, HookInstallPaths,
    HookSource, HookTarget, AGENT_HOOK_BINARY_NAME, OPENCODE_PLUGIN_MARKER, PI_EXTENSION_MARKER,
};
use super::{default_install_paths, resolve_agent_hook_command_path};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub fn install_agent_hooks(target: HookTarget) -> Result<PathBuf, AgentHookError> {
    if matches!(target, HookTarget::OpenCode | HookTarget::Hermes) {
        let paths = default_install_paths();
        install_hook_config(target, Path::new(AGENT_HOOK_BINARY_NAME), &paths)?;
        if target == HookTarget::Hermes {
            spawn_enable_hermes_plugin();
            return Ok(paths.hermes_plugin_dir);
        }
        return Ok(paths.opencode_plugin);
    }
    if target == HookTarget::Pi {
        let paths = default_install_paths();
        install_hook_config(target, Path::new(AGENT_HOOK_BINARY_NAME), &paths)?;
        return Ok(paths.pi_extension);
    }
    let command_path = resolve_agent_hook_command_path().ok_or_else(|| {
        AgentHookError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{AGENT_HOOK_BINARY_NAME} binary not found next to Norma OS or on PATH"),
        ))
    })?;
    let paths = default_install_paths();
    install_hook_config(target, &command_path, &paths)?;
    if matches!(target, HookTarget::Hermes | HookTarget::All) {
        spawn_enable_hermes_plugin();
    }
    Ok(command_path)
}

pub fn uninstall_agent_hooks(target: HookTarget) -> Result<(), AgentHookError> {
    let paths = default_install_paths();
    if matches!(target, HookTarget::Hermes | HookTarget::All) {
        // disable 约 3 秒，放后台线程：先 disable 再删受管目录，前台只处理其余目标。
        spawn_disable_and_remove_hermes_plugin(paths.hermes_plugin_dir.clone());
    }
    if target == HookTarget::Hermes {
        return Ok(());
    }
    // Hermes 目录交给上面的后台线程：实测目录缺失时 `hermes plugins disable` 失败且
    // 在 config.yaml 残留启用条目，所以 All 不能在这里同步删目录。
    uninstall_hook_config_except_hermes(target, &paths)
}

pub fn agent_hooks_installed(target: HookTarget, paths: &HookInstallPaths) -> bool {
    match target {
        HookTarget::Claude => agent_hooks_installed_for_source(HookSource::Claude, paths),
        HookTarget::Codex => agent_hooks_installed_for_source(HookSource::Codex, paths),
        HookTarget::Cursor => agent_hooks_installed_for_source(HookSource::Cursor, paths),
        HookTarget::Grok => agent_hooks_installed_for_source(HookSource::Grok, paths),
        HookTarget::OpenCode => agent_hooks_installed_for_source(HookSource::OpenCode, paths),
        HookTarget::Pi => agent_hooks_installed_for_source(HookSource::Pi, paths),
        HookTarget::Kimi => agent_hooks_installed_for_source(HookSource::Kimi, paths),
        HookTarget::Hermes => agent_hooks_installed_for_source(HookSource::Hermes, paths),
        HookTarget::ZCode => agent_hooks_installed_for_source(HookSource::ZCode, paths),
        HookTarget::All => {
            agent_hooks_installed_for_source(HookSource::Claude, paths)
                && agent_hooks_installed_for_source(HookSource::Codex, paths)
                && agent_hooks_installed_for_source(HookSource::Cursor, paths)
                && agent_hooks_installed_for_source(HookSource::Grok, paths)
                && agent_hooks_installed_for_source(HookSource::OpenCode, paths)
                && agent_hooks_installed_for_source(HookSource::Pi, paths)
                && agent_hooks_installed_for_source(HookSource::Kimi, paths)
                && agent_hooks_installed_for_source(HookSource::Hermes, paths)
                && agent_hooks_installed_for_source(HookSource::ZCode, paths)
        }
    }
}

pub fn agent_hooks_installed_for_source(source: HookSource, paths: &HookInstallPaths) -> bool {
    agent_hooks_installed_at(config_path_for_source(source, paths), source)
}

pub fn agent_hooks_installed_at(path: &Path, source: HookSource) -> bool {
    if source == HookSource::Hermes {
        return hermes_plugin_installed_at(path);
    }
    if source == HookSource::OpenCode {
        return fs::read_to_string(path)
            .is_ok_and(|source| source.contains(OPENCODE_PLUGIN_MARKER));
    }
    if source == HookSource::Pi {
        return fs::read_to_string(path).is_ok_and(|source| source.contains(PI_EXTENSION_MARKER));
    }
    if source == HookSource::Kimi {
        return kimi_hooks_installed_at(path);
    }
    if source == HookSource::Cursor {
        return cursor_hooks_installed_at(path);
    }
    if source == HookSource::ZCode {
        return zcode_hooks_installed_at(path);
    }
    let Ok(root) = read_json_or_empty(path) else {
        return false;
    };
    hook_events_for_source(source)
        .iter()
        .all(|event_name| event_has_multisoul_hook(&root, event_name, source))
}

pub fn uninstall_hook_config(
    target: HookTarget,
    paths: &HookInstallPaths,
) -> Result<(), AgentHookError> {
    uninstall_hook_config_except_hermes(target, paths)?;
    if matches!(target, HookTarget::Hermes | HookTarget::All) {
        uninstall_hermes_plugin(paths.hermes_plugin_dir.as_path())?;
    }
    Ok(())
}

/// 卸载除 Hermes 目录插件之外的目标；Hermes 须先 disable 再删目录，由调用方排序。
pub fn uninstall_hook_config_except_hermes(
    target: HookTarget,
    paths: &HookInstallPaths,
) -> Result<(), AgentHookError> {
    if matches!(target, HookTarget::Claude | HookTarget::All) {
        uninstall_for_source(paths.claude_settings.as_path(), HookSource::Claude)?;
    }
    if matches!(target, HookTarget::Codex | HookTarget::All) {
        uninstall_for_source(paths.codex_hooks.as_path(), HookSource::Codex)?;
    }
    if matches!(target, HookTarget::Cursor | HookTarget::All) {
        uninstall_cursor_hooks(paths.cursor_hooks.as_path())?;
    }
    if matches!(target, HookTarget::Grok | HookTarget::All) {
        uninstall_for_source(paths.grok_hooks.as_path(), HookSource::Grok)?;
    }
    if matches!(target, HookTarget::OpenCode | HookTarget::All) {
        uninstall_opencode_plugin(paths.opencode_plugin.as_path())?;
    }
    if matches!(target, HookTarget::Pi | HookTarget::All) {
        uninstall_pi_extension(paths.pi_extension.as_path())?;
    }
    if matches!(target, HookTarget::Kimi | HookTarget::All) {
        uninstall_kimi_hooks(paths.kimi_config.as_path())?;
    }
    if matches!(target, HookTarget::ZCode | HookTarget::All) {
        uninstall_zcode_hooks(paths.zcode_config.as_path())?;
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
    if matches!(target, HookTarget::Cursor | HookTarget::All) {
        install_cursor_hooks(paths.cursor_hooks.as_path(), command_path)?;
    }
    if matches!(target, HookTarget::Grok | HookTarget::All) {
        install_for_source(paths.grok_hooks.as_path(), command_path, HookSource::Grok)?;
    }
    if matches!(target, HookTarget::OpenCode | HookTarget::All) {
        install_opencode_plugin(paths.opencode_plugin.as_path())?;
    }
    if matches!(target, HookTarget::Pi | HookTarget::All) {
        install_pi_extension(paths.pi_extension.as_path())?;
    }
    if matches!(target, HookTarget::Kimi | HookTarget::All) {
        install_kimi_hooks(paths.kimi_config.as_path(), command_path)?;
    }
    if matches!(target, HookTarget::Hermes | HookTarget::All) {
        install_hermes_plugin(paths.hermes_plugin_dir.as_path())?;
    }
    if matches!(target, HookTarget::ZCode | HookTarget::All) {
        install_zcode_hooks(paths.zcode_config.as_path(), command_path)?;
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
        add_command_hook(&mut root, event_name, &command, source);
    }
    atomic_file::replace(path, serde_json::to_string_pretty(&root)?.as_bytes())?;
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
        atomic_file::replace(path, serde_json::to_string_pretty(&root)?.as_bytes())?;
    }
    Ok(())
}

fn config_path_for_source(source: HookSource, paths: &HookInstallPaths) -> &Path {
    match source {
        HookSource::Claude => paths.claude_settings.as_path(),
        HookSource::Codex => paths.codex_hooks.as_path(),
        HookSource::Cursor => paths.cursor_hooks.as_path(),
        HookSource::Grok => paths.grok_hooks.as_path(),
        HookSource::OpenCode => paths.opencode_plugin.as_path(),
        HookSource::Pi => paths.pi_extension.as_path(),
        HookSource::Kimi => paths.kimi_config.as_path(),
        HookSource::Hermes => paths.hermes_plugin_dir.as_path(),
        HookSource::ZCode => paths.zcode_config.as_path(),
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

pub(super) fn is_multisoul_hook_command(command: &str, source: HookSource) -> bool {
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

pub(super) fn read_json_or_empty(path: &Path) -> Result<Value, AgentHookError> {
    if !path.exists() {
        return Ok(json!({}));
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

pub(super) fn ensure_object(value: &mut Value) {
    if !value.is_object() {
        *value = json!({});
    }
}

pub(super) fn ensure_array(value: &mut Value) {
    if !value.is_array() {
        *value = json!([]);
    }
}
