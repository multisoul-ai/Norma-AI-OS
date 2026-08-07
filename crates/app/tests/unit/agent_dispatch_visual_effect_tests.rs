use super::{execute_agent_card_command, execute_high_level_agent_command};
use crate::app::AppScreen;
use crate::motion::DispatchVisualOrigin;
use canvas::CanvasRect;
use gpui::TestAppContext;
use norma::protocol::{
    AgentProfile, AgentTaskAssignment, AgentTaskReceiptStatus, CardCommand, CardCommandResult,
};
use terminal::{PtyBackend, PtyError};

struct RejectTaskWritePtyBackend;

impl PtyBackend for RejectTaskWritePtyBackend {
    fn write(&mut self, _bytes: &[u8]) -> Result<(), PtyError> {
        Err(PtyError::Io(std::io::Error::other(
            "injected dispatch task write failure",
        )))
    }

    fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), PtyError> {
        Ok(())
    }

    fn kill(&mut self) {}
}

/// DR-T-1：Norma 来源的可接受 dispatch receipt 必须创建命中同一 Agent Card 的视觉事件。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent definitions = 1（Alan/Codex）
///   assignments = 1（给 Alan 的任务）
///   target Agent cards = 1
///   expected receipts = 1；expected visual effects = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless App 并进入临时 Canvas workspace。
///   2. 通过生产命令入口创建并激活 Alan Agent Card。
///   3. 通过同一入口执行 `DispatchAgentTasks`，得到最终 Queued receipt。
///   4. 从 MotionStore 的公开 presentation seam 读取可见 dispatch effect。
///
/// 预期结果：
///   - 正断言：业务返回唯一 Queued receipt，视觉层也只有一个 effect。
///   - 正断言：视觉 effect 的目标 node_id 与真实 receipt 完全一致。
///   - 负断言：视觉 effect 不能命中其它卡片，也不能重复创建。
#[test]
fn test_norma_accepted_dispatch_receipt_spawns_one_visual_effect_for_same_target() {
    let factory: runtime::PtyFactory = Box::new(|_cmd, _cols, _rows, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)) as Box<dyn terminal::PtyBackend>)
    });
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new()
        .expect("dispatch visual effect test should create a temp workspace");
    let (app, cx) = gpui
        .add_window_view(|_window, cx| crate::app::MultisoulApp::new_with_pty_factory(cx, factory));
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });

    let create_result = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            execute_agent_card_command(
                app,
                window,
                CardCommand::CreateAgent {
                    name: Some("Alan".to_string()),
                    profile: AgentProfile::Codex,
                    activate: true,
                },
            )
        })
    });
    let dispatch_result = app.update(cx, |app, _cx| {
        app.window_active = true;
        execute_high_level_agent_command(
            app,
            CardCommand::DispatchAgentTasks {
                assignments: vec![AgentTaskAssignment {
                    name: "Alan".to_string(),
                    task: "实现设置页".to_string(),
                }],
                force_untracked: false,
            },
            CanvasRect::new(0.0, 0.0, 2_000.0, 2_000.0),
            1.0,
            agent::AgentDispatchSource::norma(),
            None,
        )
    });
    let presentations = app.update(cx, |app, _cx| app.motion.dispatch_receipt_presentations());
    let receipt = match &dispatch_result {
        Ok(CardCommandResult::AgentTasksDispatched { receipts }) if receipts.len() == 1 => {
            &receipts[0]
        }
        other => panic!("派发必须返回唯一业务 receipt，实际：{other:?}"),
    };

    assert!(
        matches!(create_result, Ok(CardCommandResult::AgentCreated { .. })),
        "测试前置必须成功创建 Alan Agent Card；实际：{create_result:?}"
    );
    assert_eq!(
        receipt.status,
        AgentTaskReceiptStatus::Queued,
        "新 Agent 等待 CLI ready 时必须返回真实 Queued receipt"
    );
    assert_eq!(
        presentations.len(),
        1,
        "唯一可接受 receipt 必须创建且只创建一个视觉 effect"
    );
    assert_eq!(
        presentations[0].origin,
        DispatchVisualOrigin::CommandDock,
        "原有 Norma 直接指派必须继续从 Command Dock 发射"
    );
    assert_eq!(
        presentations[0].target_node_id, receipt.node_id,
        "视觉 effect 必须命中 receipt 指定的真实 Agent node_id"
    );
    assert_ne!(
        presentations[0].target_node_id, 0,
        "视觉 effect 不得退化为缺失或占位目标 node_id=0"
    );
    assert_ne!(
        presentations[0].origin,
        DispatchVisualOrigin::AgentNode(receipt.node_id),
        "原有 Norma 直接指派不得被误判成目标 Agent 自己发射"
    );
}

