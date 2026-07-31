//! Dispatch receipt 特效端到端验收：真实命令入口、GPUI overlay 与 Motion 生命周期闭环。

use std::time::Duration;

use gpui::TestAppContext;
use norma::protocol::{
    AgentProfile, AgentTaskAssignment, AgentTaskReceiptStatus, CardCommand, CardCommandResult,
};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// DR-E2E-1：Norma 接受派发后，overlay 必须从 Flight 进入 Impact 并最终自动卸载。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent cards = 1（Alan/Codex）
///   dispatch assignments = 1
///   Queued particles = 12
///   Agent intro settle = 1000ms > 992ms natural timeline
///   previous flight boundary = 250ms = 5 × 50ms
///   flight duration = 480ms = 250ms + 4 × 50ms + 30ms
///   previous impact boundary = 160ms = 4 × 40ms
///   impact duration = 260ms = 160ms + 2 × 50ms
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas，通过生产命令入口创建 Alan Agent Card。
///   2. 在未派发时渲染一帧，确认 overlay 不存在。
///   3. 通过同一入口派发一条 Norma 任务并显式通知 GPUI render。
///   4. 分别推进 480ms Flight 和 260ms Impact 时间线，读取稳定 e2e snapshot。
///
/// 预期结果：
///   - 正断言：Queued receipt 创建唯一 Flight，目标与业务 node_id 一致且使用 12 个粒子预算。
///   - 正断言：旧边界不提前切相，480ms 后进入 Impact，260ms 后 effect 被卸载。
///   - 负断言：普通建卡不触发，结束后 canvas_active 不得继续占用连续帧。
#[test]
fn e2e_12_norma_dispatch_receipt_renders_and_stops_after_impact() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("dispatch effect e2e 应创建临时 workspace");
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
    // Agent intro natural timeline = 992ms；推进 1000ms 后基线 Motion 应已空闲。
    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(1_000), cx);
    });
    cx.run_until_parked();
    assert!(
        matches!(created, Ok(CardCommandResult::AgentCreated { .. })),
        "dispatch effect e2e 前置必须成功创建 Alan；实际：{created:?}"
    );
    assert!(
        cx.debug_bounds("dispatch-receipt-effect-overlay").is_none(),
        "普通建卡不是 dispatch receipt，不能提前渲染特效 overlay"
    );

    let dispatched = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            let result = app.execute_agent_card_command_for_e2e(
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Alan".to_string(),
                        task: "实现 dispatch receipt 特效".to_string(),
                    }],
                    force_untracked: false,
                },
            );
            cx.notify();
            result
        })
    });
    let receipt = match &dispatched {
        Ok(CardCommandResult::AgentTasksDispatched { receipts }) if receipts.len() == 1 => {
            &receipts[0]
        }
        other => panic!("Norma 派发必须返回唯一 receipt，实际：{other:?}"),
    };
    cx.run_until_parked();
    let flight = app.update(cx, |app, _cx| app.dispatch_receipt_effect_e2e_snapshot());

    assert_eq!(
        receipt.status,
        AgentTaskReceiptStatus::Queued,
        "新建 Agent 尚未 ready，业务 receipt 应保持真实 Queued 状态"
    );
    assert_eq!(
        flight.active_count, 1,
        "唯一 Queued receipt 必须创建且只创建一个 active effect"
    );
    assert_eq!(
        flight.effects[0].target_node_id, receipt.node_id,
        "effect 必须命中业务 receipt 返回的同一 Agent node_id"
    );
    assert_eq!(
        flight.effects[0].phase, "flight",
        "非 Reduce Motion 的 dispatch effect 必须从 Flight 开始"
    );
    assert_eq!(
        flight.effects[0].particle_count, 12,
        "Queued 风格必须使用克制的 12 粒子固定预算"
    );
    assert!(
        cx.debug_bounds("dispatch-receipt-effect-overlay").is_some(),
        "Flight 活跃时 GPUI 树必须实际渲染 dispatch overlay"
    );

    // previous boundary = 250ms = 5 × 50ms；延长节奏在此仍须保持 Flight。
    for _ in 0..5 {
        app.update(cx, |app, cx| {
            app.advance_animations_for_e2e(Duration::from_millis(50), cx);
        });
    }
    let former_flight_boundary =
        app.update(cx, |app, _cx| app.dispatch_receipt_effect_e2e_snapshot());
    assert_eq!(
        former_flight_boundary.effects[0].phase, "flight",
        "Flight 在旧 250ms 边界不得提前进入 Impact"
    );

    // remaining flight = 480ms - 250ms = 230ms = 4 × 50ms + 30ms。
    for _ in 0..4 {
        app.update(cx, |app, cx| {
            app.advance_animations_for_e2e(Duration::from_millis(50), cx);
        });
    }
    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(30), cx);
    });
    let impact = app.update(cx, |app, _cx| app.dispatch_receipt_effect_e2e_snapshot());
    assert_eq!(
        impact.effects[0].phase, "impact",
        "Flight 累计 480ms 后必须进入目标卡片 Impact"
    );
    assert!(
        impact.canvas_active,
        "Impact 尚未完成时动画帧驱动必须保持 active"
    );

    // previous boundary = 160ms = 4 × 40ms；延长节奏在此仍须保持 Impact。
    for _ in 0..4 {
        app.update(cx, |app, cx| {
            app.advance_animations_for_e2e(Duration::from_millis(40), cx);
        });
    }
    let former_impact_boundary =
        app.update(cx, |app, _cx| app.dispatch_receipt_effect_e2e_snapshot());
    assert_eq!(
        former_impact_boundary.effects[0].phase, "impact",
        "Impact 在旧 160ms 边界不得提前卸载"
    );

    // remaining impact = 260ms - 160ms = 100ms = 2 × 50ms。
    for _ in 0..2 {
        app.update(cx, |app, cx| {
            app.advance_animations_for_e2e(Duration::from_millis(50), cx);
        });
    }
    cx.run_until_parked();
    let finished = app.update(cx, |app, _cx| app.dispatch_receipt_effect_e2e_snapshot());

    assert_eq!(
        finished.active_count, 0,
        "Impact 累计 260ms 后 active effect 必须归零"
    );
    assert_eq!(
        finished.pending_count, 0,
        "单条派发结束后 pending effect 也必须明确为零"
    );
    assert!(
        finished.effects.is_empty(),
        "特效结束后不得保留不可见 presentation"
    );
    // GPUI 0.2.2 test-support 的 rendered_frame.debug_bounds 不会在 Frame::clear 时清空，
    // selector 首次出现后不能再用 `is_none()` 证明卸载；空 presentation 与 idle activity
    // 是 render() 返回 None 的生产条件，因此以下状态断言覆盖真实卸载分支。
    assert!(
        !finished.canvas_active,
        "特效结束后 canvas_active 必须恢复 false，不能继续占用 16ms frame"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}

