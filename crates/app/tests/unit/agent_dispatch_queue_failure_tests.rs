use super::execute_agent_card_command;
use gpui::TestAppContext;
use norma::protocol::{
    AgentProfile, AgentTaskAssignment, AgentTaskReceiptStatus, CardCommand, CardCommandResult,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use terminal::{PtyBackend, PtyError};

struct FailSecondEnterPtyBackend {
    writes: Arc<Mutex<Vec<Vec<u8>>>>,
    enter_count: Arc<AtomicUsize>,
}

impl PtyBackend for FailSecondEnterPtyBackend {
    fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        if bytes == [0x0d] && self.enter_count.fetch_add(1, Ordering::AcqRel) == 1 {
            return Err(PtyError::Io(std::io::Error::other(
                "injected queued Enter write failure",
            )));
        }
        self.writes
            .lock()
            .expect("queue failure PTY writes mutex should not be poisoned")
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

/// ART-7e：排队任务 Enter 失败且安全清行后，必须重试队首而不越过 FIFO。
///
/// 数据构造（含关键数值的推导过程）：
///   tasks = 3 项：task-1 Submitted，task-2/task-3 Queued
///   injected failure = 第 2 次 Enter，即 task-2 自动推进时失败；
///   expected writes = task-1 + CR + task-2 + Ctrl+U + task-2 + CR，共 6 次。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Alan 并显式标记 CLI Ready，提交 task-1 占用 Alan，再依次排队 task-2 与 task-3；
///   2. 发送 task-1 的延迟 Enter，再完成 task-1 并推进队列；
///   3. 写入 task-2，随后其 Enter 注入失败并安全清行；
///   4. 自动重写 task-2 并再次发送 Enter。
///
/// 预期结果：
///   - 正断言：两项后续任务最初都为 Queued，task-2 自动重试成功；
///   - 正断言：写入顺序包含 task-2 后的 Ctrl+U 和完整 task-2 重写；
///   - 负断言：task-3 不得越过仍可安全重试的 task-2。
#[test]
fn test_queued_submission_failure_retries_same_fifo_head_before_next_task() {
    let writes = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let enter_count = Arc::new(AtomicUsize::new(0));
    let factory_writes = Arc::clone(&writes);
    let factory_enter_count = Arc::clone(&enter_count);
    let factory: runtime::PtyFactory = Box::new(move |_cmd, _cols, _rows, _tx| {
        Ok(Box::new(FailSecondEnterPtyBackend {
            writes: Arc::clone(&factory_writes),
            enter_count: Arc::clone(&factory_enter_count),
        }) as Box<dyn PtyBackend>)
    });
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("队列失败测试应创建临时 workspace");
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
    let ready_node_id = match &create_result {
        Ok(CardCommandResult::AgentCreated {
            node_id: Some(node_id),
            ..
        }) => *node_id,
        other => panic!("测试前置必须创建活跃 Alan，实际：{other:?}"),
    };
    let marked_ready = app.update(cx, |app, _cx| {
        app.pending_agent_submissions.mark_ready(ready_node_id)
    });
    assert!(marked_ready, "FIFO 失败测试必须先把 Alan 标记为 CLI Ready");

    let first = dispatch(&app, cx, "task-1");
    let second = dispatch(&app, cx, "task-2");
    let third = dispatch(&app, cx, "task-3");
    let node_id = match &first {
        Ok(CardCommandResult::AgentTasksDispatched { receipts }) => receipts[0].node_id,
        other => panic!("首项任务应返回 node_id，实际：{other:?}"),
    };
    let first_submitted = app.update(cx, |app, _cx| {
        app.submit_pending_agent_task_for_ready_node(node_id)
    });
    let advanced = app.update(cx, |app, _cx| app.advance_agent_task_queue(node_id));
    let second_typed = app.update(cx, |app, _cx| {
        app.submit_pending_agent_task_for_ready_node(node_id)
    });
    let second_enter_failed = app.update(cx, |app, _cx| {
        app.submit_pending_agent_task_for_ready_node(node_id)
    });
    let second_retyped = app.update(cx, |app, _cx| {
        app.submit_pending_agent_task_for_ready_node(node_id)
    });
    let second_submitted = app.update(cx, |app, _cx| {
        app.submit_pending_agent_task_for_ready_node(node_id)
    });
    let actual_writes = writes.lock().expect("队列失败写入记录应可读取").clone();

    assert!(
        matches!(create_result, Ok(CardCommandResult::AgentCreated { .. })),
        "测试前置条件必须成功创建 Alan，实际：{create_result:?}"
    );
    assert!(
        matches!(
            second,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Queued
        ),
        "task-2 应先进入 FIFO 队列，实际：{second:?}"
    );
    assert!(
        matches!(
            third,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Queued
        ),
        "task-3 应排在 task-2 之后，实际：{third:?}"
    );
    assert!(first_submitted, "task-1 正文后必须发送延迟 Enter");
    assert!(advanced, "task-1 完成后应推进到 task-2");
    assert!(second_typed, "队列推进后必须写入 task-2 正文");
    assert!(
        !second_enter_failed,
        "task-2 第一次 Enter 注入失败时不得报告成功"
    );
    assert!(second_retyped, "task-2 安全清行后必须自动重写");
    assert!(second_submitted, "task-2 重写后必须再次发送 Enter 并成功");
    assert_eq!(
        actual_writes,
        vec![
            b"task-1".to_vec(),
            vec![0x0d],
            b"task-2".to_vec(),
            vec![0x15],
            b"task-2".to_vec(),
            vec![0x0d],
        ],
        "FIFO 推进应清除失败输入并完整重试同一队首"
    );
    assert_ne!(
        actual_writes.last(),
        Some(&vec![0x15]),
        "队列不得停在 task-2 的失败清行，task-2 必须完成自动重试"
    );
    assert!(
        !actual_writes.iter().any(|bytes| bytes == b"task-3"),
        "task-2 成功终态前 task-3 不得越过 FIFO 提前写入"
    );
}
