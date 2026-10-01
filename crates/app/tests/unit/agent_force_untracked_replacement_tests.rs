use super::{execute_agent_card_command, execute_norma_adapter_command_with_source};
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

/// 豆包向忙碌的无 Hook AgentCard 派新任务：直接替换旧任务，无额外授权。
///
/// 数据构造（含关键数值的推导过程）：Alan 卡片 1 张；第一次任务 1 个；
/// 第二次豆包派发 1 个，预期活跃任务仍只有 1 个，旧任务转 interrupted。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 关闭 Hook，创建并激活 Alan/Codex，使其处于 Untracked。
///   2. 通过普通来源派发 first，再通过 DoubaoVoice 派发 second。
///   3. 读取两个真实任务回执和 coordinator 状态。
///
/// 预期结果：第二次无需授权即可接收；first 被中断且 second 存在。
#[test]
fn test_doubao_replaces_busy_untracked_agent_without_authorization() {
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("应创建豆包派工测试项目");
    let (app, cx) = gpui.add_window_view(|_window, cx| {
        crate::app::MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_cmd, _cols, _rows, tx| {
                Ok(Box::new(terminal::TestPtyBackend::echo(tx)) as Box<dyn PtyBackend>)
            }),
        )
    });
    app.update(cx, |app, cx| {
        app.disable_agent_hook_for_test();
        app.enter_canvas_for_test(workspace.path(), cx);
    });
    cx.update(|window, app_cx| {
        app.update(app_cx, |app, _| {
            execute_agent_card_command(
                app,
                window,
                CardCommand::CreateAgent {
                    name: Some("Alan".into()),
                    profile: AgentProfile::Codex,
                    activate: true,
                },
            )
        })
    })
    .expect("应创建 Alan 卡片");
    let dispatch =
        |task: &str, source: agent::AgentInputSource, cx: &mut gpui::VisualTestContext| {
            cx.update(|window, app_cx| {
                app.update(app_cx, |app, cx| {
                    execute_norma_adapter_command_with_source(
                        app,
                        Some(window),
                        cx,
                        agent::AdapterCommand::AssignAgentTask {
                            agent: "Alan".into(),
                            task: task.into(),
                        },
                        source,
                    )
                })
            })
        };
    let first =
        dispatch("first", agent::AgentInputSource::Other, cx).expect("首次任务应被 Alan 接收");
    let second = dispatch("second", agent::AgentInputSource::DoubaoVoice, cx);
    let tasks = app.read_with(cx, |app, _| {
        app.agent_task_coordinator
            .as_ref()
            .expect("应存在任务协调器")
            .tasks()
    });
    assert_eq!(first.status, "accepted", "首个任务应被接收");
    assert!(
        second.is_ok(),
        "豆包第二次派工不应要求额外授权；实际：{second:?}"
    );
    let second = second.expect("成功已由上一断言确认");
    assert_eq!(second.status, "accepted", "第二个任务应被接收");
    assert!(
        tasks.iter().any(
            |task| task.task_id == first.task_id.expect("首项应有 task_id")
                && task.state == "interrupted"
        ),
        "被替换的 first 必须转为 interrupted"
    );
    assert!(
        tasks.iter().any(
            |task| task.task_id == second.task_id.expect("次项应有 task_id")
                && matches!(task.state, "queued" | "running")
        ),
        "第二项必须成为新的活跃任务"
    );
}
