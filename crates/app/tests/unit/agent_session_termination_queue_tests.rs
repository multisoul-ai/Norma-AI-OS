use super::execute_agent_card_command;
use gpui::TestAppContext;
use norma::protocol::{
    AgentProfile, AgentTaskAssignment, AgentTaskReceiptStatus, CardCommand, CardCommandResult,
};
use std::sync::{Arc, Mutex};
use terminal::{PtyBackend, PtyError};

struct RecordingPtyBackend {
    writes: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl PtyBackend for RecordingPtyBackend {
    fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        self.writes
            .lock()
            .expect("Agent 终止测试的 PTY 写入记录锁不应中毒")
            .push(bytes.to_vec());
        Ok(())
    }

    fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), PtyError> {
        Ok(())
    }

    fn kill(&mut self) {}
}

fn dispatch(
    app: &gpui::Entity<crate::app::MultisoulApp>,
    cx: &mut gpui::VisualTestContext,
    task: &str,
) -> Result<CardCommandResult, String> {
    cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            execute_agent_card_command(
                app,
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Alan".to_string(),
                        task: task.to_string(),
                    }],
                    force_untracked: false,
                },
            )
        })
    })
}

/// Agent 会话直接终止：必须清除当前任务、等待队列和 pending，随后 Norma 新派发可立即执行。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent definitions = 1（Alan/Codex），active cards = 1；
///   终止前任务 = task-1（当前）+ task-2（等待）= 2 项；
///   终止后新任务 = task-3 = 1 项；
///   每项真正提交 = 正文 write 1 次 + Enter CR(0x0d) write 1 次；
///   expected writes = task-1 的 2 次 + task-3 的 2 次 = 4 次，task-2 为 0 次。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建并激活 Alan，标记 CLI Ready，经生产 Norma command seam 派发并提交 task-1；
///   2. 在 task-1 占用 Agent 时派发 task-2，使其进入 managed FIFO 等待队列；
///   3. 注入生产 SessionEnd Hook，并调用生产终态收口 advance_agent_task_queue；
///   4. 再经同一 Norma seam 派发 task-3，并推进 pending 的 Enter 阶段；
///   5. 检查公开回执、pending 状态和真实 PTY 写入顺序。
///
/// 预期结果：
///   - 正断言：终止收口发生变化，且 task-3 成为当前可提交任务；
///   - 正断言：PTY 只收到 task-1 与 task-3，各自正文和 Enter，共 4 次；
///   - 负断言：终止后不得残留 pending，task-2 不得被提升或写入 PTY；
///   - 负断言：task-3 不得继续排在已终止会话的旧 task-2 后面。
#[test]
fn test_session_end_clears_waiting_tasks_before_next_norma_dispatch() {
    let writes = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let factory_writes = Arc::clone(&writes);
    let factory: runtime::PtyFactory = Box::new(move |_cmd, _cols, _rows, _tx| {
        Ok(Box::new(RecordingPtyBackend {
            writes: Arc::clone(&factory_writes),
        }) as Box<dyn PtyBackend>)
    });
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("Agent 终止测试应创建临时 workspace");
    let (app, cx) = gpui
        .add_window_view(|_window, cx| crate::app::MultisoulApp::new_with_pty_factory(cx, factory));
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    let created = cx.update(|window, app_cx| {
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
    let node_id = match &created {
        Ok(CardCommandResult::AgentCreated {
            node_id: Some(node_id),
            ..
        }) => *node_id,
        other => panic!("测试前置必须创建活跃 Alan，实际：{other:?}"),
    };
    let marked_ready = app.update(cx, |app, _cx| {
        app.pending_agent_submissions.mark_ready(node_id)
    });
    assert!(marked_ready, "终止队列测试必须先把 Alan 标记为 CLI Ready");

    let first = dispatch(&app, cx, "task-1");
    let first_submitted = app.update(cx, |app, _cx| {
        app.submit_pending_agent_task_for_ready_node(node_id)
    });
    let second = dispatch(&app, cx, "task-2");
    assert!(
        matches!(
            second,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Queued
        ),
        "task-1 忙碌时 task-2 必须进入唯一等待位，实际：{second:?}"
    );

    let ended = norma::agent_hook::parse_hook_event(
        norma::agent_hook::HookSource::Codex,
        node_id,
        serde_json::json!({
            "hook_event_name": "SessionEnd",
            "session_id": "terminated-session"
        }),
    )
    .expect("SessionEnd 测试事件应解析成功");
    let state_changed = app.update(cx, |app, _cx| {
        crate::settings::agent_hooks::apply_agent_hook_event(&mut app.runtime, ended)
    });
    let termination_applied = app.update(cx, |app, _cx| app.advance_agent_task_queue(node_id));
    let pending_after_termination = app.update(cx, |app, _cx| {
        app.pending_agent_submissions.is_waiting(node_id)
    });

    assert!(
        matches!(
            first,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Queued
        ),
        "task-1 在 Enter 提交前应返回唯一 Queued 回执，实际：{first:?}"
    );
    assert!(first_submitted, "task-1 正文写入后必须发送 Enter 完成提交");
    assert!(
        state_changed,
        "SessionEnd 必须把 Agent runtime 状态更新为 Ended"
    );
    assert!(
        termination_applied,
        "SessionEnd 必须触发任务协调器和 pending 队列的终止收口"
    );
    assert!(
        !pending_after_termination,
        "SessionEnd 后不得把旧 task-2 提升为 pending 提交任务"
    );

    let third = dispatch(&app, cx, "task-3");
    let third_submitted = app.update(cx, |app, _cx| {
        app.submit_pending_agent_task_for_ready_node(node_id)
    });
    let actual_writes = writes
        .lock()
        .expect("Agent 终止测试的 PTY 写入记录应可读取")
        .clone();

    assert!(
        matches!(
            third,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Queued
        ),
        "终止清队后 task-3 应成为唯一可提交任务，实际：{third:?}"
    );
    assert!(
        third_submitted,
        "终止清队后 Norma 新派发的 task-3 必须能发送 Enter 完成提交"
    );
    assert_eq!(
        actual_writes,
        vec![
            b"task-1".to_vec(),
            vec![0x0d],
            b"task-3".to_vec(),
            vec![0x0d],
        ],
        "PTY 应只收到 task-1 与终止后新派发的 task-3，旧 task-2 必须被清除"
    );
    assert!(
        !actual_writes.iter().any(|bytes| bytes == b"task-2"),
        "已终止会话的等待任务 task-2 不得被提升或写入 PTY"
    );
}
