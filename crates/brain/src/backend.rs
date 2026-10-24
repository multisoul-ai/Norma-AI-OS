//! 检测用户已安装的 ACP 后端，并把策略映射到模型与会话选项。

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendKind {
    ClaudeCode,
    Codex,
    OpenCode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Backend {
    pub kind: BackendKind,
    pub program: PathBuf,
    pub args: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Strategy {
    Speed,
    Auto,
    Quality,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Models {
    pub main: Option<String>,
    pub approval: Option<String>,
}

/// 大脑不应使用的写类和联网工具，作为权限闸门之外的第二道防线。
const DISALLOWED_TOOLS: [&str; 6] = [
    "Write",
    "Edit",
    "MultiEdit",
    "NotebookEdit",
    "WebFetch",
    "WebSearch",
];

impl BackendKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::OpenCode => "OpenCode",
        }
    }
}

/// 在适配器目录（`<dir>/node_modules/.bin`）和 PATH 目录中找可用后端。
pub fn detect(adapter_dir: &Path, path_dirs: &[PathBuf]) -> Vec<Backend> {
    let mut dirs = vec![adapter_dir.join("node_modules").join(".bin")];
    dirs.extend_from_slice(path_dirs);
    let find = |name: &str| dirs.iter().map(|d| d.join(name)).find(|p| p.is_file());
    let mut found = Vec::new();
    if let Some(program) = find("claude-agent-acp") {
        found.push(Backend {
            kind: BackendKind::ClaudeCode,
            program,
            args: vec![],
        });
    }
    if let Some(program) = find("codex-acp") {
        found.push(Backend {
            kind: BackendKind::Codex,
            program,
            args: vec![],
        });
    }
    if let Some(program) = find("opencode") {
        found.push(Backend {
            kind: BackendKind::OpenCode,
            program,
            args: vec!["acp".into()],
        });
    }
    found
}

/// 策略到模型别名：Claude Code 接受 opus / sonnet / haiku；其他后端沿用 CLI 默认。
pub fn models_for(kind: BackendKind, strategy: Strategy) -> Models {
    if kind != BackendKind::ClaudeCode {
        return Models {
            main: None,
            approval: None,
        };
    }
    let pick = |main: &str, approval: &str| Models {
        main: Some(main.into()),
        approval: Some(approval.into()),
    };
    match strategy {
        Strategy::Speed => pick("haiku", "haiku"),
        Strategy::Auto => pick("opus", "haiku"),
        Strategy::Quality => pick("opus", "opus"),
    }
}

/// `session/new` 的 `_meta`：忽略用户自己的权限设置（可能是 bypassPermissions），禁用写工具。
pub fn session_meta(kind: BackendKind, model: Option<&str>) -> Option<Value> {
    if kind != BackendKind::ClaudeCode {
        return None;
    }
    let mut options = json!({"settingSources": [], "disallowedTools": DISALLOWED_TOOLS});
    if let Some(model) = model {
        options["model"] = json!(model);
    }
    Some(json!({"claudeCode": {"options": options}}))
}

/// 会话创建后切到需要确认的模式，让写入和越出沙箱的操作都发 request_permission。
/// Codex 默认的 Auto review 会直接执行命令；只读模式下只读命令在沙箱里跑，
/// 写文件、连 norma 的 socket 都要先请求批准。
pub fn session_mode(kind: BackendKind) -> Option<&'static str> {
    match kind {
        BackendKind::ClaudeCode => Some("default"),
        BackendKind::Codex => Some("read-only"),
        BackendKind::OpenCode => None,
    }
}

#[cfg(test)]
#[path = "../tests/unit/backend_tests.rs"]
mod backend_tests;
