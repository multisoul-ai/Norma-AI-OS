//! 高风险规则：只做词法判断，不做 IO。
//!
//! 文件路径必须由调用方先规范化并解析符号链接（`app/high_risk_guard` 负责）。

mod command;
mod path;
mod shell;

use std::path::PathBuf;

/// 一次待审批的操作。字段来自 Agent hook 的 `PermissionRequest`。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalSubject {
    pub tool_name: String,
    pub command: Option<String>,
    pub file_path: Option<PathBuf>,
    pub project_root: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HighRiskReason {
    GitPublish,
    Delete,
    Deploy,
    Secrets,
    Privilege,
    OutsideProject,
    RemoteScript,
    UnparseableCommand,
    UnknownTool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RiskClass {
    Normal,
    High(HighRiskReason),
}

/// 恰好一条、没有管道、重定向和组合的命令，返回其 argv；否则返回 `None`。
pub fn single_command(command: &str) -> Option<Vec<String>> {
    let mut segments = shell::parse(command)?;
    if segments.len() != 1 {
        return None;
    }
    let segment = segments.remove(0);
    (segment.redirects.is_empty() && !segment.argv.is_empty()).then_some(segment.argv)
}

pub fn is_secret_path(raw: &str) -> bool {
    path::is_secret(raw)
}

pub fn is_inside_project(root: &std::path::Path, raw: &str) -> bool {
    path::is_inside(root, raw)
}

const READ_ONLY_TOOLS: [&str; 8] = [
    "Read",
    "Glob",
    "Grep",
    "LS",
    "TodoWrite",
    "WebFetch",
    "WebSearch",
    "Task",
];
const WRITE_TOOLS: [&str; 4] = ["Write", "Edit", "MultiEdit", "NotebookEdit"];

pub fn risk_class(subject: &ApprovalSubject) -> RiskClass {
    let tool = subject.tool_name.as_str();
    let reason = if tool == "Bash" {
        match subject.command.as_deref() {
            Some(command) => command::classify(command, &subject.project_root),
            None => Some(HighRiskReason::UnparseableCommand),
        }
    } else if WRITE_TOOLS.contains(&tool) {
        write_tool_risk(subject)
    } else if READ_ONLY_TOOLS.contains(&tool) {
        read_tool_risk(subject)
    } else {
        Some(HighRiskReason::UnknownTool)
    };
    reason.map_or(RiskClass::Normal, RiskClass::High)
}

fn write_tool_risk(subject: &ApprovalSubject) -> Option<HighRiskReason> {
    let Some(file) = subject.file_path.as_deref().and_then(|p| p.to_str()) else {
        return Some(HighRiskReason::UnparseableCommand);
    };
    if path::is_secret(file) {
        Some(HighRiskReason::Secrets)
    } else if !path::is_inside(&subject.project_root, file) {
        Some(HighRiskReason::OutsideProject)
    } else {
        None
    }
}

fn read_tool_risk(subject: &ApprovalSubject) -> Option<HighRiskReason> {
    let file = subject.file_path.as_deref().and_then(|p| p.to_str())?;
    path::is_secret(file).then_some(HighRiskReason::Secrets)
}

#[cfg(test)]
#[path = "../tests/unit/risk_tests.rs"]
mod risk_tests;
