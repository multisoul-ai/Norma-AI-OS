use super::*;

/// 删除必须绑定当前快照；原生卡可关闭，忙碌任务需明确确认，旧回合与准备态拒绝。
///
/// 数据构造：复用零 Provider 的 Claude 卡；80×24 为标准终端尺寸，仅预约启动而不执行。
/// 执行过程：空卡校验 → 预约启动 → 陈旧快照拒绝 → 模拟 run-a/turn-a → 无确认拒绝 → 确认通过 → 切换 turn-b。
/// 再将卡片改为原生模式 → 准确确认通过 → 无确认/旧运行拒绝 → 改为未支持的Cursor并拒绝。
/// 预期结果：受控和原生卡的准确回合均可关闭；旧快照/旧回合/准备态/未支持Provider拒绝，无删除或启动副作用。
#[test]
fn delete_guard_requires_current_turn_and_explicit_busy_confirmation() {
    let (mut runtime, node) = launch_tests::runtime_with_card();
    let idle = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        runtime.validate_mobile_card_deletion(&idle, false),
        Ok(()),
        "空卡可删除"
    );
    let launch = runtime
        .mobile_agent_launch(node, Path::new("unused"), 80, 24)
        .expect("仅预约");
    assert!(
        runtime.validate_mobile_card_deletion(&idle, true).is_err(),
        "准备启动之后旧空闲卡失效"
    );
    let preparing = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        runtime.validate_mobile_card_deletion(&preparing, true),
        Err("state_not_ready"),
        "未绑定会话不能确认关闭"
    );
    let run = runtime.mobile_runs.get_mut(&node).expect("预约");
    run.run_id = Some("run-a".into());
    run.session_id = Some("session-a".into());
    run.prompt_id = Some("turn-a".into());
    runtime
        .events
        .update_agent_task_state(node, crate::AgentTaskState::Thinking, "test".into());
    let working = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        working.turn_id.as_deref(),
        Some("turn-a"),
        "快照必须携带准确回合"
    );
    assert_eq!(
        runtime.validate_mobile_card_deletion(&working, false),
        Err("confirmation_required"),
        "不能默认中止运行任务"
    );
    assert_eq!(
        runtime.validate_mobile_card_deletion(&working, true),
        Ok(()),
        "用户可确认关闭准确回合"
    );
    runtime.mobile_runs.get_mut(&node).expect("运行").prompt_id = Some("turn-b".into());
    assert_eq!(
        runtime.validate_mobile_card_deletion(&working, true),
        Err("card_changed"),
        "旧回合确认不能删除新任务"
    );
    assert!(runtime.document.node(node).is_some(), "所有校验均无副作用");
    assert_eq!(runtime.live_len(), 0, "测试不得启动 Provider");
    if let CanvasNodeKind::Agent(spec) = &mut runtime.document.node_mut(node).expect("卡片").kind
    {
        spec.mobile_control = false;
    }
    let legacy = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        runtime.validate_mobile_card_deletion(&legacy, true),
        Ok(()),
        "原生卡删除应复用关卡流程，不要求改运行模式"
    );
    assert_eq!(
        runtime.validate_mobile_card_deletion(&legacy, false),
        Err("confirmation_required"),
        "原生忙碌卡仍需确认"
    );
    let mut stale = legacy.clone();
    stale.run_id = Some("stale-run".into());
    assert_eq!(
        runtime.validate_mobile_card_deletion(&stale, true),
        Err("card_changed"),
        "原生卡不能接受旧运行快照"
    );
    if let CanvasNodeKind::Agent(spec) = &mut runtime.document.node_mut(node).expect("卡片").kind
    {
        spec.profile = AgentProfile::Cursor;
    }
    let unsupported = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        runtime.validate_mobile_card_deletion(&unsupported, true),
        Err("unsupported_profile"),
        "首版仅允许管理Codex与Claude"
    );
    drop(launch);
}

/// 确认关闭受控任务后，现有 Runtime 关闭路径必须撤销所有审批句柄并回收终端。
///
/// 数据构造：80×24 echo 终端、run-a/session-a/turn-a 和一项 Claude Bash 审批；无真实 Provider。
/// 执行过程：附着终端 → 设置当前回合 → 接收审批 → 通过准确快照确认 → 关闭卡片 → 尝试迟到批准。
/// 预期结果：原卡与 live 均消失；持有旧句柄也不能批准，另一张空卡保留。
#[test]
fn confirmed_card_close_cancels_terminal_and_pending_approval() {
    let (mut runtime, node) = launch_tests::runtime_with_card();
    let retained = runtime
        .create_mobile_agent_card(
            AgentProfile::Codex,
            None,
            None,
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
        )
        .expect("保留卡");
    let launch = runtime
        .mobile_agent_launch(node, Path::new("unused"), 80, 24)
        .expect("预约");
    let factory: crate::PtyFactory =
        Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx))));
    let live = TermLive::spawn_with_factory(
        launch.generation,
        launch.command("session-a", "run-a"),
        80,
        24,
        &factory,
    )
    .expect("echo 终端");
    runtime
        .attach_mobile_live(launch, "session-a".into(), "run-a".into(), live)
        .expect("附着");
    let dir = tempfile::tempdir().expect("审批审计库");
    let run = runtime.mobile_runs.get_mut(&node).expect("运行");
    run.prompt_id = Some("turn-a".into());
    run.claude = Some(
        claude::ClaudeApprovals::new(storage::ApprovalAuditStore::new(
            dir.path().join("audit.db"),
        ))
        .expect("审计"),
    );
    runtime.events.update_agent_task_state(
        node,
        crate::AgentTaskState::Thinking,
        "UserPromptSubmit".into(),
    );
    let event = norma::agent_hook::parse_hook_event(norma::agent_hook::HookSource::Claude, node,
        serde_json::json!({"hook_event_name":"PermissionRequest", "session_id":"session-a", "prompt_id":"turn-a", "tool_name":"Bash"})).expect("审批事件");
    let approval = HookApproval::new(
        norma::agent_hook::HookApprovalRequest {
            run_id: "run-a".into(),
            event,
        },
        Instant::now(),
    )
    .expect("审批");
    assert!(
        runtime.accept_mobile_approval(approval.clone()),
        "准确审批应被接收"
    );
    let expected = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        runtime.validate_mobile_card_deletion(&expected, true),
        Ok(()),
        "允许明确关闭当前运行"
    );
    assert!(runtime.remove_node(node), "应关闭指定卡");
    assert!(
        !approval.decide(ApprovalDecision::Allow, Instant::now()),
        "关闭后旧审批不得允许执行"
    );
    assert!(runtime.document.node(node).is_none(), "原卡必须移除");
    assert!(runtime.document.node(retained).is_some(), "另一卡必须保留");
    assert_eq!(runtime.live_len(), 0, "终端不能遗留");
}
