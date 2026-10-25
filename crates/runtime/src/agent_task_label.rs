use crate::AgentTaskState;

impl AgentTaskState {
    /// 运行中途卡片能显示的实时动作，来自 hook 状态，不额外读文件。
    pub fn live_label(&self) -> Option<String> {
        match self {
            Self::Thinking => Some("思考中".into()),
            Self::RunningTool { tool_name } => {
                Some(match tool_name.as_deref().filter(|n| !n.is_empty()) {
                    Some(tool) => format!("调用 {tool}"),
                    None => "执行工具".into(),
                })
            }
            _ => None,
        }
    }
}
