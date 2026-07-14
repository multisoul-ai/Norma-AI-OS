use super::execute_agent_card_command;
use gpui::TestAppContext;
use norma::protocol::{AgentProfile, AgentTaskAssignment, CardCommand, CardCommandResult};
use std::sync::{Arc, Mutex};
use terminal::{PtyBackend, PtyError};

struct RecordingPtyBackend {
    writes: Arc<Mutex<Vec<Vec<u8>>>>,
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
