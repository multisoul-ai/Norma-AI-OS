use super::*;

/// spawn_agent_with_alias 应把 project agent 名称写入 AgentSpec.alias，同时默认 spawn_agent 仍保持 alias=None。
///
/// 数据构造（含关键数值推导）：
///   bounds = CanvasRect::new(0, 0, 640, 400)
///   aliased profile = Codex，alias = "Code Reviewer"
///   default profile = Claude，alias = None
///   override command = /bin/sh -lc "printf AGENT_OK; exit"（测试替身，避免依赖真实 agent CLI）
///   cols=80, rows=24
///   两次 production spawn 后 document 应有 2 个 Agent 节点
///
/// 执行过程：
///   1. set_test_agent_spawn_command(/bin/sh -lc "printf AGENT_OK; exit")
///      → 让 production spawn_agent_with_alias/spawn_agent 复用测试命令而不是本机 agent CLI
///   2. spawn_agent_with_alias(Codex, Some("Code Reviewer"), ...)
///      → document.add_node(AgentSpec{profile:Codex, alias:Some(...)})
///   3. spawn_agent(Claude, ...)
///      → 保持既有行为 document.add_node(AgentSpec{profile:Claude, alias:None})
///   4. 分别读取两个 document node 的 AgentSpec
///
/// 预期结果：
///   - 正断言：aliased 节点 alias == Some("Code Reviewer")
///   - 正断言：默认 spawn_agent 节点 alias == None
///   - 负断言：aliased 节点 alias 不应为 None，否则 project agent 名称无法恢复到卡片标题
#[test]
fn test_spawn_agent_with_alias_records_alias_and_default_spawn_keeps_none() {
    let mut rt = Runtime::new();
    let bounds = CanvasRect::new(0.0, 0.0, 640.0, 400.0);
    let override_command = PtyCommand::new("/bin/sh").args(["-lc", "printf AGENT_OK; exit"]);
    rt.set_test_agent_spawn_command(override_command);

    let aliased_id = rt
        .spawn_agent_with_alias(
            AgentProfile::Codex,
            Some("Code Reviewer".to_string()),
            bounds,
            80,
            24,
        )
        .expect("spawn_agent_with_alias 应成功创建 aliased agent");
    let default_id = rt
        .spawn_agent(AgentProfile::Claude, bounds, 80, 24)
        .expect("spawn_agent 应保持既有默认 agent 创建能力");

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
        "spawn_agent_with_alias 应把 project agent 名称写入 alias"
    );
    assert!(
        aliased_agent.alias.is_some(),
        "aliased agent alias 不应为 None，否则卡片标题无法使用定义名称"
    );
    assert_eq!(
        default_agent.alias, None,
        "既有 spawn_agent 应继续保持 alias=None，避免影响默认 Agent 创建"
    );

    rt.kill_all();
}

/// 同一个 Agent 定义重复激活时应复用唯一活跃卡片，而不是再启动一个 CLI。
///
/// 数据构造（含关键数值推导）：
///   definition_id = 7，对应同一条项目 Agent 定义
///   两次激活请求数量 = 2，但允许的活跃卡片上限 = 1
///   bounds = 640×400，grid = 80×24；尺寸不影响身份去重
///   override command = /bin/sh -lc "sleep 1"，避免依赖真实 Codex CLI
///
/// 执行过程：
///   1. spawn_agent_for_definition(7, "Alan") → 创建绑定 definition_id=7 的卡片
///   2. 再次 spawn_agent_for_definition(7, "Alan") → 扫描 document 并返回既有 node_id
///   3. 读取 document 节点数量和 AgentSpec 身份
///
/// 预期结果：
///   - 正断言：两次调用返回相同 node_id，卡片保存 definition_id=7
///   - 正断言：document 中只有 1 张卡片
///   - 负断言：第二次调用不得产生第 2 张同定义卡片
#[test]
fn test_spawn_agent_for_definition_reuses_single_active_card() {
    let mut runtime = Runtime::new();
    let bounds = CanvasRect::new(0.0, 0.0, 640.0, 400.0);
    runtime.set_test_agent_spawn_command(PtyCommand::new("/bin/sh").args(["-lc", "sleep 1"]));

    let first_id = runtime
        .spawn_agent_for_definition(AgentProfile::Codex, 7, "Alan".to_string(), bounds, 80, 24)
        .expect("首次激活 definition_id=7 应创建 Alan 卡片");
    let second_id = runtime
        .spawn_agent_for_definition(AgentProfile::Codex, 7, "Alan".to_string(), bounds, 80, 24)
        .expect("重复激活 definition_id=7 应复用 Alan 卡片");
    let node = runtime
        .document
        .node(first_id)
        .expect("首次创建的 Alan 节点应继续存在");
    let CanvasNodeKind::Agent(spec) = &node.kind else {
        panic!("definition_id=7 应绑定 Agent 卡片，实际是 {:?}", node.kind);
    };

    assert_eq!(
        second_id, first_id,
        "同一定义的第二次激活必须返回既有 node_id"
    );
    assert_eq!(
        runtime.document.nodes().len(),
        1,
        "两次激活同一定义后 document 仍只能有 1 张活跃卡片"
    );
    assert_eq!(
        spec.definition_id,
        Some(7),
        "唯一活跃卡片必须持久化绑定 definition_id=7"
    );
    assert_ne!(
        runtime.document.nodes().len(),
        2,
        "重复激活不得产生第 2 张同定义 Agent 卡片"
    );

    runtime.kill_all();
}

