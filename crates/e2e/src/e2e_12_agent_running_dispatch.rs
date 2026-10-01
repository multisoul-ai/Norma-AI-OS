//! 运行中的卡片直接接收新任务，由 CLI 管理执行顺序。

use gpui::{TestAppContext, VisualTestContext};
use norma::protocol::{AgentProfile, AgentTaskAssignment, CardCommand, CardCommandResult};
use std::time::Duration;

use crate::harness::{drain_until, enter_canvas, make_app, read_runtime, scripted_factory};

fn command(
    app: &gpui::Entity<app::MultisoulApp>,
    cx: &mut VisualTestContext,
    command: CardCommand,
) -> CardCommandResult {
    cx.update(|window, cx| {
        app.update(cx, |app, _| {
            app.execute_agent_card_command_for_e2e(window, command)
        })
    })
    .expect("Agent 命令应成功")
}

fn dispatch(app: &gpui::Entity<app::MultisoulApp>, cx: &mut VisualTestContext, task: &str) {
    command(
        app,
        cx,
        CardCommand::DispatchAgentTasks {
            assignments: vec![AgentTaskAssignment {
                name: "Alan".into(),
                task: task.into(),
            }],
            force_untracked: false,
        },
    );
}

fn hook(app: &gpui::Entity<app::MultisoulApp>, cx: &mut VisualTestContext, node: u64, event: &str) {
    let event = norma::agent_hook::parse_hook_event(
        norma::agent_hook::HookSource::Codex, node,
        serde_json::json!({"hook_event_name": event, "session_id": "running-dispatch", "prompt": "initial"}),
    ).expect("Hook 可解析");
    app.update(cx, |app, _| app.enqueue_agent_hook_event_for_e2e(event))
        .expect("Hook 入队");
}

fn text(runtime: &runtime::Runtime, node: u64) -> String {
    match runtime.node_view(node) {
        Some(runtime::NodeView::Term { snapshot, .. }) => {
            snapshot.rows.iter().flatten().map(|c| c.c).collect()
        }
        _ => String::new(),
    }
}

/// 运行期间连续分配两个任务，CLI 在当前任务结束前完整接收且结束后不重发。
///
/// 数据构造：
///   Alan/Codex 卡片 1 张；当前任务 initial，加 later-a/later-b，共 3 项。
///   PTY 每次正文产生独立标记，每次 Enter 产生 SENT；预期 SENT = 1 + 2 = 3。
/// 执行过程：
///   1. 派发 initial，SessionStart 后等待正文与 Enter 送达。
///   2. UserPromptSubmit 将卡片置于 Thinking，连续派发 later-a 与 later-b。
///   3. 不发送 Stop，观察两个正文和各自 Enter；随后发送 Stop 并检查重复发送。
/// 预期结果：
///   - Thinking 时收到全部 3 项，任务跟踪仍保留 3 条。
///   - later-a 和 later-b 各送达一次；Stop 后不得再次写入。
#[test]
fn running_agent_receives_assignments_before_stop_without_duplicate_delivery() {
    let mut test = TestAppContext::single();
    let (app, cx) = make_app(
        &mut test,
        scripted_factory(vec![
            (b"initial".to_vec(), b"INITIAL\r\n".to_vec()),
            (b"later-a".to_vec(), b"LATER_A\r\n".to_vec()),
            (b"later-b".to_vec(), b"LATER_B\r\n".to_vec()),
            (b"\r".to_vec(), b"SENT\r\n".to_vec()),
        ]),
    );
    let workspace = tempfile::tempdir().expect("隔离项目");
    enter_canvas(&app, cx, workspace.path());
    let created = command(
        &app,
        cx,
        CardCommand::CreateAgent {
            name: Some("Alan".into()),
            profile: AgentProfile::Codex,
            activate: true,
        },
    );
    let CardCommandResult::AgentCreated {
        node_id: Some(node),
        ..
    } = created
    else {
        panic!("应激活 Alan 卡片");
    };
    dispatch(&app, cx, "initial");
    hook(&app, cx, node, "SessionStart");
    // 1s = 10 × 100ms Enter 稳定时间，覆盖 Hook、两阶段写入和 grid drain。
    drain_until(
        &app,
        cx,
        |runtime| text(runtime, node).contains("SENT").then_some(()),
        Duration::from_secs(1),
    )
    .expect("首任务应提交");
    hook(&app, cx, node, "UserPromptSubmit");
    // 500ms / 12ms 每帧 ≈ 41 帧，足够消费单个 Hook。
    drain_until(
        &app,
        cx,
        |runtime| {
            (runtime.agent_task_state(node) == Some(runtime::AgentTaskState::Thinking))
                .then_some(())
        },
        Duration::from_millis(500),
    )
    .expect("首任务应正在运行");
    dispatch(&app, cx, "later-a");
    dispatch(&app, cx, "later-b");
    // 1s > 两个任务 × 100ms Enter 延迟；只等待输入传输，不等待任务完成。
    let delivered = drain_until(
        &app,
        cx,
        |runtime| {
            let output = text(runtime, node);
            (output.contains("LATER_A")
                && output.contains("LATER_B")
                && output.matches("SENT").count() == 3)
                .then_some(output)
        },
        Duration::from_secs(1),
    )
    .expect("Thinking 期间两个后续任务必须进入 CLI");
    assert_eq!(
        delivered.matches("LATER_A").count(),
        1,
        "later-a 必须仅送达一次"
    );
    assert_eq!(
        delivered.matches("LATER_B").count(),
        1,
        "later-b 不得覆盖 later-a 或重复"
    );
    assert_eq!(
        read_runtime(&app, cx, |runtime| runtime.agent_task_state(node)),
        Some(runtime::AgentTaskState::Thinking),
        "送达不代表首任务结束"
    );
    assert_eq!(
        app.update(cx, |app, _| app.agent_task_count_for_e2e()),
        3,
        "全部任务编号必须保留"
    );
    hook(&app, cx, node, "Stop");
    // 500ms = 5 × 100ms，覆盖旧逻辑在 Stop 后再次提交的窗口。
    let duplicate = drain_until(
        &app,
        cx,
        |runtime| {
            let output = text(runtime, node);
            (output.matches("LATER_A").count() > 1
                || output.matches("LATER_B").count() > 1
                || output.matches("SENT").count() > 3)
                .then_some(output)
        },
        Duration::from_millis(500),
    );
    assert!(
        duplicate.is_none(),
        "Stop 后不得重发已交给 CLI 的任务：{duplicate:?}"
    );
}