/// DR-T-2：非 Norma 来源即使得到 Queued receipt，也不能伪装成 Norma 派发特效。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent definitions = 1（Alan/Codex）
///   manual assignments = 1
///   accepted business receipts = 1
///   expected dispatch visual effects = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless App，创建并激活 Alan Agent Card。
///   2. 通过高层协调入口传入 `AgentDispatchSource::manual()`。
///   3. 确认业务层仍接受任务并返回 Queued receipt。
///   4. 读取 MotionStore 的 dispatch presentations。
///
/// 预期结果：
///   - 正断言：manual 派发继续产生真实 Queued receipt，不改变业务行为。
///   - 负断言：MotionStore 中不得创建任何 Norma dispatch receipt effect。
///   - 负断言：业务成功不能绕过 source 过滤器触发视觉反馈。
#[test]
fn test_manual_dispatch_receipt_does_not_spawn_norma_visual_effect() {
    let factory: runtime::PtyFactory = Box::new(|_cmd, _cols, _rows, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)) as Box<dyn terminal::PtyBackend>)
    });
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new()
        .expect("manual dispatch visual filter test should create a temp workspace");
    let (app, cx) = gpui
        .add_window_view(|_window, cx| crate::app::MultisoulApp::new_with_pty_factory(cx, factory));
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });

    let create_result = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            execute_agent_card_command(
                app,
                window,
                CardCommand::CreateAgent {
                    name: Some("Alan".to_string()),
                    profile: AgentProfile::Codex,
                    activate: true,
                },
            )
        })
    });
    let manual_result = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            let area = app.visible_canvas_rect(window);
            let zoom = app.runtime.document.viewport.zoom;
            execute_high_level_agent_command(
                app,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Alan".to_string(),
                        task: "手动执行设置页".to_string(),
                    }],
                    force_untracked: false,
                },
                area,
                zoom,
                agent::AgentDispatchSource::manual(),
                None,
            )
        })
    });
    let presentations = app.update(cx, |app, _cx| app.motion.dispatch_receipt_presentations());

    assert!(
        matches!(create_result, Ok(CardCommandResult::AgentCreated { .. })),
        "manual source 过滤测试的 Alan Agent Card 前置必须创建成功；实际：{create_result:?}"
    );
    assert!(
        matches!(
            manual_result,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Queued
        ),
        "manual 派发的业务结果必须保持唯一 Queued receipt；实际：{manual_result:?}"
    );
    assert!(
        presentations.is_empty(),
        "manual 来源不得创建 Norma dispatch receipt effect；实际：{presentations:?}"
    );
    assert_eq!(
        presentations.len(),
        0,
        "source 过滤后视觉 effect 数量必须明确为零"
    );
}

/// DR-T-3：Norma 派发写入失败时必须保留 Failed 业务回执，且不播放成功命中特效。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent cards = 1（Alan/Codex）
///   ready nodes = 1
///   task writes = 1，注入失败数 = 1
///   expected Failed receipts = 1；expected visual effects = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用拒绝全部 task write 的 PTY backend 创建 Alan Agent Card。
///   2. 显式把 Agent 标记为 ready，使任务立即进入真实输入提交路径。
///   3. 从 Norma 入口派发任务，得到最终 Failed receipt。
///   4. 读取 MotionStore presentations，确认失败没有被包装为成功动画。
///
/// 预期结果：
///   - 正断言：业务层返回唯一 Failed receipt，错误语义不被视觉层篡改。
///   - 负断言：Failed receipt 不得创建 Flight、Impact 或粒子 presentation。
///   - 负断言：MotionStore 中 effect 数量必须为零。
#[test]
fn test_failed_norma_dispatch_receipt_does_not_spawn_success_visual_effect() {
    let factory: runtime::PtyFactory = Box::new(|_cmd, _cols, _rows, _tx| {
        Ok(Box::new(RejectTaskWritePtyBackend) as Box<dyn terminal::PtyBackend>)
    });
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new()
        .expect("failed dispatch visual filter test should create a temp workspace");
    let (app, cx) = gpui
        .add_window_view(|_window, cx| crate::app::MultisoulApp::new_with_pty_factory(cx, factory));
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });

    let create_result = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            execute_agent_card_command(
                app,
                window,
                CardCommand::CreateAgent {
                    name: Some("Alan".to_string()),
                    profile: AgentProfile::Codex,
                    activate: true,
                },
            )
        })
    });
    let node_id = match &create_result {
        Ok(CardCommandResult::AgentCreated {
            node_id: Some(node_id),
            ..
        }) => *node_id,
        other => panic!("失败路径前置必须创建活跃 Alan，实际：{other:?}"),
    };
    let marked_ready = app.update(cx, |app, _cx| {
        app.pending_agent_submissions.mark_ready(node_id)
    });
    let dispatch_result = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            execute_agent_card_command(
                app,
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Alan".to_string(),
                        task: "这条任务应注入写入失败".to_string(),
                    }],
                    force_untracked: false,
                },
            )
        })
    });
    let presentations = app.update(cx, |app, _cx| app.motion.dispatch_receipt_presentations());

    assert!(
        marked_ready,
        "失败路径必须先把唯一 Alan node 标为 ready，才能进入 task write"
    );
    assert!(
        matches!(
            dispatch_result,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Failed
        ),
        "注入 task write 失败后必须返回唯一 Failed receipt；实际：{dispatch_result:?}"
    );
    assert!(
        presentations.is_empty(),
        "Failed receipt 不得创建任何成功命中 presentation；实际：{presentations:?}"
    );
    assert_eq!(
        presentations.len(),
        0,
        "Failed receipt 过滤后的视觉 effect 数量必须明确为零"
    );
}

