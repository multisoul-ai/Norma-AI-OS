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

/// VOICE-HANDOFF-E2E-3：窗口失活后渲染一帧，成功语音 Handoff 仍必须保留流星。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent cards = 1（Alan/Codex，位于当前可见 Canvas）
///   window_active = false（模拟用户切到其他 macOS 窗口）
///   voice replies = 1（“后台继续检查流星链路”）
///   expected effects before render = 1；expected effects after render = 1
///   background animation tick = 16ms（默认关闭 Energy Saver 的生产 timer cadence）
///   intro settle = 1000ms > 992ms natural timeline
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas，创建 Alan AgentCard，并推进 1000ms 清空建卡动效。
///   2. 将窗口标记为失活，通过 Voice Adapter seam 执行 reply_agent。
///   3. 在同一更新内确认业务成功且 effect 已创建。
///   4. 让 GPUI 完成下一帧渲染，再推进 16ms 生产 timer 并读取 effect 进度。
///
/// 预期结果：
///   - 正断言：语音 reply 返回 sent/queued，并创建唯一 active effect。
///   - 正断言：失活窗口完成渲染后，唯一 effect 仍命中 Alan AgentCard。
///   - 正断言：失活窗口的生产 timer 继续推进 Flight progress。
///   - 负断言：render 清理不得把失活窗口误判为目标不可见并删除 effect。
#[test]
fn e2e_12_inactive_window_keeps_voice_handoff_effect_after_render() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("inactive voice handoff e2e 应创建临时 workspace");
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
        other => panic!("inactive voice handoff 前置必须创建 Alan；实际：{other:?}"),
    };
    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(1_000), cx);
    });
    cx.run_until_parked();

    cx.deactivate_window();
    let (result, before_render) = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            let result = app.execute_voice_norma_adapter_command_for_e2e(
                window,
                cx,
                AdapterCommand::ReplyAgent {
                    agent: "Alan".to_string(),
                    message: "后台继续检查流星链路".to_string(),
                },
            );
            let snapshot = app.dispatch_receipt_effect_e2e_snapshot();
            (result, snapshot)
        })
    });
    let reply_status = result
        .expect("失活窗口不得阻止语音 reply_agent 业务成功")
        .reply_status;
    assert!(
        matches!(reply_status.as_deref(), Some("sent" | "queued")),
        "失活窗口的语音 reply 必须返回真实 sent/queued；实际：{reply_status:?}"
    );
    assert_eq!(
        before_render.active_count, 1,
        "失活窗口的成功语音 Handoff 必须先创建唯一 active effect"
    );
    assert_eq!(
        before_render.effects[0].target_node_id, node_id,
        "渲染前的失活窗口流星必须指向 Alan AgentCard"
    );

    cx.run_until_parked();
    let after_render = app.update(cx, |app, _cx| app.dispatch_receipt_effect_e2e_snapshot());
    assert_eq!(
        after_render.active_count, 1,
        "窗口失活后的 GPUI render 不得删除仍应播放的唯一流星 effect"
    );
    assert_eq!(
        after_render.pending_count, 0,
        "单条失活窗口语音 Handoff 不得产生额外 pending effect"
    );
    assert_eq!(
        after_render.effects[0].target_node_id, node_id,
        "render 后保留的流星必须继续命中 Alan AgentCard"
    );
    assert_ne!(
        after_render.effects[0].target_node_id, 0,
        "render 后保留的失活窗口流星不得退化为占位 node_id=0"
    );

    let initial_progress = after_render.effects[0].progress_milli;
    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(16));
    cx.run_until_parked();
    let advanced = app.update(cx, |app, _cx| app.dispatch_receipt_effect_e2e_snapshot());
    assert!(
        advanced.effects[0].progress_milli > initial_progress,
        "失活窗口的 16ms 生产 timer 必须继续推进 Flight；before={initial_progress}, after={}",
        advanced.effects[0].progress_milli
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}
