//! 保留 Claude 用户配置中的认证信息，同时隔离权限与工具设置。

use crate::backend::BackendKind;
use serde_json::Value;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

const AUTH_KEYS: [&str; 4] = [
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_API_KEY",
    "CLAUDE_CODE_OAUTH_TOKEN",
];

pub(super) fn configure(command: &mut Command, kind: BackendKind) -> io::Result<()> {
    if kind != BackendKind::ClaudeCode {
        return Ok(());
    }
    // 显式认证按整组优先，避免把已有凭证误发到用户配置中的另一个网关。
    if AUTH_KEYS
        .iter()
        .any(|key| environment(command, key).is_some())
    {
        return Ok(());
    }
    let dir = environment(command, "CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| environment(command, "HOME").map(|home| PathBuf::from(home).join(".claude")));
    let Some(dir) = dir else { return Ok(()) };
    let path = command
        .get_current_dir()
        .unwrap_or_else(|| Path::new("."))
        .join(dir)
        .join("settings.json");
    let data = match std::fs::read(path) {
        Ok(data) => data,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let settings: Value = serde_json::from_slice(&data)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Claude 认证配置不是有效 JSON"))?;
    let Some(env) = settings.get("env") else {
        return Ok(());
    };
    let env = env.as_object().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Claude 认证配置的 env 必须是对象",
        )
    })?;
    for key in AUTH_KEYS {
        if let Some(value) = env.get(key) {
            let value = value.as_str().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Claude 认证环境变量必须是字符串",
                )
            })?;
            command.env(key, value);
        }
    }
    Ok(())
}

fn environment(command: &Command, key: &str) -> Option<OsString> {
    match command.get_envs().find(|(name, _)| *name == key) {
        Some((_, value)) => value.map(OsString::from),
        None => std::env::var_os(key),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/claude_auth_tests.rs"]
mod tests;
