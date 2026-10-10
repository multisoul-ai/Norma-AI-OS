use canvas::{AgentProfile, AgentSpec, CanvasRect, Document, NodeKind};
use norma::workspace_dispatch::WorkspaceTargetError;
use runtime::Runtime;

fn add_card(runtime: &mut Runtime) -> u64 {
    runtime.document.add_node(
        NodeKind::Agent(AgentSpec {
            profile: AgentProfile::OpenCode,
            alias: Some("Alan".into()),
            definition_id: None,
            mobile_control: false,
        }),
        CanvasRect::new(0.0, 0.0, 640.0, 400.0),
    )
}

/// 场景：恢复原卡保留持久身份但使旧执行指纹失效，删除重建不继承旧卡身份。
/// 数据构造：工作空间 workspace、区域 area；初始路由代次 7；两次空文档均从 node_id=1 起步。
/// 执行过程：取目标 → 同 Runtime 恢复快照 → 复核旧指纹 → 删除并重建同编号卡片 → 再复核。
/// 预期结果：原卡恢复保持 card_instance_id；旧代次与旧卡分别被拒绝；全程零 PTY、零输入。
#[test]
fn restore_keeps_card_identity_but_delete_rebuild_never_accepts_old_target() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    let node = add_card(&mut runtime);
    let original = runtime
        .workspace_card_target("workspace", "area", 7, node)
        .expect("原卡应可绑定目标");
    assert_eq!(
        runtime.validate_workspace_card_target("workspace", "area", 7, &original),
        Ok(()),
        "当前身份应通过只读复核"
    );
    let snapshot = runtime.collect_canvas_snapshot();
    runtime.restore_canvas_snapshot(snapshot);
    let restored = runtime
        .workspace_card_target("workspace", "area", 7, node)
        .expect("恢复原卡应可绑定");
    assert_eq!(
        restored.card, original.card,
        "恢复同一卡片必须保留持久目标引用"
    );
    assert_ne!(
        restored.execution.runtime_instance_id, original.execution.runtime_instance_id,
        "恢复必须换 Runtime 代次"
    );
    assert_eq!(
        runtime.validate_workspace_card_target("workspace", "area", 7, &original),
        Err(WorkspaceTargetError::RuntimeChanged),
        "旧执行指纹不能因 node_id 与卡片身份相同而继续有效"
    );
    assert!(runtime.remove_node(node), "应删除恢复的原卡");
    runtime.document = Document::new();
    let replacement = add_card(&mut runtime);
    assert_eq!(replacement, node, "边界数据需复用相同 node_id");
    assert_eq!(
        runtime.validate_workspace_card_target("workspace", "area", 7, &restored),
        Err(WorkspaceTargetError::CardChanged),
        "同名同编号新卡不能接收原卡旧请求"
    );
    assert_ne!(
        runtime
            .document
            .node(replacement)
            .expect("新卡应存在")
            .card_instance_id,
        original.card.card_instance_id,
        "彻底删除后新卡必须获得新身份"
    );
    assert_eq!(runtime.live_len(), 0, "生成与复核目标不得创建任何 PTY");
}

