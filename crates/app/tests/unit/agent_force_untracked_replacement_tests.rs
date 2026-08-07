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
            .expect("force-untracked PTY 记录锁不应中毒")
            .push(bytes.to_vec());
        Ok(())
    }

    fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), PtyError> {
        Ok(())
    }

    fn kill(&mut self) {}
}

/// 强制替换 Untracked pending：旧正文必须先清除，旧 task_ref 保留 interrupted。
///
/// 数据构造（含关键数值的推导过程）：
///   active Untracked tasks = first(1) + forced second(1)
///   first pending stage     = SubmitEnter（正文已写、Enter 尚未写）
///   cleanup writes          = Ctrl+U = 0x15 × 1
///   expected writes         = first text + Ctrl+U + second text = 3
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 关闭测试 Hook，使 Codex 成为 Untracked，并创建就绪的 Alan。
///   2. 派发 first，生产路径立即写正文并等待 100ms Enter。
///   3. 在 Enter 前以 force-untracked 派发 second。
///   4. 查询旧 task_ref，并检查 PTY 分段写入。
///
/// 预期结果：
///   - 正断言：旧任务为 interrupted，新任务为 Untracked。
///   - 正断言：PTY 先写 first，再写 Ctrl+U，最后写 second。
///   - 负断言：旧 pending 不得晚到发送 Enter，也不得从查询历史消失。
#[test]
fn test_force_untracked_clears_typed_pending_before_replacement() {
    let writes = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let factory_writes = Arc::clone(&writes);
    let factory: runtime::PtyFactory = Box::new(move |_cmd, _cols, _rows, _tx| {
        Ok(Box::new(RecordingPtyBackend {
            writes: Arc::clone(&factory_writes),
        }) as Box<dyn PtyBackend>)
    });
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("force-untracked 测试必须创建临时 workspace");
    let (app, cx) = gpui
        .add_window_view(|_window, cx| crate::app::MultisoulApp::new_with_pty_factory(cx, factory));
    app.update(cx, |app, cx| {
        app.disable_agent_hook_for_test();
        app.enter_canvas_for_test(workspace.path(), cx);
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
    let node_id = match created {
        Ok(CardCommandResult::AgentCreated {
            node_id: Some(node_id),
            ..
        }) => node_id,
        other => panic!("测试前置必须创建活跃 Alan，实际：{other:?}"),
    };
    app.update(cx, |app, _cx| {
        assert!(
            app.pending_agent_submissions.mark_ready(node_id),
            "测试必须先移除 Agent 启动等待门"
        );
    });
    let dispatch = |task: &str, force_untracked: bool, cx: &mut gpui::VisualTestContext| {
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
                        force_untracked,
                    },
                )
            })
        })
    };
    let first = dispatch("first", false, cx);
    let first_task_ref = match &first {
        Ok(CardCommandResult::AgentTasksDispatched { receipts }) => receipts[0].task_id.clone(),
        other => panic!("首项 Untracked 派发必须成功，实际：{other:?}"),
    };
    let second = dispatch("second", true, cx);
    let old_status = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            execute_agent_card_command(
                app,
                window,
                CardCommand::AgentTaskStatus {
                    task_ref: first_task_ref,
                },
            )
        })
    });
    let actual_writes = writes
        .lock()
        .expect("force-untracked PTY 写入必须可读取")
        .clone();

    assert!(
        matches!(
            first,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1
                    && receipts[0].status == AgentTaskReceiptStatus::Queued
                    && receipts[0].tracking == "untracked"
        ),
        "首项无 Hook 任务在等待 Enter 时必须返回 Queued + untracked，实际：{first:?}"
    );
    assert!(
        matches!(
            second,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1
                    && receipts[0].status == AgentTaskReceiptStatus::Queued
                    && receipts[0].tracking == "untracked"
        ),
        "显式 force 后新任务必须成为唯一 Queued + untracked active，实际：{second:?}"
    );
    assert!(
        matches!(
            old_status,
            Ok(CardCommandResult::AgentTaskStatus { ref task })
                if task.status == AgentTaskReceiptStatus::Interrupted
        ),
        "被替换旧任务必须保留为可查询 interrupted，实际：{old_status:?}"
    );
    assert_eq!(
        actual_writes,
        vec![b"first".to_vec(), vec![0x15], b"second".to_vec()],
        "force 替换必须按 first → Ctrl+U → second 顺序清理输入"
    );
    assert!(
        !actual_writes.iter().any(|bytes| bytes == &[0x0d]),
        "force 替换窗口内旧 pending 不得晚到发送 Enter"
    );
}
