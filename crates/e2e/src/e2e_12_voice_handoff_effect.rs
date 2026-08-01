//! 语音 Agent Handoff 动效：assign/reply 业务语义保持独立，成功回执统一播放流星。

use std::time::Duration;

use agent::{AdapterCommand, AdapterResult};
use gpui::TestAppContext;
use norma::protocol::{AgentProfile, CardCommand, CardCommandResult};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// VOICE-HANDOFF-E2E-1：语音通过 reply_agent 继续已有会话时，发送成功后仍应播放流星。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent cards       = 1（Alan/Codex）
///   voice replies     = 1（“继续检查语音派发链路”）
///   coordinated tasks = 0（reply_agent 只续接会话，不创建 task_id）
///   expected effects  = 1（唯一成功语音 Handoff 对应唯一 Flight）
///   intro settle      = 1000ms > 992ms natural timeline
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas 并创建唯一 Alan AgentCard，推进 1000ms 清空建卡动效。
///   2. 通过语音 Norma Adapter seam 执行 reply_agent，写入已有 AgentCard 会话。
///   3. 读取真实 AdapterResult、任务协调器计数与 dispatch effect snapshot。
///
/// 预期结果：
///   - 正断言：reply_agent 返回 sent 或 queued，并带同一 AgentCard 的 node_id/reply_id。
///   - 正断言：成功语音 Handoff 创建唯一 Flight，目标正是 Alan AgentCard。
///   - 负断言：继续会话不得创建 coordinated task，也不得产生额外 pending effect。
#[test]
fn e2e_12_voice_reply_agent_handoff_spawns_dispatch_receipt_effect() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("voice reply effect e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    let created = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            app.execute_agent_card_command_for_e2e(
                window,
                CardCommand::CreateAgent {
                    name: Some("Alan".to_string()),
                    profile: AgentProfile::Codex,
                    activate: true,
                },
            )
        })
    });
    let node_id = match created {
        Ok(CardCommandResult::AgentCreated {
            node_id: Some(node_id),
            ..
        }) => node_id,
        other => panic!("voice reply effect 前置必须创建 Alan；实际：{other:?}"),
    };
    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(1_000), cx);
    });
    cx.run_until_parked();

    let before = app.update(cx, |app, _cx| app.dispatch_receipt_effect_e2e_snapshot());
    assert_eq!(
        (before.active_count, before.pending_count),
        (0, 0),
        "语音 reply 前必须没有遗留 dispatch effect，避免把建卡动效误算为 Handoff"
    );

    let result = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.execute_voice_norma_adapter_command_for_e2e(
                window,
                cx,
                AdapterCommand::ReplyAgent {
                    agent: "Alan".to_string(),
                    message: "继续检查语音派发链路".to_string(),
                },
            )
        })
    });
    let AdapterResult {
        node_id: result_node_id,
        reply_id,
        reply_status,
        ..
    } = result.expect("语音 reply_agent 应被已有 Alan AgentCard 接收");
    assert_eq!(
        result_node_id,
        Some(node_id),
        "语音 reply 回执必须指向刚创建的唯一 Alan AgentCard"
    );
    assert!(
        reply_id.is_some(),
        "成功语音 reply 必须返回 reply_id，供视觉回执稳定去重"
    );
    assert!(
        matches!(reply_status.as_deref(), Some("sent" | "queued")),
        "语音 reply 必须是真实 sent/queued 成功状态；实际：{reply_status:?}"
    );

    let task_count = app.update(cx, |app, _cx| app.agent_task_count_for_e2e());
    let flight = app.update(cx, |app, _cx| app.dispatch_receipt_effect_e2e_snapshot());
    assert_eq!(
        task_count, 0,
        "reply_agent 继续会话不得为了播放流星而伪造 coordinated task"
    );
    assert_eq!(
        flight.active_count, 1,
        "唯一成功语音 reply Handoff 必须创建且只创建一个 active effect"
    );
    assert_eq!(
        flight.pending_count, 0,
        "单条语音 reply 不得额外创建 pending effect"
    );
    assert_eq!(
        flight.effects[0].target_node_id, node_id,
        "语音 reply 流星必须命中真实接收消息的 Alan AgentCard"
    );
    assert_eq!(
        flight.effects[0].phase, "flight",
        "语音 reply 的非 Reduce Motion 视觉回执必须从 Flight 开始"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}

/// VOICE-HANDOFF-E2E-2：相同 reply_agent 从非语音入口成功时，不得误触发语音流星。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent cards        = 1（Alan/Codex）
///   non-voice replies  = 1（“普通适配器续聊”）
///   successful replies = 1（有卡即可 sent/queued）
///   expected effects   = 0（来源不是 Voice）
///   intro settle       = 1000ms > 992ms natural timeline
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas 并创建唯一 Alan AgentCard，推进 1000ms 清空建卡动效。
///   2. 通过默认非语音 Norma Adapter seam 执行相同的 reply_agent 业务。
///   3. 对比成功业务回执与 dispatch effect snapshot。
///
/// 预期结果：
///   - 正断言：reply_agent 仍成功返回 sent 或 queued，证明来源门禁不改变业务。
///   - 负断言：MotionStore 不得出现 active/pending effect，也不得创建 coordinated task。
#[test]
fn e2e_12_non_voice_reply_agent_does_not_spawn_voice_handoff_effect() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("non-voice reply e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    let created = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            app.execute_agent_card_command_for_e2e(
                window,
                CardCommand::CreateAgent {
                    name: Some("Alan".to_string()),
                    profile: AgentProfile::Codex,
                    activate: true,
                },
            )
        })
    });
    assert!(
        matches!(
            created,
            Ok(CardCommandResult::AgentCreated {
                node_id: Some(_),
                ..
            })
        ),
        "non-voice reply effect 前置必须成功创建唯一 Alan AgentCard；实际：{created:?}"
    );
    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(1_000), cx);
    });
    cx.run_until_parked();

    let result = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.execute_norma_adapter_command_for_e2e(
                window,
                cx,
                AdapterCommand::ReplyAgent {
                    agent: "Alan".to_string(),
                    message: "普通适配器续聊".to_string(),
                },
            )
        })
    });
    let reply_status = result
        .expect("非语音来源门禁不得阻止 reply_agent 业务成功")
        .reply_status;
    assert!(
        matches!(reply_status.as_deref(), Some("sent" | "queued")),
        "非语音 reply 仍必须获得真实 sent/queued 回执；实际：{reply_status:?}"
    );

    let task_count = app.update(cx, |app, _cx| app.agent_task_count_for_e2e());
    let snapshot = app.update(cx, |app, _cx| app.dispatch_receipt_effect_e2e_snapshot());
    assert_eq!(task_count, 0, "非语音 reply 不得创建 coordinated task");
    assert_eq!(
        (snapshot.active_count, snapshot.pending_count),
        (0, 0),
        "非语音 reply 成功后不得误播放仅属于 Voice Handoff 的流星"
    );
    assert!(
        snapshot.effects.is_empty(),
        "非语音 reply 不得留下任何不可见的 Handoff presentation"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}
