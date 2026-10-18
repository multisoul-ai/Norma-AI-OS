//! ZCode 的 hooks 安装：合并写入 `~/.zcode/cli/config.json` 的 `hooks.events`。
//!
//! 已验证（docs/research/2026-10-18-zcode.md）：`hooks.enabled` 缺省为关闭，必须设为 true 才触发；
//! 事件为 `{hooks:[{type,command,timeout}]}` 分组。该文件由用户和 ZCode 自己共用，因此只按
//! Norma 命令字符串精确增删，保留所有用户字段；格式异常时拒绝覆盖。

use super::atomic_file;
use super::install::{is_multisoul_hook_command, read_json_or_empty};
use super::{build_hook_command, hook_events_for_source, AgentHookError, HookSource};
use serde_json::{json, Map, Value};
use std::path::Path;

const SOURCE: HookSource = HookSource::ZCode;

fn invalid(reason: &str) -> AgentHookError {
    AgentHookError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("ZCode config.json {reason}; refusing to overwrite"),
    ))
}

/// 取出 `hooks.events` 对象；缺失时按需创建，类型不符则拒绝。
fn events_mut(root: &mut Value) -> Result<&mut Map<String, Value>, AgentHookError> {
    let root_object = root
        .as_object_mut()
        .ok_or_else(|| invalid("root is not an object"))?;
    let hooks = root_object.entry("hooks").or_insert_with(|| json!({}));
    let hooks = hooks
        .as_object_mut()
        .ok_or_else(|| invalid("hooks is not an object"))?;
    let events = hooks.entry("events").or_insert_with(|| json!({}));
    events
        .as_object_mut()
        .ok_or_else(|| invalid("hooks.events is not an object"))
}

/// `hooks.enabled` 不是 true 时用户自有 hook 处于休眠；安装会把总开关打开并让它们突然执行，
/// 因此存在任何非 Norma 的 handler 时拒绝安装，由用户自行决定是否开启。
fn refuse_when_user_hooks_disabled(root: &Value) -> Result<(), AgentHookError> {
    if root.pointer("/hooks/enabled") == Some(&json!(true)) {
        return Ok(());
    }
    let has_user_handler = root
        .pointer("/hooks/events")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|events| events.values())
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(|group| group.get("hooks").and_then(Value::as_array))
        .flatten()
        .any(|handler| !is_norma_handler(handler));
    if has_user_handler {
        return Err(invalid(
            "has user hooks while hooks.enabled is not true; enabling would activate them",
        ));
    }
    Ok(())
}

pub(super) fn install_zcode_hooks(path: &Path, command_path: &Path) -> Result<(), AgentHookError> {
    let mut root = read_json_or_empty(path)?;
    let command = build_hook_command(command_path, SOURCE);
    refuse_when_user_hooks_disabled(&root)?;
    let events = events_mut(&mut root)?;
    for name in hook_events_for_source(SOURCE) {
        let groups = events.entry(*name).or_insert_with(|| json!([]));
        let groups = groups
            .as_array_mut()
            .ok_or_else(|| invalid("an event is not an array"))?;
        let exists = groups
            .iter()
            .any(|group| group_commands(group).any(|c| c == command));
        if !exists {
            groups.push(json!({"hooks": [{"type": "command", "command": command, "timeout": 5}]}));
        }
    }
    root["hooks"]["enabled"] = json!(true);
    atomic_file::replace(path, serde_json::to_string_pretty(&root)?.as_bytes())?;
    Ok(())
}

pub(super) fn uninstall_zcode_hooks(path: &Path) -> Result<(), AgentHookError> {
    if !path.exists() {
        return Ok(());
    }
    let mut root = read_json_or_empty(path)?;
    let Some(events) = root
        .pointer_mut("/hooks/events")
        .and_then(Value::as_object_mut)
    else {
        return Ok(());
    };
    let mut changed = false;
    for name in hook_events_for_source(SOURCE) {
        let Some(groups) = events.get_mut(*name).and_then(Value::as_array_mut) else {
            continue;
        };
        groups.retain_mut(|group| {
            let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                return true;
            };
            let count = handlers.len();
            handlers.retain(|h| !is_norma_handler(h));
            if handlers.len() == count {
                return true;
            }
            changed = true;
            // 只丢弃被本次卸载清空的分组，用户原本为空或缺 hooks 的分组原样保留。
            !handlers.is_empty()
        });
        // 事件数组即使被清空也保留：无法区分它是 Norma 创建的还是用户原有的空数组。
    }
    if changed {
        // 不回写 hooks.enabled：用户可能已依赖它；只在 events 为空时移除空壳 events。
        if events.is_empty() {
            if let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) {
                hooks.remove("events");
            }
        }
        atomic_file::replace(path, serde_json::to_string_pretty(&root)?.as_bytes())?;
    }
    Ok(())
}

pub(super) fn zcode_hooks_installed_at(path: &Path) -> bool {
    let Ok(root) = read_json_or_empty(path) else {
        return false;
    };
    root.pointer("/hooks/enabled") == Some(&json!(true))
        && hook_events_for_source(SOURCE).iter().all(|name| {
            root.pointer("/hooks/events")
                .and_then(|events| events.get(*name))
                .and_then(Value::as_array)
                .is_some_and(|groups| {
                    groups
                        .iter()
                        .any(|g| group_commands(g).any(is_norma_command))
                })
        })
}

fn group_commands(group: &Value) -> impl Iterator<Item = &str> {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|handler| handler.get("command").and_then(Value::as_str))
}

fn is_norma_command(command: &str) -> bool {
    is_multisoul_hook_command(command, SOURCE)
}

fn is_norma_handler(handler: &Value) -> bool {
    handler
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(is_norma_command)
}

#[cfg(test)]
#[path = "zcode_tests.rs"]
mod tests;
