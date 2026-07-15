use super::execute_agent_card_command;
use gpui::TestAppContext;
use norma::protocol::{
    AgentProfile, AgentTaskAssignment, AgentTaskReceiptStatus, CardCommand, CardCommandResult,
};
use std::sync::{Arc, Mutex};
use terminal::{PtyBackend, PtyError};

struct RejectEnterAndClearPtyBackend {
    writes: Arc<Mutex<Vec<Vec<u8>>>>,
}

struct MixedCleanupFailurePtyBackend {
    writes: Arc<Mutex<Vec<Vec<u8>>>>,
    clear_attempts: usize,
}

impl PtyBackend for MixedCleanupFailurePtyBackend {
    fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        if bytes == [0x0d] {
            return Err(PtyError::Io(std::io::Error::other(
                "injected Enter failure",
            )));
        }
        if bytes == [0x15] {
            self.clear_attempts += 1;
            if self.clear_attempts == 2 {
                return Err(PtyError::Io(std::io::Error::other(
                    "injected second Ctrl+U failure",
                )));
            }
        }
        self.writes
            .lock()
            .expect("mixed cleanup writes mutex should not be poisoned")
            .push(bytes.to_vec());
        Ok(())
    }

    fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), PtyError> {
        Ok(())
    }

    fn kill(&mut self) {}
}

impl PtyBackend for RejectEnterAndClearPtyBackend {
    fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        if bytes == [0x0d] || bytes == [0x15] {
            return Err(PtyError::Io(std::io::Error::other(
                "injected Enter/Ctrl+U failure",
            )));
        }
        self.writes
            .lock()
            .expect("unsafe failure writes mutex should not be poisoned")
            .push(bytes.to_vec());
        Ok(())
    }

    fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), PtyError> {
        Ok(())
    }

    fn kill(&mut self) {}
}

/// ART-7f：Enter 与 Ctrl+U 都失败时，Agent 必须保持占用，禁止立即重试污染旧输入。
///
/// 数据构造（含关键数值的推导过程）：
///   first task = 正文写入成功 1 次，Enter(0x0d) 失败，Ctrl+U(0x15) 也失败
///   retry task = 1 项；因节点输入状态不安全，只能 Queued，PTY 新写入数 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Alan，并让测试 PTY 拒绝 Enter 与 Ctrl+U；
///   2. 派发 first，得到 Failed 但 cleanup 未完成；
///   3. 立即派发 retry，协调器检查 Alan 仍 busy；
///   4. 检查 retry 回执和 PTY 成功写入记录。
///
/// 预期结果：
///   - 正断言：first 为 Failed，retry 为 Queued；
///   - 正断言：成功写入只有 first 正文；
///   - 负断言：retry 正文不得立即写入并与旧正文拼接。
#[test]
fn test_cleanup_failure_keeps_agent_busy_and_prevents_immediate_retry() {
    let writes = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let factory_writes = Arc::clone(&writes);
    let factory: runtime::PtyFactory = Box::new(move |_cmd, _cols, _rows, _tx| {
        Ok(Box::new(RejectEnterAndClearPtyBackend {
            writes: Arc::clone(&factory_writes),
        }) as Box<dyn PtyBackend>)
    });
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("unsafe failure 测试应创建 workspace");
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
            .expect("测试前置条件应成功创建 Alan");
        })
    });
    let dispatch = |task: &str, cx: &mut gpui::VisualTestContext| {
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
    };

    let first = dispatch("旧正文", cx);
    let retry = dispatch("重试正文", cx);
    let actual_writes = writes
        .lock()
        .expect("unsafe failure writes 应可读取")
        .clone();

    assert!(
        matches!(
            first,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Failed
        ),
        "Enter 与清行失败后首项必须返回 Failed，实际：{first:?}"
    );
    assert!(
        matches!(
            retry,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Queued
        ),
        "输入状态不安全时重试必须 Queued，实际：{retry:?}"
    );
    assert_eq!(
        actual_writes,
        vec!["旧正文".as_bytes().to_vec()],
        "清行失败后不得立即写入重试正文"
    );
    assert!(
        !actual_writes
            .iter()
            .any(|bytes| bytes == "重试正文".as_bytes()),
        "重试正文不得与残留旧正文拼接"
    );
}

