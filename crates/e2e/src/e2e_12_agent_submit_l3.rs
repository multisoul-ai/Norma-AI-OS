//! Agent 任务真实 CLI 提交冒烟（L3，默认忽略）。

use gpui::TestAppContext;
use norma::protocol::{
    AgentProfile as WireAgentProfile, AgentTaskAssignment, AgentTaskReceiptStatus, CardCommand,
    CardCommandResult,
};
use std::time::Duration;

use crate::harness::{drain_until, enter_canvas, make_app_real, read_grid};

/// E2E-AGENT-SUBMIT-L3：真实 Agent CLI 必须经生产 PTY 收到任务正文和回车。
///
/// 数据构造（含关键数值的推导过程）：
///   `NORMA_E2E_AGENT_CLI` = codex/claude/cursor/opencode 四选一；
///   real PTY factory = None，任务正文包含唯一标记 `NORMA_SUBMITTED`；
///   startup fallback = 30,000ms；observation timeout = 90,000ms = 3 × fallback，
///   可覆盖 CLI 启动、无 Hook 兜底提交、PTY 回写三段异步链路。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用隔离测试数据库构造器与真实 `portable_pty` 进入临时 workspace；
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
