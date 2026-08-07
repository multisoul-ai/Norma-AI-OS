//! Headless GPUI E2E：验证新 Agent 等待 CLI 就绪后自动输入并提交任务。

use gpui::TestAppContext;
use norma::protocol::{
    AgentProfile as WireAgentProfile, AgentTaskAssignment, AgentTaskReceiptStatus, CardCommand,
    CardCommandResult,
};
use std::time::Duration;

use crate::harness::{
    drain_until, enter_canvas, make_app, read_grid, read_runtime, scripted_factory,
};

/// E2E-AGENT-SUBMIT-01：新 Agent 的任务必须等待 CLI Ready，随后自动写入文本并发送真实回车。
///
/// 数据构造（含关键数值的推导过程）：
///   initial definitions = 0，initial Agent cards = 0；
///   CreateAgent(activate=false) → definitions = 1，Agent cards 仍为 0；
///   DispatchAgentTasks(assignments=1) → Agent cards = 1，pending submissions = 1；
///   scripted PTY rules = 2：任务文本 → `TASK_TYPED`，回车 `\r` → `SUBMITTED`。
///   observation timeout = 1,000ms ÷ 12ms/frame ≈ 83 帧，远大于所需的 1 次 drain。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 通过公开 e2e command seam 创建未激活的 Alan/Codex 定义；
///   2. 派发 1 个任务，触发创建 Agent Card，但不立即写 PTY；
///   3. 将 Codex SessionStart 放入生产 Hook 队列，并推进 12ms runtime frame loop；
///   4. 从 Agent terminal grid 读取 scripted PTY 对文本写入与回车写入的独立回执。
///
/// 预期结果：
///   - 正断言：Ready 前 receipt=Queued 且 pending=true；
///   - 正断言：Ready 后 grid 同时包含 `TASK_TYPED` 与 `SUBMITTED`；
///   - 负断言：Ready 前 grid 不得提前出现任一写入回执，Ready 后 pending 必须清空。
#[test]
fn new_agent_task_waits_for_ready_then_types_and_submits() {
    let mut test_cx = TestAppContext::single();
    let rules = vec![
        (
            "分析 README".as_bytes().to_vec(),
            b"TASK_TYPED\r\n".to_vec(),
        ),
        (b"\r".to_vec(), b"SUBMITTED\r\n".to_vec()),
    ];
    let (app, cx) = make_app(&mut test_cx, scripted_factory(rules));
    let workspace = tempfile::tempdir().expect("Agent submit e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    let created = cx.update(|window, cx| {
        app.update(cx, |app, _cx| {
            app.execute_agent_card_command_for_e2e(
                window,
                CardCommand::CreateAgent {
                    name: Some("Alan".to_string()),
                    profile: WireAgentProfile::Codex,
                    activate: false,
                },
            )
        })
    });
    assert!(
        matches!(
            created,
            Ok(CardCommandResult::AgentCreated { node_id: None, .. })
        ),
        "未激活的 Alan/Codex 定义应创建成功且不得提前打开 Card，实际：{created:?}"
    );

    let dispatched = cx.update(|window, cx| {
        app.update(cx, |app, _cx| {
            app.execute_agent_card_command_for_e2e(
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Alan".to_string(),
                        task: "分析 README".to_string(),
                    }],
                    force_untracked: false,
                },
            )
        })
    });
    let CardCommandResult::AgentTasksDispatched { receipts } =
        dispatched.expect("向 Alan 派发任务应成功")
    else {
        panic!("任务派发应返回 AgentTasksDispatched");
    };
    assert_eq!(receipts.len(), 1, "单个 assignment 应产生恰好 1 个 receipt");
    let receipt = &receipts[0];
    let node_id = receipt.node_id;
    assert_eq!(
        receipt.status,
        AgentTaskReceiptStatus::Queued,
        "新 Agent CLI Ready 前 receipt 必须是 Queued，不能谎报 Submitted"
    );
    assert!(
        app.update(cx, |app, _cx| app.agent_submission_waiting_for_e2e(node_id)),
        "新 Agent 的任务应保留在 startup pending 队列"
    );
    let before_text = read_grid(&app, cx, node_id)
        .expect("新 Agent 应已有可读取的 terminal grid")
        .rows
        .iter()
        .flatten()
        .map(|cell| cell.c)
        .collect::<String>();
    assert!(
        !before_text.contains("TASK_TYPED"),
        "Ready 前不得把任务文本写入 PTY，grid：{before_text:?}"
    );
    assert!(
        !before_text.contains("SUBMITTED"),
        "Ready 前不得发送回车，grid：{before_text:?}"
    );

    let ready_event = norma::agent_hook::parse_hook_event(
        norma::agent_hook::HookSource::Codex,
        node_id,
        serde_json::json!({
            "hook_event_name": "SessionStart",
            "session_id": "codex-e2e-ready"
        }),
    )
    .expect("Codex SessionStart e2e 事件应可解析");
    app.update(cx, |app, _cx| {
        app.enqueue_agent_hook_event_for_e2e(ready_event)
    })
    .expect("SessionStart 应进入生产 Agent Hook 队列");
    let after_text = drain_until(
        &app,
        cx,
        |runtime| match runtime.node_view(node_id) {
            Some(runtime::NodeView::Term { snapshot, .. }) => {
                let text = snapshot
                    .rows
                    .iter()
                    .flatten()
                    .map(|cell| cell.c)
                    .collect::<String>();
                (text.contains("TASK_TYPED") && text.contains("SUBMITTED")).then_some(text)
            }
            _ => None,
        },
        // 1,000ms / 12ms ≈ 83 次 frame，足以覆盖入队、drain、两次 PTY write 与 grid drain。
        Duration::from_millis(1_000),
    )
    .expect("Ready 后 1 秒内应观察到文本写入与真实回车的两个 PTY 回执");

    assert!(
        after_text.contains("TASK_TYPED"),
        "Ready 后必须写入完整任务文本，grid：{after_text:?}"
    );
    assert!(
        after_text.contains("SUBMITTED"),
        "Ready 后必须额外发送真实回车，grid：{after_text:?}"
    );
    assert!(
        !app.update(cx, |app, _cx| app.agent_submission_waiting_for_e2e(node_id)),
        "提交成功后不得残留 startup pending 状态"
    );
}

