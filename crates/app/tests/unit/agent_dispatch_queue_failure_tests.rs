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

/// ART-7e：排队任务提交失败后必须继续 FIFO，不能让后续任务永久搁置。
///
/// 数据构造（含关键数值的推导过程）：
///   tasks = 3 项：task-1 Submitted，task-2/task-3 Queued
///   injected failure = 第 2 次 Enter，即 task-2 自动推进时失败
///   expected writes = task-1 + CR + task-2 + Ctrl+U + task-3 + CR，共 6 次
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 提交 task-1 占用 Alan，再依次排队 task-2 与 task-3；
///   2. 完成 task-1 并推进队列，task-2 的 Enter 注入失败；
///   3. 系统清除 task-2 未提交正文，将其标记失败并继续取 task-3；
///   4. task-3 写入正文与 Enter，成为新的运行中任务。
///
/// 预期结果：
///   - 正断言：两项后续任务最初都为 Queued，推进最终返回 true；
///   - 正断言：写入顺序包含 task-2 后的 Ctrl+U 和完整 task-3；
///   - 负断言：task-3 不得因 task-2 失败而永久留在队列。
#[test]
fn test_queued_submission_failure_skips_failed_head_and_submits_next_fifo_task() {
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

    let first = dispatch(&app, cx, "task-1");
    let second = dispatch(&app, cx, "task-2");
    let third = dispatch(&app, cx, "task-3");
    let node_id = match &first {
        Ok(CardCommandResult::AgentTasksDispatched { receipts }) => receipts[0].node_id,
        other => panic!("首项任务应返回 node_id，实际：{other:?}"),
    };
    let advanced = app.update(cx, |app, _cx| app.advance_agent_task_queue(node_id));
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
    assert!(advanced, "task-2 提交失败后应继续推进并成功提交 task-3");
    assert_eq!(
        actual_writes,
        vec![
            b"task-1".to_vec(),
            vec![0x0d],
            b"task-2".to_vec(),
            vec![0x15],
            b"task-3".to_vec(),
            vec![0x0d],
        ],
        "FIFO 推进应清除失败队首，再完整提交下一项"
    );
    assert_ne!(
        actual_writes.last(),
        Some(&vec![0x15]),
        "队列不得停在 task-2 的失败清行，task-3 必须继续提交"
    );
}