/// DR-E2E-2：飞行期间窗口失活或目标离屏必须取消特效，恢复后不得重现旧动画。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent cards = 1（Alan/Codex）
///   accepted dispatches = 2
///   cancellation causes = inactive window(1) + offscreen target(1) = 2
///   offscreen pan = (10000, 10000)px，远大于 1280 × 800 test viewport
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Alan 并派发第一条任务，确认 Flight 已进入 MotionStore。
///   2. 模拟窗口失活并重绘，确认第一条 Flight 被取消。
///   3. 恢复窗口后派发第二条任务，再把 Canvas 平移到完全离屏并重绘。
///   4. 恢复原 viewport，确认旧 Flight/Impact 不会重新出现。
///
/// 预期结果：
///   - 正断言：两次派发各自先创建唯一 active effect。
///   - 正断言：两种取消原因累计精确取消 2 个 effect。
///   - 负断言：窗口/viewport 恢复后不得出现旧 presentation 或 dispatch slot。
#[test]
fn e2e_12_dispatch_receipt_cancels_when_window_or_target_becomes_invisible() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("dispatch cancel e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    cx.update(|window, app_cx| {
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
    })
    .expect("dispatch cancel e2e 前置必须创建 Alan");
    // Agent intro natural timeline = 992ms；推进 1000ms 隔离 dispatch activity。
    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(1_000), cx);
    });

    dispatch_to_alan(&app, cx, "窗口失活取消测试");
    cx.run_until_parked();
    let before_deactivate = app.update(cx, |app, _cx| app.dispatch_receipt_effect_e2e_snapshot());
    assert_eq!(
        before_deactivate.active_count, 1,
        "窗口失活前第一条 dispatch Flight 必须已进入 active store"
    );

    cx.deactivate_window();
    app.update(cx, |_app, cx| cx.notify());
    cx.run_until_parked();
    let after_deactivate = app.update(cx, |app, _cx| app.dispatch_receipt_effect_e2e_snapshot());
    assert_eq!(
        after_deactivate.cancelled_effects, 1,
        "窗口失活重绘必须精确取消第一条 dispatch effect"
    );
    assert!(
        after_deactivate.effects.is_empty(),
        "失活窗口不得保留稍后恢复的旧 Flight presentation"
    );

    cx.update(|window, _app_cx| window.activate_window());
    cx.run_until_parked();
    dispatch_to_alan(&app, cx, "目标离屏取消测试");
    cx.run_until_parked();
    let before_pan = app.update(cx, |app, _cx| app.dispatch_receipt_effect_e2e_snapshot());
    assert_eq!(
        before_pan.active_count, 1,
        "恢复窗口后第二条 dispatch 必须创建新的 active Flight"
    );

    app.update(cx, |app, cx| {
        app.runtime.document.viewport.pan = canvas::Vec2::new(10_000.0, 10_000.0);
        cx.notify();
    });
    cx.run_until_parked();
    let after_pan = app.update(cx, |app, _cx| app.dispatch_receipt_effect_e2e_snapshot());
    assert_eq!(
        after_pan.cancelled_effects, 2,
        "目标完全离屏后累计取消数必须从 1 增加到 2"
    );
    assert!(
        after_pan.effects.is_empty(),
        "目标离屏不得保留或播放悬空 Impact"
    );

    app.update(cx, |app, cx| {
        app.runtime.document.viewport.pan = canvas::Vec2::ZERO;
        cx.notify();
    });
    cx.run_until_parked();
    let restored = app.update(cx, |app, _cx| app.dispatch_receipt_effect_e2e_snapshot());
    assert!(
        restored.effects.is_empty(),
        "恢复 viewport 后已取消的旧 dispatch effect 不得重新出现"
    );
    assert_eq!(
        (restored.active_count, restored.pending_count),
        (0, 0),
        "两条 effect 均取消后不得残留 active/pending dispatch slot"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}

fn dispatch_to_alan(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
    task: &str,
) {
    let result = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            let result = app.execute_agent_card_command_for_e2e(
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Alan".to_string(),
                        task: task.to_string(),
                    }],
                    force_untracked: false,
                },
            );
            cx.notify();
            result
        })
    });
    assert!(
        matches!(result, Ok(CardCommandResult::AgentTasksDispatched { ref receipts }) if receipts.len() == 1),
        "可见前台 Canvas 的测试派发必须返回唯一业务 receipt；实际：{result:?}"
    );
}
