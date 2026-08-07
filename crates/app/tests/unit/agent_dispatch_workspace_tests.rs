use super::execute_agent_card_command;
use gpui::TestAppContext;
use norma::protocol::{
    AgentProfile, AgentTaskAssignment, AgentTaskReceiptStatus, CardCommand, CardCommandResult,
};
use terminal::{PtyBackend, PtyError};

struct AcceptingPtyBackend;

fn created_agent_node_id(result: &Result<CardCommandResult, String>) -> u64 {
    match result {
        Ok(CardCommandResult::AgentCreated {
            node_id: Some(node_id),
            ..
        }) => *node_id,
        other => panic!("测试前置必须创建活跃 Agent，实际：{other:?}"),
    }
}

impl PtyBackend for AcceptingPtyBackend {
    fn write(&mut self, _bytes: &[u8]) -> Result<(), PtyError> {
        Ok(())
    }

    fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), PtyError> {
        Ok(())
    }

    fn kill(&mut self) {}
}

/// ART-8：切换 workspace 后，新项目同 id Agent 不得继承旧项目的 busy 与等待任务。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace A definitions = 1 条 Alan，因此 definition_id = 1
///   workspace A tasks       = 2 项：第一项运行中、第二项 "旧项目机密" 排队
///   workspace B definitions = 1 条 Bella，新项目自增 id 重新从 1 开始
///   workspace B tasks       = 1 项 "检查新项目"，应返回 Submitted 而不是 Queued
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 workspace A 创建并标记 Alan Ready，提交第一项并排队包含旧项目文本的第二项；
///   2. 通过生产 workspace 入口切换到 workspace B；
///   3. 在 workspace B 创建同为 definition_id=1 的 Bella，标记 Ready 后提交新项目任务；
///   4. 读取结构化回执，确认新项目没有继承旧 busy/queue。
///
/// 预期结果：
///   - 正断言：Bella 的新任务立即 Submitted；
///   - 负断言：新项目不得因旧项目任务而返回 Queued。
#[test]
fn test_workspace_switch_discards_previous_project_agent_queue() {
    let factory: runtime::PtyFactory = Box::new(move |_cmd, _cols, _rows, _tx| {
        Ok(Box::new(AcceptingPtyBackend) as Box<dyn PtyBackend>)
    });
    let mut gpui = TestAppContext::single();
    let root = tempfile::TempDir::new().expect("应创建隔离的双 workspace 根目录");
    let workspace_a = root.path().join("workspace-a");
    let workspace_b = root.path().join("workspace-b");
    std::fs::create_dir(&workspace_a).expect("应创建 workspace A");
    std::fs::create_dir(&workspace_b).expect("应创建 workspace B");
    let (app, cx) = gpui
        .add_window_view(|_window, cx| crate::app::MultisoulApp::new_with_pty_factory(cx, factory));
    app.update(cx, |app, cx| app.enter_canvas_for_test(&workspace_a, cx));

    let create_a = cx.update(|window, app_cx| {
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
    let ready_a = created_agent_node_id(&create_a);
    let marked_a_ready = app.update(cx, |app, _cx| {
        app.pending_agent_submissions.mark_ready(ready_a)
    });
    assert!(marked_a_ready, "workspace A 的 Alan 必须先标记为 CLI Ready");
    let first_a = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            execute_agent_card_command(
                app,
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Alan".to_string(),
                        task: "运行旧项目任务".to_string(),
                    }],
                    force_untracked: false,
                },
            )
        })
    });
    let queued_a = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            execute_agent_card_command(
                app,
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Alan".to_string(),
                        task: "旧项目机密".to_string(),
                    }],
                    force_untracked: false,
                },
            )
        })
    });
    cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.open_workspace_path(workspace_b.clone(), window, cx)
        });
    });

    let create_b = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            execute_agent_card_command(
                app,
                window,
                CardCommand::CreateAgent {
                    name: Some("Bella".to_string()),
                    profile: AgentProfile::Codex,
                    activate: true,
                },
            )
        })
    });
    let ready_b = created_agent_node_id(&create_b);
    let marked_b_ready = app.update(cx, |app, _cx| {
        app.pending_agent_submissions.mark_ready(ready_b)
    });
    assert!(
        marked_b_ready,
        "workspace B 的 Bella 必须先标记为 CLI Ready"
    );
    let dispatch_b = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            execute_agent_card_command(
                app,
                window,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "Bella".to_string(),
                        task: "检查新项目".to_string(),
                    }],
                    force_untracked: false,
                },
            )
        })
    });

    assert!(
        matches!(
            create_a,
            Ok(CardCommandResult::AgentCreated {
                definition_id: 1,
                ..
            })
        ),
        "workspace A 应创建 definition_id=1 的 Alan，实际：{create_a:?}"
    );
    assert!(
        matches!(first_a, Ok(CardCommandResult::AgentTasksDispatched { .. })),
        "workspace A 第一项任务应成功进入运行态，实际：{first_a:?}"
    );
    assert!(
        matches!(
            queued_a,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Queued
        ),
        "workspace A 第二项任务应确实进入等待队列，实际：{queued_a:?}"
    );
    assert!(
        matches!(
            create_b,
            Ok(CardCommandResult::AgentCreated {
                definition_id: 1,
                ..
            })
        ),
        "workspace B 应独立创建同为 definition_id=1 的 Bella，实际：{create_b:?}"
    );
    assert!(
        matches!(
            dispatch_b,
            Ok(CardCommandResult::AgentTasksDispatched { ref receipts })
                if receipts.len() == 1 && receipts[0].status == AgentTaskReceiptStatus::Queued
        ),
        "workspace B 的 Bella 不得继承 Alan 的旧队列；新任务仅因等待 Enter 而 Queued，实际：{dispatch_b:?}"
    );
    assert!(
        app.update(cx, |app, _cx| app
            .agent_task_coordinator
            .as_ref()
            .is_some_and(|coordinator| coordinator.queued_task_count(1) == 0)),
        "workspace A 的排队文本不得进入 workspace B 同 id Agent 的协调队列"
    );
}
