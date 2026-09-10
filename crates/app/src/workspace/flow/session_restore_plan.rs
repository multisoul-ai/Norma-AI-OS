use canvas::CanvasNodeKind;
use runtime::Runtime;

pub(super) fn legacy_discovery_node_ids(runtime: &Runtime) -> Vec<u64> {
    let agents = runtime
        .document
        .nodes()
        .iter()
        .filter_map(|node| match &node.kind {
            CanvasNodeKind::Agent(spec) => Some((node.id, spec.profile)),
            _ => None,
        })
        .collect::<Vec<_>>();
    agents
        .iter()
        .filter(|(node_id, profile)| {
            runtime.agent_session_id(*node_id).is_none()
                && agents.iter().filter(|(_, other)| other == profile).count() == 1
        })
        .map(|(node_id, _)| *node_id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use canvas::{AgentProfile, AgentSpec, CanvasRect};
    use storage::{NodeSession, NodeSessionKind};

    fn add_agent(runtime: &mut Runtime, profile: AgentProfile) -> u64 {
        runtime.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile,
                alias: None,
                definition_id: None,
                mobile_control: false,
            }),
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
        )
    }

    /// 同一 Provider 只要已有多张卡片，缺失绑定的卡片就不得猜测 workspace latest。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   Claude Agent = 2 张：node 1 已绑定 `session-1`，node 2 无绑定；
    ///   同 Provider 卡片总数 = 2，安全历史发现要求总数必须恰好为 1。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 创建两张 Claude 卡片；
    ///   2. hydrate 仅恢复 node 1 的精确绑定；
    ///   3. 生成允许走旧版 Provider history 的节点列表。
    ///
    /// 预期结果：
    ///   - 正断言：node 1 仍保留精确 session-1；
    ///   - 负断言：node 2 不得进入 legacy discovery 列表；
    ///   - 负断言：列表必须为空，避免 latest 属于 node 1 或第三个历史任务。
    #[test]
    fn legacy_discovery_is_disabled_when_provider_has_another_bound_card() {
        let mut runtime = Runtime::new();
        let bound = add_agent(&mut runtime, AgentProfile::Claude);
        let unbound = add_agent(&mut runtime, AgentProfile::Claude);
        runtime.hydrate_agent_session_bindings(&[NodeSession {
            node_id: bound,
            kind: NodeSessionKind::Agent,
            cwd: None,
            session_id: Some("session-1".to_string()),
        }]);

        let discovery_node_ids = legacy_discovery_node_ids(&runtime);

        assert_eq!(
            runtime.agent_session_id(bound).as_deref(),
            Some("session-1"),
            "node 1 的精确绑定应保留，不能为了兼容 node 2 而清空"
        );
        assert!(
            !discovery_node_ids.contains(&unbound),
            "同 Provider 已有另一张卡时，node 2 不得猜测 workspace latest"
        );
        assert!(
            discovery_node_ids.is_empty(),
            "两张 Claude 卡片中只有一张缺失绑定时也必须禁用 legacy discovery"
        );
    }
}
