use super::*;

/// spawn_agent_with_command_and_alias 应把 project agent 名称写入 AgentSpec.alias，同时旧 spawn_agent_with_command 仍保持 alias=None。
///
/// 数据构造（含关键数值推导）：
///   bounds = CanvasRect::new(0, 0, 640, 400)
///   aliased profile = Codex，alias = "Code Reviewer"
///   default profile = Claude，alias = None
///   command = /bin/sh -lc "printf AGENT_OK; exit"（测试替身，避免依赖真实 agent CLI）
///   两次 spawn 后 document 应有 2 个 Agent 节点
///
/// 执行过程：
///   1. spawn_agent_with_command_and_alias(Codex, Some("Code Reviewer"), ...)
///      → document.add_node(AgentSpec{profile:Codex, alias:Some(...)})
///   2. spawn_agent_with_command(Claude, ...)
///      → 保持既有行为 document.add_node(AgentSpec{profile:Claude, alias:None})
///   3. 分别读取两个 document node 的 AgentSpec
///
/// 预期结果：
///   - 正断言：aliased 节点 alias == Some("Code Reviewer")
///   - 正断言：默认 spawn_agent_with_command 节点 alias == None
///   - 负断言：aliased 节点 alias 不应为 None，否则 project agent 名称无法恢复到卡片标题
#[test]
fn test_spawn_agent_with_alias_records_alias_and_default_spawn_keeps_none() {
    let mut rt = Runtime::new();
    let bounds = CanvasRect::new(0.0, 0.0, 640.0, 400.0);
    let aliased_command = PtyCommand::new("/bin/sh").args(["-lc", "printf AGENT_OK; exit"]);
    let default_command = PtyCommand::new("/bin/sh").args(["-lc", "printf AGENT_OK; exit"]);

    let aliased_id = rt
        .spawn_agent_with_command_and_alias(
            AgentProfile::Codex,
            Some("Code Reviewer".to_string()),
            bounds,
            aliased_command,
            80,
            24,
        )
        .expect("spawn_agent_with_command_and_alias 应成功创建 aliased agent");
    let default_id = rt
        .spawn_agent_with_command(AgentProfile::Claude, bounds, default_command, 80, 24)
        .expect("spawn_agent_with_command 应保持既有默认 agent 创建能力");

    let aliased_node = rt
        .document
        .node(aliased_id)
        .expect("aliased agent node 应存在于 document");
    let default_node = rt
        .document
        .node(default_id)
        .expect("default agent node 应存在于 document");

    let CanvasNodeKind::Agent(aliased_agent) = &aliased_node.kind else {
        panic!(
            "aliased spawn 不应创建非 Agent 节点: {:?}",
            aliased_node.kind
        );
    };
    let CanvasNodeKind::Agent(default_agent) = &default_node.kind else {
        panic!(
            "default spawn 不应创建非 Agent 节点: {:?}",
            default_node.kind
        );
    };

    assert_eq!(
        aliased_agent.alias.as_deref(),
        Some("Code Reviewer"),
        "spawn_agent_with_command_and_alias 应把 project agent 名称写入 alias"
    );
    assert!(
        aliased_agent.alias.is_some(),
        "aliased agent alias 不应为 None，否则卡片标题无法使用定义名称"
    );
    assert_eq!(
        default_agent.alias, None,
        "既有 spawn_agent_with_command 应继续保持 alias=None，避免影响默认 Agent 创建"
    );

    rt.kill_all();
}
