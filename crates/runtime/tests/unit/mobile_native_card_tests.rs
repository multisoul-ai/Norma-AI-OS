use super::*;
use crate::{AgentTaskState, AgentTaskUpdate, NodeInput};
use norma::agent_hook::{parse_hook_event, HookSource};
use serde_json::json;
use std::time::Duration;

/// 旧桌面 Hook 没有回合身份时仍可结束任务，但不能伪造手机的准确回合结果。
///
/// 数据构造：1 张 Codex 原生卡、80×24 echo PTY；同一 session 的提交与 Stop 均无 turn_id。
/// 执行过程：原卡提交 → 旧 Stop → 带身份的新提交 → 缺身份 Stop → 准确 Stop。
/// 预期结果：旧桌面结束事件可处理；缺身份事件不能结束新回合，也不能生成手机成果记录。
#[test]
fn native_legacy_hooks_preserve_desktop_completion_without_mobile_receipts() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::Codex,
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .expect("原生卡");
    let submit = parse_hook_event(HookSource::Codex, node,
        json!({"hook_event_name":"UserPromptSubmit","session_id":"legacy-session","prompt":"desktop"}))
        .expect("旧提交");
    let stop = parse_hook_event(
        HookSource::Codex,
        node,
        json!({"hook_event_name":"Stop","session_id":"legacy-session","final_response":"done"}),
    )
    .expect("旧结束");
    assert!(runtime.observe_mobile_hook(&submit), "旧桌面提交应继续处理");
    assert!(
        runtime.observe_mobile_hook(&stop),
        "旧桌面 Stop 不应被手机观察器丢弃"
    );
    assert!(
        runtime.mobile_latest_turn_key(node).is_none(),
        "不能编造旧回合身份"
    );
    assert!(
        runtime.native_mobile_event(&stop).is_none(),
        "无准确回合不能存为手机成果"
    );

    let mut identified = submit.clone();
    identified.turn_id = Some("new-turn".into());
    assert!(
        runtime.observe_mobile_hook(&identified),
        "真实新回合应被记录"
    );
    assert!(
        !runtime.observe_mobile_hook(&stop),
        "缺回合身份的旧 Stop 不能结束新回合"
    );
    assert!(
        runtime.native_mobile_event(&stop).is_none(),
        "缺身份 Stop 不能生成新回合成果"
    );
    let mut finished = stop;
    finished.turn_id = Some("new-turn".into());
    assert!(
        runtime.observe_mobile_hook(&finished),
        "同回合 Stop 应被接受"
    );
    assert_eq!(
        runtime
            .native_mobile_event(&finished)
            .expect("准确成果")
            .2
            .expect("终态")
            .key
            .turn_id,
        "new-turn",
        "成果必须绑定真实回合"
    );
    assert!(
        !runtime.observe_mobile_hook(&finished),
        "重复 Stop 不能再次推进桌面状态"
    );
}

/// 原生 Codex 卡片接受手机输入，仍使用原 PTY；电脑草稿和旧会话不被覆盖。
///
/// 数据构造：一张原生 Codex 卡、80×24 echo PTY、session-a；正文首尾含空格，由Codex原生提交trim。
/// 执行过程：原生创建 → SessionStart → 手机Paste → 50ms时重绘 → 101ms时一次Enter → 准确回执 → 电脑输入。
/// 预期结果：mobile_control 持久标记仍为 false；只有一个 PTY；新草稿阻止手机发送；旧 run 被拒绝。
#[test]
fn native_card_mobile_message_uses_same_terminal_and_preserves_desktop_draft() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::Codex,
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .expect("原生卡");
    let start = parse_hook_event(
        HookSource::Codex,
        node,
        json!({"hook_event_name":"SessionStart","session_id":"session-a"}),
    )
    .expect("启动事件");
    assert!(runtime.observe_mobile_hook(&start), "原生 Hook 必须被接收");
    runtime.apply_agent_task_update_with_outcome(AgentTaskUpdate {
        node_id: node,
        profile: AgentProfile::Codex,
        provider_event: "SessionStart".into(),
        to: AgentTaskState::Ready,
        task_title: None,
        session_id: Some("session-a".into()),
    });
    let card = runtime.mobile_card_snapshots().remove(0);
    assert!(
        runtime
            .mobile_approval_snapshots(node)
            .expect("原生卡不能阻塞整个项目审批列表")
            .is_empty(),
        "空闲原生卡没有待审批事项"
    );
    let receipt = runtime
        .send_mobile_message(&card, "  phone message  ".into())
        .expect("原卡必须允许手机直接输入");
    assert!(receipt.try_recv().is_err(), "写入不等于 Provider 确认");
    // 与原桌面队列一致：100ms paste-to-Enter；101ms 保证跨过边界。
    runtime.live.get_mut(&node).expect("原终端").drain();
    let pasted = Instant::now();
    runtime.advance_mobile_messages(pasted);
    // 50ms = 100ms等待的一半；一次布局更新不能把原有Enter期限不断后移。
    runtime
        .live
        .get_mut(&node)
        .expect("原终端")
        .resize(crate::NodeResize::TermGrid { cols: 81, rows: 24 })
        .expect("模拟重绘");
    runtime.advance_mobile_messages(pasted + Duration::from_millis(50));
    runtime.advance_mobile_messages(pasted + Duration::from_millis(101));
    let submit = parse_hook_event(HookSource::Codex, node, json!({"hook_event_name":"UserPromptSubmit","session_id":"session-a","turn_id":"turn-a","prompt":"phone message"})).expect("准确提交事件");
    assert!(runtime.observe_mobile_hook(&submit), "同会话的提交必须确认");
    assert_eq!(
        receipt.try_recv().expect("回执").expect("成功")["turn_id"],
        "turn-a",
        "回执必须使用 Provider 回合"
    );
    assert_eq!(runtime.live_len(), 1, "手机输入不能另开终端");
    let stale_stop = parse_hook_event(
        HookSource::Codex,
        node,
        json!({"hook_event_name":"Stop","session_id":"session-a","turn_id":"old-turn"}),
    )
    .expect("旧回合结束事件");
    assert!(
        !runtime.observe_mobile_hook(&stale_stop),
        "旧回合结束不能改变当前任务状态"
    );
    assert!(
        matches!(&runtime.document.node(node).expect("卡片").kind, CanvasNodeKind::Agent(spec) if !spec.mobile_control),
        "不能把原生卡迁移到另一套运行模式"
    );
    runtime
        .send_node_input(node, NodeInput::Text("desktop draft".into()))
        .expect("电脑继续输入");
    let current = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        runtime.validate_mobile_message(&current, "another phone message"),
        Err("desktop_input_pending"),
        "手机不得覆盖电脑草稿"
    );
    let mut stale = current.clone();
    stale.run_id = Some("old-run".into());
    assert_eq!(
        runtime.validate_mobile_message(&stale, "stale message"),
        Err("card_changed"),
        "旧运行不能接收输入"
    );
    runtime.apply_agent_task_update_with_outcome(AgentTaskUpdate {
        node_id: node,
        profile: AgentProfile::Codex,
        provider_event: "PermissionRequest".into(),
        to: AgentTaskState::WaitingApproval {
            tool_name: Some("Bash".into()),
        },
        task_title: None,
        session_id: Some("session-a".into()),
    });
    let notices = runtime
        .mobile_approval_snapshots(node)
        .expect("原生确认提示");
    assert_eq!(notices.len(), 1, "等待确认应显示电脑处理提示");
    assert_eq!(
        notices[0].tool, "native_terminal",
        "提示必须表明原生终端来源"
    );
    assert!(
        notices[0].decisions.is_empty(),
        "没有真实审批协议不能伪造批准按钮"
    );
    assert_eq!(
        runtime.choose_mobile_approval(&notices[0].target, ApprovalDecision::Allow),
        Err("decision_unavailable"),
        "提示不能被当成可批准请求"
    );
    let current = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        current.can_interrupt,
        Some(true),
        "原生Codex已经接入准确终态回执"
    );
    assert_eq!(
        runtime.validate_mobile_interrupt(&current),
        Err("desktop_input_pending"),
        "Ctrl-C不能清除电脑端尚未提交的草稿"
    );
}