/// E2E-AGENT-SUBMIT-02：用户在 Agent 卡片按 Ctrl+C 终止会话后，旧等待队列必须清空，
/// 下一次 Norma 派发必须立即执行。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent definitions = 1（Alan/Codex），active cards = 1；
///   终止前任务 = task-1（当前）+ task-2（等待）= 2 项；
///   终止后任务 = task-3 = 1 项；
///   scripted PTY 回执 = task-1 → TASK_1，task-2 → TASK_2，task-3 → TASK_3；
///   每次提交 Enter = SUBMITTED，预期终止前后各 1 次，因此总计 2 次；
///   每段 observation timeout = 1,000ms ÷ 12ms/frame ≈ 83 帧。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建未激活 Alan，派发 task-1，注入 SessionStart 使其完成正文与 Enter 提交；
///   2. 注入 UserPromptSubmit 进入 Thinking，再派发 task-2 进入 managed FIFO；
///   3. 经真实 GPUI keyboard seam 发送 Ctrl+C，触发 Agent 卡片直接终止；
///   4. 再经 Norma command seam 派发 task-3，推进生产 frame loop；
///   5. 从 runtime 状态与 terminal grid 验证终止收口和新任务提交。
///
/// 预期结果：
///   - 正断言：Ctrl+C 后 runtime 立即进入 Ended，task-3 最终写入并提交；
///   - 正断言：grid 中 task-1 与 task-3 各出现 1 次，SUBMITTED 共出现 2 次；
///   - 负断言：已清除的 task-2 不得写入 grid；
///   - 负断言：task-3 不得继续卡在终止前的旧队列后面。
#[test]
fn ctrl_c_termination_clears_queue_before_next_norma_dispatch() {
    let mut test_cx = TestAppContext::single();
    let rules = vec![
        (b"task-1".to_vec(), b"TASK_1\r\n".to_vec()),
        (b"task-2".to_vec(), b"TASK_2\r\n".to_vec()),
        (b"task-3".to_vec(), b"TASK_3\r\n".to_vec()),
        (b"\r".to_vec(), b"SUBMITTED\r\n".to_vec()),
    ];
    let (app, cx) = make_app(&mut test_cx, scripted_factory(rules));
    let workspace = tempfile::tempdir().expect("Agent Ctrl+C e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    let created = cx.update(|window, cx| {
        app.update(cx, |app, _cx| {
            app.execute_agent_card_command_for_e2e(
                window,
                CardCommand::CreateAgent {
                    name: Some("Alan".to_string()),
                    profile: WireAgentProfile::Codex,
                    activate: false,
                },
            )
        })
    });
    assert!(
        matches!(
            created,
            Ok(CardCommandResult::AgentCreated { node_id: None, .. })
        ),
        "Ctrl+C 队列 e2e 前置必须创建未激活 Alan，实际：{created:?}"
    );

    let first = cx.update(|window, cx| {
        app.update(cx, |app, _cx| {
            app.execute_agent_card_command_for_e2e(
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Alan".to_string(),
                        task: "task-1".to_string(),
                    }],
                    force_untracked: false,
                },
            )
        })
    });
    let node_id = match &first {
        Ok(CardCommandResult::AgentTasksDispatched { receipts }) if receipts.len() == 1 => {
            receipts[0].node_id
        }
        other => panic!("task-1 应返回唯一派发回执，实际：{other:?}"),
    };
    let ready_event = norma::agent_hook::parse_hook_event(
        norma::agent_hook::HookSource::Codex,
        node_id,
        serde_json::json!({
            "hook_event_name": "SessionStart",
            "session_id": "ctrl-c-queue-session"
        }),
    )
    .expect("Ctrl+C 队列 e2e 的 SessionStart 应解析成功");
    app.update(cx, |app, _cx| {
        app.enqueue_agent_hook_event_for_e2e(ready_event)
    })
    .expect("SessionStart 应进入生产 Hook 队列");
    let before_interrupt = drain_until(
        &app,
        cx,
        |runtime| match runtime.node_view(node_id) {
            Some(runtime::NodeView::Term { snapshot, .. }) => {
                let text = snapshot
                    .rows
                    .iter()
                    .flatten()
                    .map(|cell| cell.c)
                    .collect::<String>();
                (text.contains("TASK_1") && text.contains("SUBMITTED")).then_some(text)
            }
            _ => None,
        },
        // 1,000ms / 12ms ≈ 83 帧，覆盖 SessionStart、正文和 Enter 三段推进。
        Duration::from_millis(1_000),
    )
    .expect("task-1 应在 1 秒内完成正文与 Enter 提交");
    assert!(
        before_interrupt.contains("TASK_1"),
        "终止前 task-1 必须已经写入 PTY，grid：{before_interrupt:?}"
    );

    let thinking_event = norma::agent_hook::parse_hook_event(
        norma::agent_hook::HookSource::Codex,
        node_id,
        serde_json::json!({
            "hook_event_name": "UserPromptSubmit",
            "session_id": "ctrl-c-queue-session",
            "prompt": "task-1"
        }),
    )
    .expect("Ctrl+C 队列 e2e 的 UserPromptSubmit 应解析成功");
    app.update(cx, |app, _cx| {
        app.enqueue_agent_hook_event_for_e2e(thinking_event)
    })
    .expect("UserPromptSubmit 应进入生产 Hook 队列");
    let thinking = drain_until(
        &app,
        cx,
        |runtime| {
            (runtime.agent_task_state(node_id) == Some(runtime::AgentTaskState::Thinking))
                .then_some(true)
        },
        // 1,000ms / 12ms ≈ 83 帧，覆盖 1 个 Hook event 的 drain。
        Duration::from_millis(1_000),
    );
    assert_eq!(
        thinking,
        Some(true),
        "task-1 必须进入 Thinking，确保 Ctrl+C 命中活动 Agent 任务"
    );

    let second = cx.update(|window, cx| {
        app.update(cx, |app, _cx| {
            app.execute_agent_card_command_for_e2e(
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Alan".to_string(),
                        task: "task-2".to_string(),
                    }],
                    force_untracked: false,
                },
            )
        })
    });
    assert!(
        matches!(
            second,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Queued
        ),
        "task-1 Thinking 时 task-2 必须进入等待队列，实际：{second:?}"
    );

    cx.simulate_keystrokes("ctrl-c");
    cx.run_until_parked();
    let ended = read_runtime(&app, cx, |runtime| runtime.agent_task_state(node_id));
    assert_eq!(
        ended,
        Some(runtime::AgentTaskState::Ended),
        "Ctrl+C 必须立即把活动 Agent 会话标记为 Ended"
    );

    let third = cx.update(|window, cx| {
        app.update(cx, |app, _cx| {
            app.execute_agent_card_command_for_e2e(
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Alan".to_string(),
                        task: "task-3".to_string(),
                    }],
                    force_untracked: false,
                },
            )
        })
    });
    assert!(
        matches!(
            third,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Queued
        ),
        "终止后的 task-3 应进入唯一待提交位，实际：{third:?}"
    );
    let after_interrupt = drain_until(
        &app,
        cx,
        |runtime| match runtime.node_view(node_id) {
            Some(runtime::NodeView::Term { snapshot, .. }) => {
                let text = snapshot
                    .rows
                    .iter()
                    .flatten()
                    .map(|cell| cell.c)
                    .collect::<String>();
                (text.contains("TASK_3") && text.matches("SUBMITTED").count() == 2).then_some(text)
            }
            _ => None,
        },
        // 1,000ms / 12ms ≈ 83 帧，覆盖 task-3 正文与 Enter 两阶段。
        Duration::from_millis(1_000),
    )
    .expect("终止清队后 task-3 应在 1 秒内完成提交");

    assert!(
        after_interrupt.contains("TASK_3"),
        "终止后新派发的 task-3 必须写入 PTY，grid：{after_interrupt:?}"
    );
    assert_eq!(
        after_interrupt.matches("SUBMITTED").count(),
        2,
        "task-1 与 task-3 应各提交一次，旧 task-2 不得产生第三次提交"
    );
    assert!(
        !after_interrupt.contains("TASK_2"),
        "Ctrl+C 终止时必须清除等待任务 task-2，grid：{after_interrupt:?}"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}
