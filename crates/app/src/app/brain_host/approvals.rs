//! 把 Runtime 里等待处理的审批翻译成大脑能判断的形态，并还原审批标识。

use brain::risk::ApprovalSubject;
use norma::mobile::{ApprovalSnapshot, ApprovalTarget};
use serde_json::Value;
use std::path::Path;

/// 审批标识：各字段只含字母、数字、`-`、`_`、`:`，所以可以用 `.` 连接再拆开。
pub(super) fn encode_id(target: &ApprovalTarget) -> String {
    format!(
        "{}.{}.{}.{}.{}.{}",
        target.node_id,
        target.profile,
        target.run_id,
        target.session_id,
        target.turn_id,
        target.request_id
    )
}

fn text_of(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let words: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
            // Codex 常把命令包成 `["bash", "-lc", "<真正的命令>"]`，判断时看里面那条。
            match words.as_slice() {
                [shell, flag, inner]
                    if matches!(
                        *shell,
                        "bash" | "sh" | "zsh" | "/bin/bash" | "/bin/sh" | "/bin/zsh"
                    ) && flag.starts_with('-')
                        && flag.ends_with('c') =>
                {
                    Some((*inner).to_string())
                }
                _ => Some(words.join(" ")),
            }
        }
        _ => None,
    }
}

/// Claude 的 `tool` 是工具名，`details` 是 tool_input；Codex 的 `tool` 是请求方法名，`details` 是参数。
pub(super) fn subject_of(snapshot: &ApprovalSnapshot, root: &Path) -> ApprovalSubject {
    let tool = snapshot.tool.as_str();
    let tool_name = if tool.contains("commandExecution") || tool.contains("exec") {
        "Bash".to_string()
    } else if tool.contains("fileChange") || tool.contains("applyPatch") {
        "Write".to_string()
    } else {
        tool.to_string()
    };
    let details = &snapshot.details;
    ApprovalSubject {
        tool_name,
        command: text_of(&details["command"]),
        file_path: ["file_path", "path"]
            .iter()
            .find_map(|key| details[*key].as_str())
            .map(Into::into),
        project_root: root.to_path_buf(),
    }
}

/// 用户是否允许大脑自行放行这类操作：命令看“只读、测试、构建”，写文件看“项目内写文件”。
/// 其他工具不受这两个开关限制，由大脑判断风险。
pub(super) fn autonomy_allows(autonomy: &super::config::Autonomy, tool_name: &str) -> bool {
    match tool_name {
        "Bash" => autonomy.readonly_commands,
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => autonomy.project_writes,
        _ => true,
    }
}

/// 播报用的一句话：谁想做什么。
pub(super) fn summarize(subject: &ApprovalSubject) -> String {
    let what = subject
        .command
        .clone()
        .or_else(|| subject.file_path.as_ref().map(|p| p.display().to_string()))
        .unwrap_or_else(|| subject.tool_name.clone());
    let short: String = what.chars().take(60).collect();
    if what.chars().count() > 60 {
        format!("{short}…")
    } else {
        short
    }
}