/// ART-7g：同一 untracked Agent 的批量失败包含不安全项时，不得因另一安全项释放 busy。
///
/// 数据构造（含关键数值的推导过程）：
///   batch tasks = 2 项，均正文写入成功且 Enter 失败
///   task-1 cleanup = Ctrl+U 第 1 次成功，因此 retry_safe = true
///   task-2 cleanup = Ctrl+U 第 2 次失败，因此 retry_safe = false
///   retry task = 1 项；busy 必须保留，新增 PTY 写入数 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 禁用 Hook，使 Alan 使用 untracked 协调模式；
///   2. 同批派发两项任务，分别制造安全失败与不安全失败；
///   3. 不带 force 立即再次派发；
///   4. 检查批量回执、重试结果及 PTY 写入。
///
/// 预期结果：
///   - 正断言：批量两项均返回 Failed；
///   - 正断言：立即重试因 Alan 仍 busy 而返回 Err；
///   - 负断言：重试正文不得写入残留输入所在 PTY。
#[test]
fn test_mixed_untracked_failures_keep_busy_when_any_cleanup_is_unsafe() {
    let writes = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let factory_writes = Arc::clone(&writes);
    let factory: runtime::PtyFactory = Box::new(move |_cmd, _cols, _rows, _tx| {
        Ok(Box::new(MixedCleanupFailurePtyBackend {
            writes: Arc::clone(&factory_writes),
            clear_attempts: 0,
        }) as Box<dyn PtyBackend>)
    });
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("mixed failure 测试应创建 workspace");
    let (app, cx) = gpui
        .add_window_view(|_window, cx| crate::app::MultisoulApp::new_with_pty_factory(cx, factory));
    app.update(cx, |app, cx| {
        app.disable_agent_hook_for_test();
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
            .expect("测试前置条件应成功创建 Alan");
        })
    });
    let dispatch = |assignments: Vec<AgentTaskAssignment>, cx: &mut gpui::VisualTestContext| {
        cx.update(|window, app_cx| {
            app.update(app_cx, |app, _cx| {
                execute_agent_card_command(
                    app,
                    window,
                    CardCommand::DispatchAgentTasks {
                        assignments,
                        force_untracked: false,
                    },
                )
            })
        })
    };

    let batch = dispatch(
        vec![
            AgentTaskAssignment {
                name: "Alan".to_string(),
                task: "安全失败正文".to_string(),
            },
            AgentTaskAssignment {
                name: "Alan".to_string(),
                task: "不安全失败正文".to_string(),
            },
        ],
        cx,
    );
    let retry = dispatch(
        vec![AgentTaskAssignment {
            name: "Alan".to_string(),
            task: "不得写入的重试正文".to_string(),
        }],
        cx,
    );
    let actual_writes = writes
        .lock()
        .expect("mixed failure writes 应可读取")
        .clone();

    assert!(
        matches!(
            batch,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 2
                    && receipts[0].status == AgentTaskReceiptStatus::Failed
                    && receipts[1].status == AgentTaskReceiptStatus::Failed
        ),
        "混合失败批次的两项回执都必须为 Failed，实际：{batch:?}"
    );
    assert!(
        retry.is_err(),
        "任一 cleanup 不安全时 untracked Agent 必须保持 busy，实际：{retry:?}"
    );
    assert!(
        !actual_writes
            .iter()
            .any(|bytes| bytes == "不得写入的重试正文".as_bytes()),
        "不安全失败后不得立即写入重试正文"
    );
    assert_eq!(
        actual_writes,
        vec![
            "安全失败正文".as_bytes().to_vec(),
            vec![0x15],
            "不安全失败正文".as_bytes().to_vec(),
        ],
        "写入应停在未清理的不安全正文，不得包含重试任务"
    );
}
