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

fn created_agent_node_id(result: &Result<CardCommandResult, String>) -> u64 {
    match result {
        Ok(CardCommandResult::AgentCreated {
            node_id: Some(node_id),
            ..
        }) => *node_id,
        other => panic!("测试前置必须创建活跃 Agent，实际：{other:?}"),
    }
}

/// ART-7b：新激活 Agent 先等待就绪，收到 SessionStart 后自动写入正文与真实 Enter。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent definition = Alan/Codex 1 条
///   task text = "开发贪吃蛇"，5 个汉字 × 3 UTF-8 bytes = 15 bytes
///   CLI ready events = 先 0 次，随后注入 1 次 Codex SessionStart
///   waiting PTY writes = 0 次；ready PTY writes = 正文 1 次 + Enter CR(0x0d) 1 次 = 2 次
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用记录每次 write(bytes) 的 PTY 后端启动 headless App
///   2. 通过生产 execute_agent_card_command 创建并激活 Alan
///   3. 通过同一生产入口执行 dispatch_agent_tasks
///   4. 不注入 SessionStart，立即读取公开回执与 PTY 写入记录
///   5. 注入同一 node 的 Codex SessionStart，并调用生产 pending 提交入口
///   6. 再读取 PTY 分段写入，区分正文与真实 Enter
///
/// 预期结果：
///   - 正断言：任务回执为 Queued，表示仍在等待 CLI 就绪
///   - 正断言：等待阶段 PTY 写入次数为 0
///   - 正断言：SessionStart 后依次写入正文和 CR，共 2 次
///   - 负断言：不得在等待阶段返回 Submitted，也不得把正文与 LF 合并写入
#[test]
fn test_new_agent_dispatch_waits_then_submits_after_session_start() {
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
    let waiting_writes = writes
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
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1
                    && receipts[0].status == AgentTaskReceiptStatus::Queued
        ),
        "新激活 Agent 未就绪时应返回唯一 Queued 回执；实际结果：{dispatch_result:?}"
    );
    assert_eq!(
        waiting_writes.len(),
        0,
        "CLI 未就绪时不得写入任务正文或 Enter；实际：{waiting_writes:?}"
    );
    assert!(
        !matches!(
            dispatch_result,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.iter().any(|receipt| {
                    matches!(
                        receipt.status,
                        AgentTaskReceiptStatus::Submitted | AgentTaskReceiptStatus::Untracked
                    )
                })
        ),
        "CLI 未就绪时不得误报 Submitted 或 Untracked；实际结果：{dispatch_result:?}"
    );

    let node_id = match &dispatch_result {
        Ok(CardCommandResult::AgentTasksDispatched { receipts }) => receipts[0].node_id,
        other => panic!("Queued 回执前置条件已断言成功，实际：{other:?}"),
    };
    let ready_event = norma::agent_hook::parse_hook_event(
        norma::agent_hook::HookSource::Codex,
        node_id,
        serde_json::json!({
            "hook_event_name": "SessionStart",
            "session_id": "codex-ready-session"
        }),
    )
    .expect("Codex SessionStart 测试事件应解析成功");
    let typed_after_ready = app.update(cx, |app, cx| {
        let changed =
            crate::settings::agent_hooks::apply_agent_hook_event(&mut app.runtime, ready_event);
        assert!(changed, "SessionStart 必须把目标 Agent 状态更新为 Ready");
        app.submit_pending_agent_task_for_ready_node(node_id, cx)
    });
    let submitted_after_delay = app.update(cx, |app, cx| {
        app.submit_pending_agent_task_for_ready_node(node_id, cx)
    });
    let ready_writes = writes
        .lock()
        .expect("ready PTY writes mutex should remain readable")
        .clone();

    assert!(
        typed_after_ready,
        "SessionStart 后必须先找到并写入等待中的 Agent 任务正文"
    );
    assert!(
        submitted_after_delay,
        "正文写入批次稳定后必须发送 Enter 完成提交"
    );
    assert_eq!(
        ready_writes,
        vec!["开发贪吃蛇".as_bytes().to_vec(), vec![0x0d]],
        "就绪后必须先写 15-byte 正文，再写真实 Enter CR(0x0d)"
    );
    assert_ne!(
        ready_writes,
        vec!["开发贪吃蛇\n".as_bytes().to_vec()],
        "不得把正文与 LF 合并成一次粘贴，否则 TUI 可能只保留输入而不提交"
    );
}

