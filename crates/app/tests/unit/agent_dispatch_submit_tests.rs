use super::execute_agent_card_command;
use gpui::TestAppContext;
use norma::protocol::{
    AgentProfile, AgentTaskAssignment, AgentTaskReceiptStatus, CardCommand, CardCommandResult,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use terminal::{PtyBackend, PtyError};

struct RecordingPtyBackend {
    writes: Arc<Mutex<Vec<Vec<u8>>>>,
}

struct FailFirstEnterPtyBackend {
    writes: Arc<Mutex<Vec<Vec<u8>>>>,
    failed_enter: Arc<AtomicBool>,
}

impl PtyBackend for FailFirstEnterPtyBackend {
    fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        if bytes == [0x0d] && !self.failed_enter.swap(true, Ordering::AcqRel) {
            return Err(PtyError::Io(std::io::Error::other(
                "injected Enter write failure",
            )));
        }
        self.writes
            .lock()
            .expect("failing PTY writes mutex should not be poisoned")
            .push(bytes.to_vec());
        Ok(())
    }

    fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), PtyError> {
        Ok(())
    }

    fn kill(&mut self) {}
}

impl PtyBackend for RecordingPtyBackend {
    fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        self.writes
            .lock()
            .expect("recording PTY writes mutex should not be poisoned")
            .push(bytes.to_vec());
        Ok(())
    }

    fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), PtyError> {
        Ok(())
    }

    fn kill(&mut self) {}
}

/// ART-7b：Agent 任务委派必须把正文粘贴与 Enter 键拆成两个 PTY 输入，确保 CLI 真正提交。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent definition = Alan/Codex 1 条
///   task text = "开发贪吃蛇"，5 个汉字 × 3 UTF-8 bytes = 15 bytes
///   Enter = terminal::Key::Enter 编码后的 CR 1 byte（0x0d）
///   expected PTY writes = 2 次：正文一次 + Enter 一次
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用记录每次 write(bytes) 的 PTY 后端启动 headless App
///   2. 通过生产 execute_agent_card_command 创建并激活 Alan
///   3. 通过同一生产入口执行 dispatch_agent_tasks
///   4. 读取 PTY 的分段写入记录，区分文本粘贴与真实 Enter 键
///
/// 预期结果：
///   - 正断言：第一次写入只有任务正文，第二次写入只有 CR
///   - 负断言：不得把正文与 LF 合成一次 Text 写入，否则交互式 Agent CLI 只显示文字而不提交
#[test]
fn test_agent_dispatch_writes_text_then_real_enter_key() {
    let writes = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let factory_writes = Arc::clone(&writes);
    let factory: runtime::PtyFactory = Box::new(move |_cmd, _cols, _rows, _tx| {
        Ok(Box::new(RecordingPtyBackend {
            writes: Arc::clone(&factory_writes),
        }) as Box<dyn PtyBackend>)
    });
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new()
        .expect("Agent dispatch regression test should create a temp workspace");
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
    let dispatch_result = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            execute_agent_card_command(
                app,
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Alan".to_string(),
                        task: "开发贪吃蛇".to_string(),
                    }],
                    force_untracked: false,
                },
            )
        })
    });
    let actual_writes = writes
        .lock()
        .expect("recording PTY writes mutex should remain readable")
        .clone();

    assert!(
        matches!(create_result, Ok(CardCommandResult::AgentCreated { .. })),
        "测试前置条件必须成功创建并激活 Alan；实际结果：{create_result:?}"
    );
    assert!(
        matches!(
            dispatch_result,
            Ok(CardCommandResult::AgentTasksDispatched { .. })
        ),
        "生产 dispatch_agent_tasks 应返回任务回执；实际结果：{dispatch_result:?}"
    );
    assert_eq!(
        actual_writes.len(),
        2,
        "任务委派必须产生正文与 Enter 两次独立 PTY write；实际：{actual_writes:?}"
    );
    assert_eq!(
        actual_writes[0],
        "开发贪吃蛇".as_bytes(),
        "第一次 PTY write 应只粘贴任务正文，不应携带控制字符"
    );
    assert_eq!(
        actual_writes[1],
        vec![0x0d],
        "第二次 PTY write 应是 Key::Enter 编码得到的 CR(0x0d)"
    );
    assert_ne!(
        actual_writes,
        vec!["开发贪吃蛇\n".as_bytes().to_vec()],
        "不得用单次 Text 写入正文加 LF；该路径只会让 Agent CLI 显示文字而不发布"
    );
}

