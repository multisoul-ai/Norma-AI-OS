//! 大脑的本地配置：`~/.norma/brain/config.json`，以及后端和适配器进程的运行环境。

use brain::{Backend, BackendKind, Strategy};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(super) struct BrainSettings {
    pub enabled: bool,
    pub backend: Option<BackendKind>,
    pub strategy: Strategy,
}

/// 默认 `~/.norma/brain`；`NORMA_BRAIN_DIR` 可整体改位置，用于隔离测试。
pub(super) fn state_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("NORMA_BRAIN_DIR").filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    dirs::home_dir().map(|home| home.join(".norma").join("brain"))
}

/// 读取配置；文件不存在或损坏时大脑保持关闭。
pub(super) fn load_settings() -> BrainSettings {
    let Some(path) = state_dir().map(|dir| dir.join("config.json")) else {
        return parse_settings("");
    };
    parse_settings(&std::fs::read_to_string(path).unwrap_or_default())
}

/// 空文本或无效 JSON 都视为关闭。
pub(super) fn parse_settings(text: &str) -> BrainSettings {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return BrainSettings {
            enabled: false,
            backend: None,
            strategy: Strategy::Auto,
        };
    };
    BrainSettings {
        enabled: value["enabled"].as_bool().unwrap_or(false),
        backend: match value["backend"].as_str() {
            Some("claude") => Some(BackendKind::ClaudeCode),
            Some("codex") => Some(BackendKind::Codex),
            Some("opencode") => Some(BackendKind::OpenCode),
            _ => None,
        },
        strategy: match value["strategy"].as_str() {
            Some("speed") => Strategy::Speed,
            Some("quality") => Strategy::Quality,
            _ => Strategy::Auto,
        },
    }
}

/// GUI 启动的进程 PATH 很短，补上 Homebrew 和 nvm 的 node 目录，适配器是 node 脚本。
pub(super) fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    if let Some(home) = dirs::home_dir() {
        let versions = home.join(".nvm").join("versions").join("node");
        let mut found: Vec<PathBuf> = std::fs::read_dir(versions)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path().join("bin"))
            .collect();
        found.sort();
        found.reverse();
        dirs.extend(found);
        dirs.push(home.join(".local").join("bin"));
    }
    dirs
}

/// 按配置挑后端；没有指定时按 Claude Code、Codex、OpenCode 的顺序取第一个。
pub(super) fn pick_backend(settings: &BrainSettings) -> Option<Backend> {
    let dir = state_dir()?.join("adapters");
    let found = brain::backend::detect(&dir, &search_dirs());
    match settings.backend {
        Some(kind) => found.into_iter().find(|backend| backend.kind == kind),
        None => found.into_iter().next(),
    }
}

pub(super) fn adapter_env(
    token: &str,
    workspace_id: &str,
    route_id: &str,
) -> Vec<(String, String)> {
    let path = std::env::join_paths(search_dirs())
        .map(|joined| joined.to_string_lossy().into_owned())
        .unwrap_or_default();
    vec![
        ("PATH".into(), path),
        (
            "NORMA_SOCKET".into(),
            norma::client::default_socket_path().display().to_string(),
        ),
        ("NORMA_BRAIN_TOKEN".into(), token.into()),
        ("NORMA_WORKSPACE_ID".into(), workspace_id.into()),
        ("NORMA_WORKSPACE_ROUTE_ID".into(), route_id.into()),
    ]
}