/// ART-7d：Enter 写入失败且清行成功时，必须自动重写同一任务并重试提交。
///
/// 数据构造（含关键数值的推导过程）：
///   first task writes = 正文 1 次成功 + Enter 1 次注入失败
///   retry writes      = 同一正文 1 次成功 + Enter 1 次成功
///   recovery write    = Ctrl+U 1 次，用于清除未提交的 first 正文
///   expected successful writes = first 正文 + Ctrl+U + retry 正文 + retry Enter，共 4 次
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Alan，并让测试 PTY 仅在第一次 Enter 写入时返回错误；
///   2. 派发 "第一次任务" → 回执保持 Queued，正文进入 PTY；
///   3. 推进 Enter 阶段，制造失败并用 Ctrl+U 清行；
///   4. 推进自动重试的正文与 Enter 两阶段；
///   5. 读取公开回执和成功写入记录。
///
/// 预期结果：
///   - 正断言：初始回执为 Queued，失败后同一任务自动重写并最终提交；
///   - 正断言：Enter 失败后先写 Ctrl+U，再写原正文与 Enter，证明旧正文不会污染重试；
///   - 负断言：不得要求用户再次派发，也不得在 Enter 成功前误报 Submitted。
#[test]
fn test_agent_dispatch_enter_failure_queues_and_automatically_retries() {
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
    let ready_node_id = created_agent_node_id(&create_result);
    let marked_ready = app.update(cx, |app, _cx| {
        app.pending_agent_submissions.mark_ready(ready_node_id)
    });
    assert!(marked_ready, "Enter 失败测试必须先把既有 Agent 标记为就绪");

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
    let failed_enter = app.update(cx, |app, cx| {
        app.submit_pending_agent_task_for_ready_node(ready_node_id, cx)
    });
    let retry_typed = app.update(cx, |app, cx| {
        app.submit_pending_agent_task_for_ready_node(ready_node_id, cx)
    });
    let retry_submitted = app.update(cx, |app, cx| {
        app.submit_pending_agent_task_for_ready_node(ready_node_id, cx)
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
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Queued
        ),
        "Enter 成功前任务必须保持 Queued，实际：{failed_dispatch:?}"
    );
    assert!(!failed_enter, "第一次 Enter 写入失败时不得报告提交成功");
    assert!(retry_typed, "安全清行后必须自动重写同一任务正文");
    assert!(retry_submitted, "自动重写后必须再次发送 Enter 并提交成功");
    assert_eq!(
        actual_writes,
        vec![
            "第一次任务".as_bytes().to_vec(),
            vec![0x15],
            "第一次任务".as_bytes().to_vec(),
            vec![0x0d],
        ],
        "第一次正文后必须用 Ctrl+U(0x15) 清行；自动重试再写同一正文与 Enter"
    );
    assert!(
        !matches!(
            failed_dispatch,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.iter().any(|receipt| matches!(
                    receipt.status,
                    AgentTaskReceiptStatus::Submitted | AgentTaskReceiptStatus::Untracked
                ))
        ),
        "PTY 未提交成功时不得误报 Submitted 或 Untracked"
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
    let ready_node_id = created_agent_node_id(&create_result);
    let marked_ready = app.update(cx, |app, _cx| {
        app.pending_agent_submissions.mark_ready(ready_node_id)
    });
    assert!(marked_ready, "队列推进测试必须先把既有 Agent 标记为就绪");
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
    let first_submitted = app.update(cx, |app, cx| {
        app.submit_pending_agent_task_for_ready_node(ready_node_id, cx)
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
    let second_typed = app.update(cx, |app, cx| {
        app.submit_pending_agent_task_for_ready_node(node_id, cx)
    });
    let second_submitted = app.update(cx, |app, cx| {
        app.submit_pending_agent_task_for_ready_node(node_id, cx)
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
            &first_dispatch,
            Ok(CardCommandResult::AgentTasksDispatched { receipts })
                if receipts.len() == 1
                    && receipts[0].status == AgentTaskReceiptStatus::Queued
        ),
        "首项 managed 任务在 Enter 成功前应保持 Queued，实际结果：{first_dispatch:?}"
    );
    assert!(
        matches!(
            &second_dispatch,
            Ok(CardCommandResult::AgentTasksDispatched { receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Queued
        ),
        "忙碌 Agent 的第二项任务应先进入队列，实际结果：{second_dispatch:?}"
    );
    assert!(first_submitted, "首项正文写入后必须发送 Enter 完成提交");
    assert!(queue_advanced, "完成首项后应成功推进并提交下一项排队任务");
    assert!(second_typed, "队列推进后必须先写入第二项正文");
    assert!(second_submitted, "第二项正文稳定后必须发送 Enter 完成提交");
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