/// iOS 从项目定义创建 Agent 时，卡片应同时保留定义身份与手机控制模式。
///
/// 数据构造（含关键数值推导）：
///   definition_id = 17，名称 = Alan；bounds = 640×400，grid = 80×24。
///   测试命令 sleep 1 只提供可回收 PTY，不连接真实 Provider。
///
/// 执行过程：
///   1. 通过项目定义专用的手机入口创建 Codex 卡片。
///   2. 从 document 读取同一 node_id 的 AgentSpec。
///   3. 回收所有测试子进程。
///
/// 预期结果：
///   - 正断言：definition_id 精确为 17，alias 精确为 Alan。
///   - 正断言：mobile_control=true，手机后续可以在原卡片发起会话。
///   - 负断言：卡片不能退化为没有项目定义身份的普通手机卡片。
#[test]
fn test_spawn_mobile_agent_for_definition_preserves_control_and_identity() {
    let mut runtime = Runtime::new();
    let bounds = CanvasRect::new(0.0, 0.0, 640.0, 400.0);
    runtime.set_test_agent_spawn_command(PtyCommand::new("/bin/sh").args(["-lc", "sleep 1"]));

    let node_id = runtime
        .spawn_mobile_agent_card_for_definition(
            AgentProfile::Codex,
            17,
            "Alan".to_string(),
            bounds,
            80,
            24,
        )
        .expect("iOS 应能从 definition_id=17 创建可控 Alan 卡片");
    let node = runtime
        .document
        .node(node_id)
        .expect("创建结果必须指向仍存在的 Agent 节点");
    let CanvasNodeKind::Agent(spec) = &node.kind else {
        panic!("项目定义创建结果必须是 Agent 卡片，实际是 {:?}", node.kind);
    };

    assert_eq!(
        spec.definition_id,
        Some(17),
        "iOS 智能卡片必须保留项目 definition_id"
    );
    assert_eq!(
        spec.alias.as_deref(),
        Some("Alan"),
        "iOS 智能卡片必须显示项目定义名称"
    );
    assert!(
        spec.mobile_control,
        "项目定义卡片仍必须开启手机控制，不能退化为桌面旧会话模式"
    );
    assert_ne!(
        spec.definition_id, None,
        "项目定义身份不能在手机卡片创建路径中丢失"
    );

    runtime.kill_all();
}

/// 活跃 Agent 定义改名时应原位同步唯一卡片标题，不改变 definition 绑定或节点数量。
///
/// 数据构造（含关键数值推导）：
///   definition_id = 11
///   original name = "Alan"，new name = "Robin"
///   active cards = 1，重命名后仍应为 1
///   override command = /bin/sh -lc "sleep 1"，避免依赖真实 Agent CLI
///
/// 执行过程（逐步说明系统如何处理）：
///   1. spawn_agent_for_definition(11, "Alan") 创建绑定卡片
///   2. sync_agent_definition_name(11, "Robin") 原位更新 AgentSpec.alias
///   3. 读取同一 node_id 的 AgentSpec 与 document 节点数量
///
/// 预期结果：
///   - 正断言：同步返回 true，alias 变为 Robin
///   - 正断言：definition_id 仍为 11，document 仍只有 1 张卡片
///   - 负断言：旧 alias Alan 不应残留，也不应新增第 2 张卡片
#[test]
fn test_sync_agent_definition_name_renames_active_card_in_place() {
    let mut runtime = Runtime::new();
    let bounds = CanvasRect::new(0.0, 0.0, 640.0, 400.0);
    runtime.set_test_agent_spawn_command(PtyCommand::new("/bin/sh").args(["-lc", "sleep 1"]));
    let node_id = runtime
        .spawn_agent_for_definition(AgentProfile::Codex, 11, "Alan".to_string(), bounds, 80, 24)
        .expect("definition_id=11 应成功创建 Alan 卡片");

    let did_sync = runtime.sync_agent_definition_name(11, "Robin");
    let node = runtime
        .document
        .node(node_id)
        .expect("重命名后原 node_id 应继续存在");
    let CanvasNodeKind::Agent(spec) = &node.kind else {
        panic!(
            "definition_id=11 应继续绑定 Agent 卡片，实际是 {:?}",
            node.kind
        );
    };

    assert!(did_sync, "活跃 definition_id=11 的名称同步应返回 true");
    assert_eq!(
        spec.alias.as_deref(),
        Some("Robin"),
        "活跃卡片标题应原位同步为新名称 Robin"
    );
    assert_eq!(
        spec.definition_id,
        Some(11),
        "重命名不得改变卡片的 definition_id 绑定"
    );
    assert_eq!(
        runtime.document.nodes().len(),
        1,
        "重命名后 document 仍只能包含原有 1 张活跃卡片"
    );
    assert_ne!(
        spec.alias.as_deref(),
        Some("Alan"),
        "旧名称 Alan 不应残留在活跃卡片标题中"
    );

    runtime.kill_all();
}
