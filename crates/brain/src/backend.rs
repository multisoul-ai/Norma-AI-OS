//! 检测用户已安装的 ACP 后端，并把策略映射到模型与会话选项。

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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

/// 会话进程额外的环境变量。OpenCode 默认的 build 模式执行命令不发权限请求，
/// 注入配置让每条命令都先请求批准，并禁止它改文件和联网。
pub fn session_env(kind: BackendKind) -> Vec<(String, String)> {
    if kind != BackendKind::OpenCode {
        return Vec::new();
    }
    let config = json!({"permission": {"bash": "ask", "edit": "deny", "webfetch": "deny"}});
    vec![("OPENCODE_CONFIG_CONTENT".into(), config.to_string())]
}

/// Codex 的推理强度：主会话随策略取 low、medium、high，审批会话始终 low。其他后端不设。
pub fn effort_for(kind: BackendKind, strategy: Strategy, main: bool) -> Option<&'static str> {
    if kind != BackendKind::Codex {
        return None;
    }
    Some(match (main, strategy) {
        (false, _) | (true, Strategy::Speed) => "low",
        (true, Strategy::Auto) => "medium",
        (true, Strategy::Quality) => "high",
    })
}

/// 后端会话里可选的一个模型。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelChoice {
    pub id: String,
    pub name: String,
}

/// 从 `session/new` 的回复里读出模型列表：取 `configOptions` 中 id 为 `model` 的那一项。
pub fn model_choices(created: &Value) -> Vec<ModelChoice> {
    created["configOptions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|option| option["id"] == "model")
        .flat_map(|option| option["options"].as_array().cloned().unwrap_or_default())
        .filter_map(|choice| {
            let id = choice["value"].as_str()?.to_string();
            let name = choice["name"].as_str().unwrap_or(&id).to_string();
            Some(ModelChoice { id, name })
        })
        .collect()
}

#[cfg(test)]
#[path = "../tests/unit/backend_tests.rs"]
mod backend_tests;
