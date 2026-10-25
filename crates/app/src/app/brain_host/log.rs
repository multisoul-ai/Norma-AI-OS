//! 大脑自行处理过的事项（自主动作）的类型，及其“以后这类先问我”的类别键。

use brain::risk::ApprovalSubject;
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AutoKind {
    Approved,
    Denied,
    Assigned,
    /// 用户在概览里批准了大脑的提议，由 App 派发。
    UserDispatched,
}

impl AutoKind {
    /// 持久化用的稳定键。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Denied => "denied",
            Self::Assigned => "assigned",
            Self::UserDispatched => "user",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AutoAction {
    pub at: SystemTime,
    pub kind: AutoKind,
    pub agent: String,
    pub node_id: u64,
    /// 做了什么：命令、路径或任务文字。
    pub summary: String,
    /// 大脑给的理由。
    pub reason: String,
    /// “以后这类先问我”要加入的类别键。
    pub ask_key: String,
}

/// 派后续任务这一类的键。
pub(crate) const ASSIGN_KEY: &str = "assign";

/// 审批的类别键：命令按程序名（`Bash:cargo`），写文件按工具名（`Write`）。
/// 用户点“以后这类先问我”后，同类审批不再由大脑放行。
pub(crate) fn ask_key(subject: &ApprovalSubject) -> String {
    match subject.tool_name.as_str() {
        "Bash" => {
            let program = subject
                .command
                .as_deref()
                .and_then(|command| command.split_whitespace().next())
                .map(|word| word.rsplit('/').next().unwrap_or(word).to_ascii_lowercase())
                .unwrap_or_default();
            format!("Bash:{program}")
        }
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => "Write".to_string(),
        other => other.to_string(),
    }
}
