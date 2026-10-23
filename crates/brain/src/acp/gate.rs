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
        let tool = call["name"]
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
            Value::Array(items) => Some(
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            _ => None,
        };
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
            "Read" | "Glob" | "Grep" | "LS" => self.judge_read(request.path.as_deref()),
            other => Verdict::Reject(format!("大脑不能使用 {other}")),
        }
    }

    fn judge_command(&self, command: Option<&str>) -> Verdict {
        let Some(argv) = command.and_then(single_command) else {
            return Verdict::Reject("只允许单条 norma 命令，不允许管道、重定向或组合".into());
        };
        if Path::new(&argv[0]) == self.norma_bin {
            Verdict::Allow
        } else {
            Verdict::Reject(format!("只允许执行 {}", self.norma_bin.display()))
        }
    }

    fn judge_read(&self, path: Option<&str>) -> Verdict {
        let Some(path) = path else {
            return Verdict::Reject("读取请求缺少路径".into());
        };
        if is_secret_path(path) {
            return Verdict::Reject("不允许读取密钥或凭据文件".into());
        }
        if crate::risk::is_inside_project(&self.project_root, path) {
            Verdict::Allow
        } else {
            Verdict::Reject("只允许读取工作空间内的文件".into())
        }
    }
}

/// 在 `options` 里找出与结论对应的选项 id。
pub fn pick_option(params: &Value, verdict: &Verdict) -> Option<String> {
    let want_allow = matches!(verdict, Verdict::Allow);
    params["options"].as_array()?.iter().find_map(|option| {
        let kind = option["kind"].as_str().unwrap_or_default();
        (kind.starts_with("allow") == want_allow && !kind.is_empty())
            .then(|| option["optionId"].as_str().map(str::to_string))
            .flatten()
    })
}

#[cfg(test)]
#[path = "../../tests/unit/gate_tests.rs"]
mod gate_tests;
