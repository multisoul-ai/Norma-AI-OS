//! Headless GPUI E2E：验证新 Agent 等待 CLI 就绪后自动输入并提交任务。

use gpui::TestAppContext;
use norma::protocol::{
    AgentProfile as WireAgentProfile, AgentTaskAssignment, AgentTaskReceiptStatus, CardCommand,
    CardCommandResult,
};
use std::time::Duration;

use crate::harness::{
    drain_until, enter_canvas, make_app, make_app_real, read_grid, scripted_factory,
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

/// E2E-AGENT-SUBMIT-L3：真实 Agent CLI 必须经生产 PTY 收到任务正文和回车。
///
/// 数据构造（含关键数值的推导过程）：
///   `NORMA_E2E_AGENT_CLI` = codex/claude/cursor/opencode 四选一；
///   real PTY factory = None，任务正文包含唯一标记 `NORMA_SUBMITTED`；
///   startup fallback = 30,000ms；observation timeout = 90,000ms = 3 × fallback，
///   可覆盖 CLI 启动、无 Hook 兜底提交、PTY 回写三段异步链路。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用生产 `MultisoulApp::new` 与真实 `portable_pty` 进入临时 workspace；
///   2. 关闭测试进程的 Hook 回传，按环境变量创建未激活 Agent，并派发固定回复任务；
///   3. 等待 30 秒保守兜底，避免 SessionStart 早于全屏 TUI 可输入状态；
///   4. 等待 grid 中 `NORMA_SUBMITTED` 出现两次（输入一次、回复一次），并检查 pending 已清空；
///   5. 显式 kill 所有 live PTY，避免遗留 Agent 子进程。
///
/// 预期结果：
///   - 正断言：初始回执为 Queued，真实 grid 最终同时包含输入与 Agent 固定回复；
///   - 正断言：正文与回车写入成功后 pending 状态清空；
///   - 负断言：Ready 前不得误报 Submitted，测试结束不得保留 live 子进程。
#[test]
#[ignore = "L3 会启动真实 Agent CLI；需显式设置 NORMA_E2E_AGENT_CLI"]
fn real_agent_cli_receives_and_submits_prompt() {
    let (profile, agent_name) = match std::env::var("NORMA_E2E_AGENT_CLI")
        .expect("运行 L3 前必须设置 NORMA_E2E_AGENT_CLI=codex|claude|cursor|opencode|grok")
        .as_str()
    {
        "codex" => (WireAgentProfile::Codex, "CodexProbe"),
        "claude" => (WireAgentProfile::Claude, "ClaudeProbe"),
        "cursor" => (WireAgentProfile::Cursor, "CursorProbe"),
        "opencode" => (WireAgentProfile::OpenCode, "OpenCodeProbe"),
        "grok" => (WireAgentProfile::Grok, "GrokProbe"),
        other => {
            panic!("NORMA_E2E_AGENT_CLI 必须是 codex、claude、cursor、opencode 或 grok，实际：{other:?}")
        }
    };
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app_real(&mut test_cx);
    let workspace = tempfile::tempdir().expect("真实 Agent L3 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    app.update(cx, |app, _cx| {
        app.disable_agent_hook_for_test();
        app.runtime
            .set_agent_hook_config(Some(runtime::AgentHookRuntimeConfig {
                endpoint: "http://127.0.0.1:9/agent-hooks".to_string(),
                token: "e2e-unreachable-hook".to_string(),
            }));
    });

    cx.update(|window, cx| {
        app.update(cx, |app, _cx| {
            app.execute_agent_card_command_for_e2e(
                window,
                CardCommand::CreateAgent {
                    name: Some(agent_name.to_string()),
                    profile,
                    activate: false,
                },
            )
        })
    })
    .expect("真实 Agent L3 应创建未激活定义");
    let dispatched = cx
        .update(|window, cx| {
            app.update(cx, |app, _cx| {
                app.execute_agent_card_command_for_e2e(
                    window,
                    CardCommand::DispatchAgentTasks {
                        assignments: vec![AgentTaskAssignment {
                            name: agent_name.to_string(),
                            task: "Reply exactly NORMA_SUBMITTED".to_string(),
                        }],
                        force_untracked: false,
                    },
                )
            })
        })
        .expect("真实 Agent L3 派发应成功");
    let CardCommandResult::AgentTasksDispatched { receipts } = dispatched else {
        panic!("真实 Agent L3 派发必须返回 AgentTasksDispatched");
    };
    assert_eq!(receipts.len(), 1, "真实 Agent L3 单项派发应产生 1 个回执");
    let receipt = &receipts[0];
    assert_eq!(
        receipt.status,
        AgentTaskReceiptStatus::Queued,
        "真实 CLI 尚未 Ready 时不得误报 Submitted"
    );
    let node_id = receipt.node_id;

    let observed = drain_until(
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
                (text.matches("NORMA_SUBMITTED").count() >= 2).then_some(text)
            }
            _ => None,
        },
        Duration::from_millis(90_000),
    );
    let pending = app.update(cx, |app, _cx| app.agent_submission_waiting_for_e2e(node_id));
    let final_grid = read_grid(&app, cx, node_id)
        .map(|snapshot| {
            snapshot
                .rows
                .iter()
                .flatten()
                .map(|cell| cell.c)
                .collect::<String>()
        })
        .unwrap_or_else(|| "<missing agent grid>".to_string());
    app.update(cx, |app, _cx| app.runtime.kill_all());

    let observed = observed.unwrap_or_else(|| {
        panic!(
            "真实 CLI 应在 90 秒内经 30 秒无 Hook 兜底提交任务并返回固定回复；pending={pending}，grid={final_grid:?}"
        )
    });
    assert!(
        observed.matches("NORMA_SUBMITTED").count() >= 2,
        "真实 CLI grid 必须同时显示已提交输入和 Agent 固定回复，实际：{observed:?}"
    );
    assert!(!pending, "真实 CLI 收到正文与回车后不得残留 pending 状态");
}
