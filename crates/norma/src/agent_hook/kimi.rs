//! Kimi Code CLI 2.2.0 Hook 接入。
//!
//! 已验证事实见 docs/research/2026-10-18-kimi-code-cli.md：hooks 只能写在用户级
//! `config.toml` 的 `[[hooks]]` 里，schema 严格（event/matcher/command/timeout）。
//! norma crate 没有 TOML 依赖，因此这里做文本级编辑，只管理带标记的自有区块。

use super::{atomic_file, build_hook_command, hook_events_for_source, AgentHookError, HookSource};
use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) const BLOCK_BEGIN: &str = "# >>> multisoul-agent-hook managed (do not edit) >>>";
pub(super) const BLOCK_END: &str = "# <<< multisoul-agent-hook managed <<<";
const HOOK_TIMEOUT_SECONDS: u32 = 10;

/// `KIMI_CODE_HOME` 非空时优先，否则 `~/.kimi-code/config.toml`。
pub fn kimi_config_path_for(home: &Path, kimi_code_home: Option<OsString>) -> PathBuf {
    match kimi_code_home.filter(|value| !value.is_empty()) {
        Some(root) => PathBuf::from(root).join("config.toml"),
        None => home.join(".kimi-code").join("config.toml"),
    }
}

fn invalid(message: String) -> AgentHookError {
    AgentHookError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message,
    ))
}

fn read_config(path: &Path) -> Result<String, AgentHookError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(AgentHookError::Io(error)),
    }
}

fn toml_basic_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// 返回 (去掉自有区块后的文本, 是否存在区块)；标记残缺时报错。
fn strip_block(text: &str, path: &Path) -> Result<(String, bool), AgentHookError> {
    let mut kept = String::new();
    let mut inside = false;
    let mut found = false;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if !inside && trimmed == BLOCK_BEGIN {
            inside = true;
            found = true;
        } else if inside && trimmed == BLOCK_END {
            inside = false;
        } else if !inside {
            kept.push_str(line);
        }
    }
    if inside {
        return Err(invalid(format!(
            "refusing to edit {}: managed Kimi hook block has no end marker",
            path.display()
        )));
    }
    if !found && text.contains(BLOCK_END) {
        return Err(invalid(format!(
            "refusing to edit {}: stray managed Kimi hook end marker",
            path.display()
        )));
    }
    Ok((kept, found))
}

fn has_unmergeable_hooks(text: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim();
        if trimmed == "[hooks]" {
            return true;
        }
        trimmed
            .strip_prefix("hooks")
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    })
}

fn managed_block(command_path: &Path) -> String {
    let command = toml_basic_string(&build_hook_command(command_path, HookSource::Kimi));
    let mut block = format!("{BLOCK_BEGIN}\n");
    for event in hook_events_for_source(HookSource::Kimi) {
        block.push_str(&format!(
            "[[hooks]]\nevent = \"{event}\"\ncommand = {command}\ntimeout = {HOOK_TIMEOUT_SECONDS}\n"
        ));
    }
    block.push_str(BLOCK_END);
    block.push('\n');
    block
}

pub(super) fn install_kimi_hooks(path: &Path, command_path: &Path) -> Result<(), AgentHookError> {
    let text = read_config(path)?;
    let (mut base, _) = strip_block(&text, path)?;
    if has_unmergeable_hooks(&base) {
        return Err(invalid(format!(
            "refusing to edit {}: inline `hooks = ...` or [hooks] table cannot be merged safely",
            path.display()
        )));
    }
    if !base.is_empty() && !base.ends_with('\n') {
        base.push('\n');
    }
    base.push_str(&managed_block(command_path));
    atomic_file::replace(path, base.as_bytes())?;
    Ok(())
}

pub(super) fn uninstall_kimi_hooks(path: &Path) -> Result<(), AgentHookError> {
    if !path.exists() {
        return Ok(());
    }
    let text = read_config(path)?;
    let (kept, found) = strip_block(&text, path)?;
    if found {
        atomic_file::replace(path, kept.as_bytes())?;
    }
    Ok(())
}

pub(super) fn kimi_hooks_installed_at(path: &Path) -> bool {
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    let mut inside = false;
    let mut block = String::new();
    for line in text.lines() {
        match line.trim() {
            BLOCK_BEGIN => inside = true,
            BLOCK_END => inside = false,
            _ if inside => {
                block.push_str(line);
                block.push('\n');
            }
            _ => {}
        }
    }
    hook_events_for_source(HookSource::Kimi)
        .iter()
        .all(|event| block.contains(&format!("event = \"{event}\"")))
        && block.contains("forward --source kimi")
}

/// Kimi 的 UserPromptSubmit.prompt 是 `[{type,text}]` 数组；只拼接 type=text 的段。
pub(super) fn kimi_prompt_text(raw: &Value) -> Option<String> {
    let parts = raw.get("prompt")?.as_array()?;
    let text = parts
        .iter()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}
