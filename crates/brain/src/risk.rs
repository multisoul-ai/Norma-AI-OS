//! 审批请求的描述，以及大脑自己的权限闸门用到的命令、路径判断。
//!
//! 审批本身是否危险由大脑判断，这里不再有“高风险规则”。闸门限制的是大脑自己
//! 能执行什么（只许单条 `norma brain …` 和项目内只读），与用户的审批无关。

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
