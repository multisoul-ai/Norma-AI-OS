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
    // 只有 2.x 才有原生问答；1.x 排在 PATH 前面时不能遮住后面的新版。
    let opencode = dirs
        .iter()
        .map(|d| d.join("opencode"))
        .filter(|p| p.is_file())
        .find(|p| opencode_major(p).is_some_and(|major| major >= 2));
    if let Some(program) = opencode {
        found.push(Backend {
            kind: BackendKind::OpenCode,
            program,
            args: vec!["acp".into()],
        });
    }
    found
}

/// `opencode --version` 的主版本号，兼容 `1.19.3` 与 `opencode v2.1.3` 两种输出。
fn opencode_major(program: &Path) -> Option<u32> {
    let output = std::process::Command::new(program)
        .arg("--version")
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .find_map(|word| word.trim_start_matches('v').split('.').next()?.parse().ok())
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

/// `session/new` 的 `_meta`。主会话和审批会话不限制工具。
/// 幕僚会话只留 Bash 与 Read，并要求每次 Bash 都进权限回调。
pub fn session_meta(kind: BackendKind, model: Option<&str>, staff: bool) -> Option<Value> {
    if kind != BackendKind::ClaudeCode {
        return None;
    }
    let mut options = json!({
        "settingSources": [],
        "strictMcpConfig": true,
        // 大脑会话不写 ~/.claude/projects，避免挤进用户的 /resume 列表。
        "persistSession": false,
        "allowDangerouslySkipPermissions": false
    });
    if let Some(model) = model {
        options["model"] = json!(model);
    }
    if staff {
        options["tools"] = json!(["Bash", "Read"]);
        options["settings"] = json!({
            "permissions": {
                "ask": ["Bash"],
                "blockReadsOutsideWorkingDirectories": true
            }
        });
    }
    Some(json!({"claudeCode": {"options": options}}))
}

/// 共用进程时，token 不能放进进程环境（那会被所有会话看见）。
/// Claude 走 `options.env`，Codex 走 `norma.config`，由加载器合并进该会话。
pub fn shared_staff_meta(kind: BackendKind, model: Option<&str>, token: &str) -> Option<Value> {
    match kind {
        BackendKind::ClaudeCode => {
            let mut meta = session_meta(kind, model, true)?;
            meta["claudeCode"]["options"]["env"] = json!({ "NORMA_BRAIN_TOKEN": token });
            Some(meta)
        }
        BackendKind::Codex => Some(json!({
            "norma": {
                "config": {
                    "shell_environment_policy": { "set": { "NORMA_BRAIN_TOKEN": token } }
                }
            }
        })),
        BackendKind::OpenCode => None,
    }
}

/// Claude 使用 default；Codex 使用 auto（可写工作区）。权限请求都会被放行。
pub fn session_mode(kind: BackendKind) -> Option<&'static str> {
    match kind {
        BackendKind::ClaudeCode => Some("default"),
        BackendKind::Codex => Some("auto"),
        BackendKind::OpenCode => None,
    }
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