/// 场景：没有 Provider run_id 的普通卡片也冻结真实 PTY 会话，并拒绝过时指纹。
/// 数据构造：一个 OpenCode 记录型 PTY，标准网格 80×24；首个 TerminalSessionId=1，路由代次=7。
/// 执行过程：启动原卡 → 捕获目标 → 改名并复核 → 分别尝试错误区域、代次和旧会话 → 核对输入记录。
/// 预期结果：捕获 PTY session=1；改名仍有效；其他身份与会话被拒绝；卡片收到零字节输入。
#[test]
fn ordinary_card_captures_terminal_session_and_identity_checks_are_read_only() {
    let writes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = writes.clone();
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::recording(
            tx,
            observed.clone(),
        )))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::OpenCode,
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            80,
            24,
        )
        .expect("应启动普通 OpenCode 卡片");
    let target = runtime
        .workspace_card_target("workspace", "area", 7, node)
        .expect("普通卡应可冻结目标");
    assert_eq!(
        target.execution.terminal_session_id,
        Some(1),
        "即使没有 Provider run_id，也必须冻结实际 PTY session"
    );
    assert_eq!(
        target.execution.run_id, None,
        "普通 OpenCode 卡片不能伪造受控 run_id"
    );
    let NodeKind::Agent(spec) = &mut runtime.document.node_mut(node).expect("原卡应存在").kind
    else {
        panic!("原卡必须为 Agent");
    };
    spec.alias = Some("Simon".into());
    assert_eq!(
        runtime.validate_workspace_card_target("workspace", "area", 7, &target),
        Ok(()),
        "改名不能改变持久目标"
    );
    assert_eq!(
        runtime.validate_workspace_card_target("workspace", "other-area", 7, &target),
        Err(WorkspaceTargetError::RouteChanged),
        "相同目录另一画布不得复用原地址"
    );
    assert_eq!(
        runtime.validate_workspace_card_target("workspace", "area", 8, &target),
        Err(WorkspaceTargetError::RouteChanged),
        "路由从 7 更新为 8 后旧请求必须失效"
    );
    let mut stale_session = target.clone();
    stale_session.execution.terminal_session_id = Some(2);
    assert_eq!(
        runtime.validate_workspace_card_target("workspace", "area", 7, &stale_session),
        Err(WorkspaceTargetError::SessionChanged),
        "其他 PTY session 不能路由至原卡"
    );
    assert!(
        runtime.bind_discovered_agent_session(node, "old-session"),
        "应能绑定原卡 Provider 会话"
    );
    let original = runtime
        .workspace_card_target("workspace", "area", 7, node)
        .expect("原会话应可冻结");
    assert!(
        runtime.bind_discovered_agent_session(node, "new-session"),
        "显式替换应更新 Provider 会话"
    );
    assert_eq!(
        runtime.validate_workspace_card_target("workspace", "area", 7, &original),
        Err(WorkspaceTargetError::SessionChanged),
        "Provider 会话替换后旧请求不能接收"
    );
    assert!(
        writes.lock().expect("应读取实际 PTY 记录").is_empty(),
        "所有身份查询和拒绝都不得产生卡片输入"
    );
    assert_eq!(
        runtime.live_len(),
        1,
        "查询不能启动第二个 PTY，也不能停止原 PTY"
    );
}

/// 场景：同一 PTY 内启动新的 Provider 会话，也必须拒绝上一运行的冻结指纹。
/// 数据构造：原生 Codex 记录型 PTY，80×24；两个可信 SessionStart 分别为 session-a 与 session-b。
/// 执行过程：接收 a 的 Hook → 冻结目标 → 接收 b 的 Hook → 复核旧与新目标 → 查看字节记录。
/// 预期结果：卡片与 PTY 身份保留，run/session 更新；旧运行拒绝，新运行通过；复核不发任何输入。
#[test]
fn provider_session_replacement_rejects_old_run_without_touching_the_terminal() {
    use norma::agent_hook::{parse_hook_event, HookSource};
    let writes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = writes.clone();
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::recording(
            tx,
            observed.clone(),
        )))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::Codex,
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            80,
            24,
        )
        .expect("应启动原生 Codex 卡片");
    let event = |session| {
        parse_hook_event(
            HookSource::Codex,
            node,
            serde_json::json!({"hook_event_name": "SessionStart", "session_id": session}),
        )
        .expect("应构造可信会话 Hook")
    };
    assert!(
        runtime.observe_mobile_hook(&event("session-a")),
        "原会话 Hook 应被接收"
    );
    let original = runtime
        .workspace_card_target("workspace", "area", 7, node)
        .expect("原运行应可冻结");
    assert_eq!(
        original.execution.session_id.as_deref(),
        Some("session-a"),
        "冻结目标必须记录已确认的 Provider 会话"
    );
    assert!(
        original.execution.run_id.is_some(),
        "原生 Codex 应有独立运行代次"
    );
    assert!(
        runtime.observe_mobile_hook(&event("session-b")),
        "新会话 Hook 应被接收"
    );
    let current = runtime
        .workspace_card_target("workspace", "area", 7, node)
        .expect("新运行应可冻结");
    assert_eq!(current.card, original.card, "新会话仍属于原卡片");
    assert_eq!(
        current.execution.terminal_session_id, original.execution.terminal_session_id,
        "会话切换没有重建原 PTY"
    );
    assert_ne!(
        current.execution.run_id, original.execution.run_id,
        "新的 Provider 会话必须换运行代次"
    );
    assert_eq!(
        runtime.validate_workspace_card_target("workspace", "area", 7, &original),
        Err(WorkspaceTargetError::RunChanged),
        "旧运行不能投递至同卡的新会话"
    );
    assert_eq!(
        runtime.validate_workspace_card_target("workspace", "area", 7, &current),
        Ok(()),
        "新冻结目标应正确定位当前会话"
    );
    assert!(
        writes.lock().expect("应读取实际 PTY 记录").is_empty(),
        "只读复核不能给任一会话发送输入"
    );
}