/// ART-7d：PTY 提交失败必须返回结构化失败回执并释放 Agent 运行槽。
///
/// 数据构造（含关键数值的推导过程）：
///   first task writes = 正文 1 次成功 + Enter 1 次注入失败
///   retry writes      = 正文 1 次成功 + Enter 1 次成功
///   recovery write    = Ctrl+U 1 次，用于清除未提交的 first 正文
///   expected successful writes = first 正文 + Ctrl+U + retry 正文 + retry Enter，共 4 次
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Alan，并让测试 PTY 仅在第一次 Enter 写入时返回错误；
///   2. 派发 "第一次任务" → 正文进入 PTY，但 Enter 失败；
///   3. 再派发 "重试任务" → 若 coordinator 已回滚，应立即重新提交；
///   4. 读取两次公开回执和成功写入记录。
///
/// 预期结果：
///   - 正断言：第一次返回 Failed，第二次返回 Submitted；
///   - 正断言：Enter 失败后先写 Ctrl+U，再写重试正文与 Enter，证明旧正文不会污染重试；
///   - 负断言：第一次不得返回整个命令 Err，也不得误报 Submitted/Queued。
#[test]
fn test_agent_dispatch_enter_failure_returns_failed_receipt_and_allows_retry() {
    let writes = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let failed_enter = Arc::new(AtomicBool::new(false));
    let factory_writes = Arc::clone(&writes);
    let factory_failed_enter = Arc::clone(&failed_enter);
    let factory: runtime::PtyFactory = Box::new(move |_cmd, _cols, _rows, _tx| {
        Ok(Box::new(FailFirstEnterPtyBackend {
            writes: Arc::clone(&factory_writes),
            failed_enter: Arc::clone(&factory_failed_enter),
        }) as Box<dyn PtyBackend>)
    });
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new()
        .expect("Agent failure rollback test should create a temp workspace");
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

    let failed_dispatch = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            execute_agent_card_command(
                app,
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Alan".to_string(),
                        task: "第一次任务".to_string(),
                    }],
                    force_untracked: false,
                },
            )
        })
    });
    let retry_dispatch = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            execute_agent_card_command(
                app,
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Alan".to_string(),
                        task: "重试任务".to_string(),
                    }],
                    force_untracked: false,
                },
            )
        })
    });
    let actual_writes = writes
        .lock()
        .expect("failure rollback PTY writes mutex should remain readable")
        .clone();

    assert!(
        matches!(create_result, Ok(CardCommandResult::AgentCreated { .. })),
        "测试前置条件必须成功创建 Alan，实际：{create_result:?}"
    );
    assert!(
        matches!(
            failed_dispatch,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Failed
        ),
        "Enter 写入失败应成为结构化 Failed 回执而非整条命令 Err，实际：{failed_dispatch:?}"
    );
    assert!(
        matches!(
            retry_dispatch,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Submitted
        ),
        "失败后 coordinator 应释放 busy 槽，使重试立即 Submitted，实际：{retry_dispatch:?}"
    );
    assert_eq!(
        actual_writes,
        vec![
            "第一次任务".as_bytes().to_vec(),
            vec![0x15],
            "重试任务".as_bytes().to_vec(),
            vec![0x0d],
        ],
        "第一次正文后必须用 Ctrl+U(0x15) 清行；重试再完整写入正文与 Enter"
    );
    assert!(
        !matches!(
            failed_dispatch,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.iter().any(|receipt| matches!(
                    receipt.status,
                    AgentTaskReceiptStatus::Submitted | AgentTaskReceiptStatus::Queued
                ))
        ),
        "PTY 未提交成功时不得误报 Submitted 或 Queued"
    );
}

