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

fn created_agent_node_id(result: &CardCommandResult) -> u64 {
    match result {
        CardCommandResult::AgentCreated {
            node_id: Some(node_id),
            ..
        } => *node_id,
        other => panic!("测试前置必须创建活跃 Agent，实际：{other:?}"),
    }
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

/// ART-7h：等待 CLI Ready 的任务发生不安全提交失败时，必须给出可见错误并保持占用。
///
/// 数据构造（含关键数值的推导过程）：
///   startup pending tasks = 1；Ready 前 PTY writes = 0；
///   Ready 后第 1 阶段 writes = 正文 1 次；第 2 阶段 Enter → CR(13 = 0x0d) 失败；
///   cleanup = Ctrl+U → ASCII `u`(0x75) & 0x1f = 0x15，也失败；
///   retry tasks = 1，因残留输入无法安全清理，新增 PTY writes = 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建启动中的 Alan，并派发“旧正文”，得到 Queued；
///   2. 模拟 CLI Ready，生产 pending 第一次推进只写正文；
///   3. 第二次推进发送 Enter，但 Enter 与清行均失败；
///   4. 读取 workspace 可见错误；
///   5. 立即派发“重试正文”，检查协调器仍保持 busy。
///
/// 预期结果：
///   - 正断言：首次回执为 Queued，正文阶段成功、Enter 阶段失败，并显示“提交失败”错误；
///   - 正断言：重试仍为 Queued，等待用户处理不安全输入；
///   - 负断言：不得写入重试正文，也不得释放 busy 后误报 Submitted。
#[test]
fn test_pending_unsafe_failure_is_visible_and_does_not_release_busy() {
    let writes = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let factory_writes = Arc::clone(&writes);
    let factory: runtime::PtyFactory = Box::new(move |_cmd, _cols, _rows, _tx| {
        Ok(Box::new(RejectEnterAndClearPtyBackend {
            writes: Arc::clone(&factory_writes),
        }) as Box<dyn PtyBackend>)
    });
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("pending unsafe 测试应创建 workspace");
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
    let node_id = match create_result {
        Ok(CardCommandResult::AgentCreated {
            node_id: Some(node_id),
            ..
        }) => node_id,
        other => panic!("测试前置必须创建启动中的 Alan，实际：{other:?}"),
    };
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

    let queued = dispatch("旧正文", cx);
    let typed = app.update(cx, |app, _cx| {
        app.submit_pending_agent_task_for_ready_node(node_id)
    });
    let submitted = app.update(cx, |app, _cx| {
        app.submit_pending_agent_task_for_ready_node(node_id)
    });
    let visible_error = app.update(cx, |app, _cx| app.workspace_state.error.clone());
    let first_task_ref = match &queued {
        Ok(CardCommandResult::AgentTasksDispatched { receipts }) => receipts[0].task_id.clone(),
        other => panic!("首次 Queued 回执前置条件已断言成功，实际：{other:?}"),
    };
    let failed_status = cx.update(|window, app_cx| {
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
    let retry = dispatch("重试正文", cx);
    let actual_writes = writes
        .lock()
        .expect("pending unsafe writes 应可读取")
        .clone();

    assert!(
        matches!(
            queued,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Queued
        ),
        "启动中的 Alan 首项任务必须返回 Queued，实际：{queued:?}"
    );
    assert!(typed, "CLI Ready 后第一阶段必须成功写入任务正文");
    assert!(!submitted, "Enter 与清行失败后 pending 提交必须报告未成功");
    assert!(
        matches!(
            failed_status,
            Ok(CardCommandResult::AgentTaskStatus { ref task })
                if task.status == AgentTaskReceiptStatus::Failed
        ),
        "Enter 与清行均失败后首项任务必须保留为可查询 Failed，实际：{failed_status:?}"
    );
    assert!(
        !matches!(
            failed_status,
            Ok(CardCommandResult::AgentTaskStatus { ref task })
                if task.status == AgentTaskReceiptStatus::Submitted
        ),
        "不安全提交失败后首项任务不得继续伪装为 Submitted"
    );
    assert!(
        visible_error.as_deref().is_some_and(|message| {
            message.contains("提交失败")
                && message.contains("连续关闭两次")
                && message.contains("重新激活")
        }),
        "不安全失败必须给出可执行的关闭、清空与重新激活恢复入口，实际：{visible_error:?}"
    );
    assert!(
        matches!(
            retry,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Queued
        ),
        "不安全失败后重试必须保持 Queued，不能立即污染输入，实际：{retry:?}"
    );
    assert_eq!(
        actual_writes,
        vec!["旧正文".as_bytes().to_vec()],
        "不安全失败后成功写入只能有旧正文，不得包含重试正文"
    );
    assert!(
        !actual_writes
            .iter()
            .any(|bytes| bytes == "重试正文".as_bytes()),
        "不安全失败后不得把重试正文写入残留输入所在 PTY"
    );
}

/// ART-7f：Enter 与 Ctrl+U 都失败时，Agent 必须保持占用，禁止立即重试污染旧输入。
///
/// 数据构造（含关键数值的推导过程）：
///   first task = 正文写入成功 1 次，Enter(0x0d) 失败，Ctrl+U(0x15) 也失败
///   retry task = 1 项；因节点输入状态不安全，只能 Queued，PTY 新写入数 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Alan、显式标记 CLI Ready，并让测试 PTY 拒绝 Enter 与 Ctrl+U；
///   2. 派发 first，正文写入后得到 Queued；
///   3. 推进 Enter 阶段，制造 Enter 与 cleanup 同时失败；
///   4. 立即派发 retry，协调器检查 Alan 仍 busy；
///   5. 检查 retry 回执和 PTY 成功写入记录。
///
/// 预期结果：
///   - 正断言：first 为 Queued、Enter 阶段失败，retry 仍为 Queued；
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
            .expect("测试前置条件应成功创建 Alan")
        })
    });
    let ready_node_id = created_agent_node_id(&create_result);
    let marked_ready = app.update(cx, |app, _cx| {
        app.pending_agent_submissions.mark_ready(ready_node_id)
    });
    assert!(
        marked_ready,
        "cleanup 失败测试必须先把 Alan 标记为 CLI Ready"
    );
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
    let submitted = app.update(cx, |app, _cx| {
        app.submit_pending_agent_task_for_ready_node(ready_node_id)
    });
    let visible_error = app.update(cx, |app, _cx| app.workspace_state.error.clone());
    let retry = dispatch("重试正文", cx);
    let actual_writes = writes
        .lock()
        .expect("unsafe failure writes 应可读取")
        .clone();

    assert!(
        matches!(
            first,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Queued
        ),
        "Enter 尚未发送前首项必须保持 Queued，实际：{first:?}"
    );
    assert!(!submitted, "Enter 与清行同时失败时第二阶段必须报告未成功");
    assert!(
        visible_error.as_deref().is_some_and(|message| {
            message.contains("连续关闭两次") && message.contains("重新激活")
        }),
        "立即提交的不安全失败也必须给出可执行的关闭与重建恢复入口，实际：{visible_error:?}"
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

/// ART-7g：同一 untracked Agent 不得在一个批次接收多项任务，避免两段正文先于 Enter 拼接。
///
/// 数据构造（含关键数值的推导过程）：
///   batch tasks = 2 项，目标均为无 Hook 的 Alan；
///   safe simultaneous capacity = 1，因此 2 > 1，整批必须在 PTY 写入前拒绝；
///   expected PTY writes = 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 禁用 Hook，创建并标记 Alan Ready，使其使用 untracked 协调模式；
///   2. 同批派发两项任务；
///   3. 检查命令在任何正文写入前拒绝整批。
///
/// 预期结果：
///   - 正断言：批量派发返回 Err，明确无 Hook 时一次只能派发一项；
///   - 负断言：两项正文均不得写入 PTY。
#[test]
fn test_untracked_batch_rejects_multiple_tasks_before_typing() {
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
            .expect("测试前置条件应成功创建 Alan")
        })
    });
    let ready_node_id = created_agent_node_id(&create_result);
    let marked_ready = app.update(cx, |app, _cx| {
        app.pending_agent_submissions.mark_ready(ready_node_id)
    });
    assert!(marked_ready, "混合失败测试必须先把 Alan 标记为 CLI Ready");
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
    let actual_writes = writes
        .lock()
        .expect("mixed failure writes 应可读取")
        .clone();

    assert!(
        batch.as_ref().is_err_and(|message| {
            message.contains("没有 Hook 跟踪") && message.contains("一次只能派发一个任务")
        }),
        "无 Hook 的同一 Agent 批量多任务必须在写入前拒绝，实际：{batch:?}"
    );
    assert!(
        actual_writes.is_empty(),
        "无 Hook 批量任务被拒绝后 PTY 不得有任何正文写入，实际：{actual_writes:?}"
    );
    assert!(
        !actual_writes
            .iter()
            .any(|bytes| bytes == "安全失败正文".as_bytes()
                || bytes == "不安全失败正文".as_bytes()),
        "批量拒绝后两项任务正文均不得写入 PTY"
    );
}
