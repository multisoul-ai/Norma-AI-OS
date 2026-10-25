//! 大脑的权限闸门：只放行 `norma` 命令和工作空间内的只读访问。

use crate::risk::{is_secret_path, single_command};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionRequest {
    pub tool: String,
    pub command: Option<String>,
    pub path: Option<String>,
}

impl PermissionRequest {
    /// 从 ACP 的 `session/request_permission` 参数提取判断所需字段，兼容不同适配器。
    pub fn from_params(params: &Value) -> Self {
        let call = &params["toolCall"];
        let raw = &call["rawInput"];
        let kind = call["kind"].as_str().unwrap_or_default();
        let mut tool = call["name"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| match kind {
                "execute" => "Bash".into(),
                "read" | "search" => "Read".into(),
                "edit" | "delete" | "move" => "Write".into(),
                other => other.to_string(),
            });
        let command = match &raw["command"] {
            Value::String(s) => Some(s.clone()),
            Value::Object(fields) if fields.len() == 1 => fields
                .get("normaShellCommand")
                .and_then(Value::as_str)
                .and_then(shell_script),
            Value::Array(items) => items
                .iter()
                .map(Value::as_str)
                .collect::<Option<Vec<_>>>()
                .map(|argv| {
                    argv.into_iter()
                        .map(|arg| format!("'{}'", arg.replace('\'', "'\\''")))
                        .collect::<Vec<_>>()
                        .join(" ")
                }),
            _ => None,
        };
        if tool == "exec_command"
            && kind == "execute"
            && raw["command"]["normaShellCommand"].is_string()
            && command.is_some()
        {
            tool = "Bash".into();
        }
        let path = raw["file_path"]
            .as_str()
            .or_else(|| raw["path"].as_str())
            .or_else(|| call["locations"][0]["path"].as_str())
            .map(str::to_string);
        Self {
            tool,
            command,
            path,
        }
    }
}

/// 只解码适配器保留下来的原始 shell argv。裸字符串里的引号仍代表可执行文件名，
/// 不能猜测它曾经是 shell 包装，更不能递归拆包。
fn shell_script(command: &str) -> Option<String> {
    let argv = single_command(command)?;
    if argv.len() != 3
        || !matches!(argv[0].as_str(), "/bin/sh" | "/bin/bash" | "/bin/zsh")
        || !matches!(argv[1].as_str(), "-c" | "-lc")
    {
        return None;
    }
    Some(argv[2].clone())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Verdict {
    Allow,
    Reject(String),
}

#[derive(Clone, Debug)]
pub struct Gate {
    pub norma_bin: PathBuf,
    pub project_root: PathBuf,
}

impl Gate {
    pub fn judge(&self, request: &PermissionRequest) -> Verdict {
        match request.tool.as_str() {
            "Bash" => self.judge_command(request.command.as_deref()),
            "Read" => self.judge_read(request.path.as_deref()),
            other => Verdict::Reject(format!("大脑不能使用 {other}")),
        }
    }

    fn judge_command(&self, command: Option<&str>) -> Verdict {
        let Some(argv) = command.and_then(single_command) else {
            return Verdict::Reject("只允许单条 norma 命令，不允许管道、重定向或组合".into());
        };
        if Path::new(&argv[0]) != self.norma_bin {
            return Verdict::Reject(format!("只允许执行 {}", self.norma_bin.display()));
        }
        // 只开放 brain 子命令，避免大脑通过 card send 之类的命令向终端注入输入。
        if argv.get(1).map(String::as_str) != Some("brain") {
            return Verdict::Reject("只允许 norma brain 子命令".into());
        }
        Verdict::Allow
    }

    fn judge_read(&self, path: Option<&str>) -> Verdict {
        let Some(path) = path else {
            return Verdict::Reject("读取请求缺少路径".into());
        };
        if is_secret_path(path) {
            return Verdict::Reject("不允许读取密钥或凭据文件".into());
        }
        let root = self.project_root.canonicalize();
        let candidate = self.project_root.join(path).canonicalize();
        if let (Ok(root), Ok(candidate)) = (root, candidate) {
            if candidate.is_file()
                && candidate.starts_with(root)
                && !is_secret_path(&candidate.to_string_lossy())
                && crate::risk::is_inside_project(&self.project_root, path)
            {
                return Verdict::Allow;
            }
        }
        Verdict::Reject("只允许读取工作空间内实际存在的非凭据文件".into())
    }
}

/// 在 `options` 里找出与结论对应的选项 id。
pub fn pick_option(params: &Value, verdict: &Verdict) -> Option<String> {
    let wanted = if matches!(verdict, Verdict::Allow) {
        "allow_once"
    } else {
        "reject_once"
    };
    params["options"].as_array()?.iter().find_map(|option| {
        let kind = option["kind"].as_str().unwrap_or_default();
        (kind == wanted)
            .then(|| option["optionId"].as_str().map(str::to_string))
            .flatten()
    })
}

#[cfg(test)]
#[path = "../../tests/unit/gate_tests.rs"]
mod gate_tests;

#[cfg(test)]
#[path = "../../tests/unit/gate_shell_tests.rs"]
mod gate_shell_tests;