/// ART-7c：忙碌 Agent 的排队任务自动推进时，也必须写入正文后发送真实 Enter。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent definition = Alan/Codex 1 条，Hook server 使其使用 managed queue
///   task-1 = "先检查项目"，5 个汉字 × 3 UTF-8 bytes = 15 bytes
///   task-2 = "再开发贪吃蛇"，6 个汉字 × 3 UTF-8 bytes = 18 bytes
///   每个已提交任务 = 正文 write 1 次 + Enter CR(0x0d) write 1 次
///   expected PTY writes = 2 tasks × 2 writes/task = 4 次
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建并激活 Alan，第一次 dispatch 立即提交 task-1，使 Alan 进入 busy
///   2. 第二次 dispatch task-2 → managed coordinator 将其标为 Queued，不写 PTY
///   3. 调用生产队列完成边界 advance_agent_task_queue → 取出 task-2 并自动提交
///   4. 读取 PTY 分段写入，验证两项任务都使用正文与真实 Enter 的相同协议
///
/// 预期结果：
///   - 正断言：task-1 为 Submitted、task-2 初始为 Queued，队列推进返回 true
///   - 正断言：四次写入依次为 task-1、CR、task-2、CR
///   - 负断言：task-2 不得退回单次 `正文 + LF` 写入，否则 CLI 只显示而不发布
#[test]
fn test_queued_agent_task_advances_with_text_then_real_enter_key() {
    let writes = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let factory_writes = Arc::clone(&writes);
    let factory: runtime::PtyFactory = Box::new(move |_cmd, _cols, _rows, _tx| {
        Ok(Box::new(RecordingPtyBackend {
            writes: Arc::clone(&factory_writes),
        }) as Box<dyn PtyBackend>)
    });
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new()
        .expect("Agent queue regression test should create a temp workspace");
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
    let first_dispatch = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            execute_agent_card_command(
                app,
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Alan".to_string(),
                        task: "先检查项目".to_string(),
                    }],
                    force_untracked: false,
                },
            )
        })
    });
    let second_dispatch = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            execute_agent_card_command(
                app,
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Alan".to_string(),
                        task: "再开发贪吃蛇".to_string(),
                    }],
                    force_untracked: false,
                },
            )
        })
    });
    let node_id = match &first_dispatch {
        Ok(CardCommandResult::AgentTasksDispatched { receipts }) => receipts[0].node_id,
        other => panic!("首次任务必须成功提交并返回 node_id，实际结果：{other:?}"),
    };
    let queue_advanced = app.update(cx, |app, _cx| app.advance_agent_task_queue(node_id));
    let actual_writes = writes
        .lock()
        .expect("recording PTY writes mutex should remain readable")
        .clone();

    assert!(
        matches!(create_result, Ok(CardCommandResult::AgentCreated { .. })),
        "测试前置条件必须成功创建并激活 Alan；实际结果：{create_result:?}"
    );
    assert!(
        matches!(
            &first_dispatch,
            Ok(CardCommandResult::AgentTasksDispatched { receipts })
                if receipts.len() == 1
                    && receipts[0].status == AgentTaskReceiptStatus::Submitted
        ),
        "首项 managed 任务应立即提交，实际结果：{first_dispatch:?}"
    );
    assert!(
        matches!(
            &second_dispatch,
            Ok(CardCommandResult::AgentTasksDispatched { receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Queued
        ),
        "忙碌 Agent 的第二项任务应先进入队列，实际结果：{second_dispatch:?}"
    );
    assert!(queue_advanced, "完成首项后应成功推进并提交下一项排队任务");
    assert_eq!(
        actual_writes.len(),
        4,
        "两项任务都必须产生正文与 Enter 两次写入；实际：{actual_writes:?}"
    );
    assert_eq!(
        actual_writes[0],
        "先检查项目".as_bytes(),
        "首次提交的第一次写入应只包含 task-1 正文"
    );
    assert_eq!(
        actual_writes[1],
        vec![0x0d],
        "首次提交的第二次写入应是 Enter CR(0x0d)"
    );
    assert_eq!(
        actual_writes[2],
        "再开发贪吃蛇".as_bytes(),
        "队列推进的第一次写入应只包含 task-2 正文"
    );
    assert_eq!(
        actual_writes[3],
        vec![0x0d],
        "队列推进的第二次写入应是 Enter CR(0x0d)"
    );
    assert_ne!(
        &actual_writes[2..],
        &["再开发贪吃蛇\n".as_bytes().to_vec()],
        "排队任务不得退回单次正文加 LF 的旧提交协议"
    );
}
