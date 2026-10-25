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

/// `session/new` 的 `_meta`：仅暴露逐文件 Read 和 Bash，且每次都必须向 ACP 请求权限。
/// 默认安全读取也可能不触发 canUseTool，所以除了隔离用户设置，还需要显式 ask 规则。
pub fn session_meta(kind: BackendKind, model: Option<&str>) -> Option<Value> {
    if kind != BackendKind::ClaudeCode {
        return None;
    }
    let mut options = json!({
        "settingSources": [],
        "disallowedTools": DISALLOWED_TOOLS,
        "tools": ["Bash", "Read"],
        "strictMcpConfig": true,
        "allowDangerouslySkipPermissions": false,
        "settings": {"permissions": {
            "ask": ["Bash", "Read"],
            "disableBypassPermissionsMode": "disable"
        }}
    });
    if let Some(model) = model {
        options["model"] = json!(model);
    }
    Some(json!({"claudeCode": {"options": options}}))
}

/// Claude 使用 default，并由 session_meta 的逐次 ask 规则补齐默认只读工具。
/// Codex 使用 read-only：只限制写入，不限制读取，已知的读取隔离缺口由用户接受。
pub fn session_mode(kind: BackendKind) -> Option<&'static str> {
    match kind {
        BackendKind::ClaudeCode => Some("default"),
        BackendKind::Codex => Some("read-only"),
        BackendKind::OpenCode => None,
    }
}

/// OpenCode 的候选配置；它可能被 Agent 级权限覆盖，不能据此保证逐次询问。
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