/// DR-T-4：非 Canvas 和离屏目标必须保留业务 receipt，但拒绝创建旧特效。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent cards = 1（Alan/Codex）
///   accepted dispatches = Launchpad(1) + offscreen area(1) = 2
///   fake visible area = (5000, 5000, 800, 600)，与卡片当前 rect 无交集
///   expected unavailable skips = 2；expected visual effects = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 Canvas 创建 Alan 后切到 Launchpad 派发第一条任务。
///   2. 返回 Canvas，并向完全离屏的可见区域派发第二条任务。
///   3. 读取两次业务结果、Motion presentation 与结构化诊断。
///
/// 预期结果：
///   - 正断言：两次任务均继续返回真实业务 receipt，不被视觉门禁阻塞。
///   - 正断言：两种不可播放上下文各记录一次 unavailable skip。
///   - 负断言：任一步都不得创建稍后恢复播放的 Flight 或 Impact。
#[test]
fn test_norma_dispatch_visual_effect_requires_visible_canvas_target() {
    let factory: runtime::PtyFactory = Box::new(|_cmd, _cols, _rows, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)) as Box<dyn terminal::PtyBackend>)
    });
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new()
        .expect("dispatch visibility gate test should create a temp workspace");
    let (app, cx) = gpui
        .add_window_view(|_window, cx| crate::app::MultisoulApp::new_with_pty_factory(cx, factory));
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            execute_agent_card_command(
                app,
                window,
                CardCommand::CreateAgent {
                    name: Some("Alan".to_string()),
                    profile: AgentProfile::Codex,
                    activate: true,
                },
            )
        })
    })
    .expect("visibility gate 前置必须创建 Alan Agent Card");

    let launchpad_result = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            app.screen = AppScreen::Launchpad;
            execute_agent_card_command(
                app,
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Alan".to_string(),
                        task: "Launchpad 中仍需返回业务 receipt".to_string(),
                    }],
                    force_untracked: false,
                },
            )
        })
    });
    let offscreen_result = app.update(cx, |app, _cx| {
        app.screen = AppScreen::Canvas;
        execute_high_level_agent_command(
            app,
            CardCommand::DispatchAgentTasks {
                assignments: vec![AgentTaskAssignment {
                    name: "Alan".to_string(),
                    task: "离屏目标仍需返回业务 receipt".to_string(),
                }],
                force_untracked: false,
            },
            CanvasRect::new(5_000.0, 5_000.0, 800.0, 600.0),
            1.0,
            agent::AgentDispatchSource::norma(),
            None,
        )
    });
    let (presentations, diagnostics) = app.update(cx, |app, _cx| {
        (
            app.motion.dispatch_receipt_presentations(),
            app.motion.diagnostics(),
        )
    });

    assert!(
        matches!(launchpad_result, Ok(CardCommandResult::AgentTasksDispatched { ref receipts }) if receipts.len() == 1),
        "Launchpad 视觉门禁不得阻断业务派发；实际：{launchpad_result:?}"
    );
    assert!(
        matches!(offscreen_result, Ok(CardCommandResult::AgentTasksDispatched { ref receipts }) if receipts.len() == 1),
        "离屏目标视觉门禁不得阻断业务派发；实际：{offscreen_result:?}"
    );
    assert_eq!(
        diagnostics.dispatch_unavailable_skips, 2,
        "Launchpad 与离屏目标应各记录一次 unavailable skip"
    );
    assert!(
        presentations.is_empty(),
        "不可播放上下文不得积压恢复后重现的 dispatch effect；实际：{presentations:?}"
    );
    assert_eq!(
        presentations.len(),
        0,
        "两次不可见派发后的 active presentation 数量必须明确为零"
    );
}
