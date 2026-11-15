//! 审批请求的描述。

use std::path::PathBuf;

/// 一次待审批的操作。字段来自 Agent hook 的 `PermissionRequest`。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalSubject {
    pub tool_name: String,
    pub command: Option<String>,
    pub file_path: Option<PathBuf>,
    pub project_root: PathBuf,
}
