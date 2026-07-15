use canvas::NodeId;
use runtime::Runtime;

pub(crate) fn activate_spawned_node(runtime: &mut Runtime, node_id: NodeId) {
    runtime.selection.select_only(node_id);
    runtime.focus.focus_node(node_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use canvas::{AgentProfile, AgentSpec, CanvasNodeKind, CanvasRect};
    use runtime::InputMode;

    /// 新建 Claude/Codex Agent 后应立即选中并聚焦，保证输入不会被画布层吞掉。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   document.next_id 初始为 1
    ///   Claude Agent 先加入 document，因此 claude_id = 1
    ///   Codex Agent 后加入 document，因此 codex_id = 2
    ///   两者共用 activate_spawned_node 路径，模拟菜单/工具栏新建 Agent 后的 app 状态收口
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. activate_spawned_node(claude_id) → 应 select_only(1) 且 focus_node(1)
    ///   2. activate_spawned_node(codex_id) → 应 select_only(2) 且 focus_node(2)
    ///   3. 查询 selection 与 focus，确认旧 Claude 不再保持选中/聚焦
    ///
    /// 预期结果：
    ///   - 正断言：Claude 激活后 selection/focus 都指向 claude_id
    ///   - 正断言：Codex 激活后 selection/focus 都指向 codex_id
    ///   - 负断言：Codex 激活后 Claude 不应仍被选中或聚焦
    ///   - 负断言：Agent 激活后焦点不应停留在 CanvasSelect
    #[test]
    fn test_spawned_agent_activation_selects_and_focuses_claude_and_codex_nodes() {
        let mut runtime = Runtime::new();
        let claude_id = runtime.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Claude,
                alias: None,
                definition_id: None,
            }),
            CanvasRect::new(0.0, 0.0, 640.0, 360.0),
        );
        let codex_id = runtime.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Codex,
                alias: None,
                definition_id: None,
            }),
            CanvasRect::new(720.0, 0.0, 640.0, 360.0),
        );

        activate_spawned_node(&mut runtime, claude_id);

        assert!(
            runtime.selection.contains(claude_id),
            "Claude Agent 新建激活后应被选中，后续视觉和输入状态才能对齐"
        );
        assert_eq!(
            runtime.focus.mode(),
            InputMode::NodeFocused(claude_id),
            "Claude Agent 新建激活后应立即聚焦，否则键盘输入会被 CanvasSelect 忽略"
        );

        activate_spawned_node(&mut runtime, codex_id);

        assert!(
            runtime.selection.contains(codex_id),
            "Codex Agent 新建激活后应被选中"
        );
        assert!(
            !runtime.selection.contains(claude_id),
            "Codex Agent 新建激活后 Claude 不应仍保持选中，否则 selection 不是 select_only"
        );
        assert_eq!(
            runtime.focus.mode(),
            InputMode::NodeFocused(codex_id),
            "Codex Agent 新建激活后应立即聚焦，让 IME commit 和 Enter 都有目标节点"
        );
        assert_ne!(
            runtime.focus.mode(),
            InputMode::NodeFocused(claude_id),
            "Codex Agent 激活后焦点不应仍停留在 Claude Agent"
        );
        assert_ne!(
            runtime.focus.mode(),
            InputMode::CanvasSelect,
            "Agent 新建激活后焦点不应停留在 CanvasSelect，否则 codex/claude 输入会丢失"
        );
    }
}
