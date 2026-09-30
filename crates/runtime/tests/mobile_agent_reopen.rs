use canvas::{AgentProfile, CanvasNodeKind, CanvasRect};
use runtime::{AgentTaskState, AgentTaskUpdate, Runtime};

fn report(runtime: &mut Runtime, id: u64, state: AgentTaskState) {
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id: id,
        profile: AgentProfile::Claude,
        provider_event: "Stop".into(),
        to: state,
        task_title: Some("保留的任务".into()),
        session_id: Some("saved-session".into()),
    });
}

/// 旧运行会话不自动迁移，只能在结束后显式切换受控模式并保留原卡与历史绑定。
///
/// 数据构造：两个旧 Claude echo PTY，80×24 为标准终端；目标卡保存 saved-session，另一张卡不参与切换。
/// 执行过程：目标 Thinking 时重开被拒绝 → Done 后错误会话被拒绝 → 正确会话切换 → 保存并恢复快照。
/// 预期结果：忙碌时原 PTY 保留；成功后目标无执行器且未安排任务，另一卡仍运行；身份、别名、历史和新模式持久保留。
#[test]
fn legacy_agent_switches_only_after_completion_and_keeps_history() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let bounds = CanvasRect::new(0.0, 0.0, 640.0, 400.0);
    let id = runtime
        .spawn_agent_with_alias(AgentProfile::Claude, Some("旧卡片".into()), bounds, 80, 24)
        .expect("旧卡");
    let other = runtime
        .spawn_agent(AgentProfile::Claude, bounds, 80, 24)
        .expect("另一卡");
    report(&mut runtime, id, AgentTaskState::Thinking);
    assert!(
        runtime
            .enable_mobile_agent_control(id, "saved-session")
            .is_err(),
        "运行任务不得迁移"
    );
    assert_eq!(runtime.live_len(), 2, "拒绝时不能中断任一终端");
    assert!(
        matches!(&runtime.document.node(id).expect("旧卡").kind,CanvasNodeKind::Agent(spec) if !spec.mobile_control),
        "不能自动启用手机控制"
    );
    report(&mut runtime, id, AgentTaskState::Done);
    assert!(
        runtime
            .enable_mobile_agent_control(id, "wrong-session")
            .is_err(),
        "过时页面不能重开其他会话"
    );
    runtime
        .enable_mobile_agent_control(id, "saved-session")
        .expect("任务结束后可显式启用");
    assert!(
        runtime.node_view(id).is_none(),
        "目标旧 PTY 必须结束，不能悄悄启动新任务"
    );
    assert!(runtime.node_view(other).is_some(), "另一终端必须保留");
    assert_eq!(
        runtime.agent_session_id(id).as_deref(),
        Some("saved-session"),
        "原历史绑定必须保留"
    );
    assert_eq!(
        runtime.agent_task_title(id).as_deref(),
        Some("保留的任务"),
        "旧任务名称不应消失"
    );
    assert!(
        runtime
            .enable_mobile_agent_control(id, "saved-session")
            .is_err(),
        "重复启用不能重复关闭或启动"
    );
    let snapshot = runtime.collect_canvas_snapshot();
    let mut restored = Runtime::new();
    restored.restore_canvas_snapshot(snapshot);
    assert!(
        matches!(&restored.document.node(id).expect("原卡身份").kind,CanvasNodeKind::Agent(spec) if spec.mobile_control && spec.alias.as_deref()==Some("旧卡片")),
        "模式和名称必须随原卡保存"
    );
    assert!(
        matches!(&restored.document.node(other).expect("另一卡").kind,CanvasNodeKind::Agent(spec) if !spec.mobile_control),
        "另一卡不能一起迁移"
    );
}

/// 旧 CLI 已退出但未发结束 Hook 时，以真实 PTY 退出为准，不能永久卡在过时 Thinking。
///
/// 数据构造：一个 80×24 echo PTY 和 session=saved-session；最后一次 Hook 为 Thinking，随后 PTY 明确 Exited(1)。
/// 执行过程：登记正在运行 → 注入真实 PTY 出口事件 → drain → 显式启用手机控制。
/// 预期结果：活跃时不允许切换，退出后允许；原历史保留，不能因此启动新任务。
#[test]
fn exited_legacy_pty_can_reopen_without_a_final_hook() {
    let (send, recv) = std::sync::mpsc::channel();
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(move |_, _, _, tx| {
        send.send(tx.clone()).expect("PTY 事件通道");
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let id = runtime
        .spawn_agent(
            AgentProfile::Claude,
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            80,
            24,
        )
        .expect("旧卡");
    report(&mut runtime, id, AgentTaskState::Thinking);
    assert!(
        runtime
            .enable_mobile_agent_control(id, "saved-session")
            .is_err(),
        "退出前不能迁移"
    );
    recv.recv()
        .expect("事件发送端")
        .send(terminal::PtyEvent::Exited { code: Some(1) })
        .expect("退出通知");
    runtime.drain_all();
    runtime
        .enable_mobile_agent_control(id, "saved-session")
        .expect("已经退出不能永久等待不存在的 Stop Hook");
    assert!(runtime.node_view(id).is_none(), "不得启动替代任务");
    assert_eq!(
        runtime.agent_session_id(id).as_deref(),
        Some("saved-session"),
        "退出不能删除可恢复历史"
    );
}