/// 原卡任务耗时必须来自准确回合事件；重复提交、旧回合结束不能重置计时。
///
/// 数据构造：1张Codex原卡、80×24内存PTY；session-a中的turn-a、turn-b代表两轮真实提交。
/// 执行过程：空卡 → 提交a → 重复a → 错误Stop → 正确Stop → 提交b → 无身份旧Hook。
/// 预期结果：空卡无耗时，运行快照不随查询变化；正确结束才冻结；新回合不能继承旧耗时。
#[test]
fn native_task_timing_tracks_exact_turn_without_snapshot_polling() {
    let mut runtime = Runtime::new_for_workspace("/tmp");
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::Codex,
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .expect("原卡");
    let hook = |name: &str, turn: &str| {
        parse_hook_event(
            HookSource::Codex,
            node,
            json!({"hook_event_name":name,"session_id":"session-a","turn_id":turn}),
        )
        .expect("事件")
    };
    assert!(
        runtime.mobile_card_snapshots()[0].task_timing.is_none(),
        "空卡不伪造起点"
    );
    assert!(
        runtime.observe_mobile_hook(&hook("UserPromptSubmit", "turn-a")),
        "准确提交"
    );
    let first = runtime.mobile_card_snapshots()[0]
        .task_timing
        .clone()
        .expect("真实起点");
    assert!(first.completed_elapsed_ms.is_none(), "开始不等于完成");
    assert!(
        runtime.observe_mobile_hook(&hook("UserPromptSubmit", "turn-a")),
        "保留现有重复Hook行为"
    );
    assert_eq!(
        runtime.mobile_card_snapshots()[0].task_timing.as_ref(),
        Some(&first),
        "查询/重复提交不推进计时快照或WSS"
    );
    assert!(
        !runtime.observe_mobile_hook(&hook("Stop", "old-turn")),
        "旧回合不能停止当前计时"
    );
    assert!(
        runtime.mobile_card_snapshots()[0]
            .task_timing
            .as_ref()
            .expect("计时")
            .completed_elapsed_ms
            .is_none(),
        "仍在运行"
    );
    assert!(
        runtime.observe_mobile_hook(&hook("Stop", "turn-a")),
        "准确结束"
    );
    let finished = runtime.mobile_card_snapshots()[0]
        .task_timing
        .clone()
        .expect("终态耗时");
    assert!(
        finished.completed_elapsed_ms.is_some(),
        "正确Stop冻结单调时钟耗时"
    );
    assert_eq!(
        finished.started_at_unix_ms, first.started_at_unix_ms,
        "完成不改变真实起点"
    );
    assert!(
        runtime.observe_mobile_hook(&hook("UserPromptSubmit", "turn-b")),
        "开始下一回合"
    );
    assert!(
        runtime.mobile_card_snapshots()[0]
            .task_timing
            .as_ref()
            .expect("新起点")
            .completed_elapsed_ms
            .is_none(),
        "不能继承旧完成耗时"
    );
    assert!(
        !runtime.observe_mobile_hook(&hook("Stop", "")),
        "缺身份不能冻结新回合"
    );
}
