use super::execute_agent_card_command;
use gpui::TestAppContext;
use norma::protocol::{AgentProfile, CardCommand, CardCommandResult};
use terminal::{PtyBackend, PtyError};

struct AcceptingPtyBackend;

impl PtyBackend for AcceptingPtyBackend {
    fn write(&mut self, _bytes: &[u8]) -> Result<(), PtyError> {
        Ok(())
    }

    fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), PtyError> {
        Ok(())
    }

    fn kill(&mut self) {}
}

fn created_agent(result: &Result<CardCommandResult, String>) -> (i64, u64) {
    match result {
        Ok(CardCommandResult::AgentCreated {
            definition_id,
            node_id: Some(node_id),
            ..
        }) => (*definition_id, *node_id),
        other => panic!("测试前置必须创建活跃 Agent，实际：{other:?}"),
    }
}

/// A-1：活跃 Agent 节点在 Workspace 已建立时，必须解析出完整身份快照。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent definition = Alan/Codex 1 条，激活卡片 1 张
///   workspace_id     = "workspace-test"
///   identity 字段    = 6 项
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 以 headless App + 临时 workspace 启动，进入画布。
///   2. 通过生产入口创建并激活 Alan 卡片。
///   3. 设置 Workspace ID，用 source = (definition_id, node_id) 解析身份。
///
/// 预期结果：
///   - 正断言：快照的 name 为 Alan、definition_id 与 node_id 与创建结果一致。
///   - 正断言：workspace_id 与 profile 与预期一致。
///   - 负断言：不得返回 None（活跃节点必须可识别）。
#[test]
fn agent_identity_for_source_resolves_active_node() {
    let factory: runtime::PtyFactory =
        Box::new(
            |_cmd, _cols, _rows, _tx| Ok(Box::new(AcceptingPtyBackend) as Box<dyn PtyBackend>),
        );
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("测试应创建临时 workspace");
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
    let (definition_id, node_id) = created_agent(&create_result);

    let snapshot = app.update(cx, |app, _cx| {
        app.workspace_id = Some("workspace-test".to_string());
        app.agent_identity_for_source(agent::AgentDispatchSource::agent(definition_id, node_id))
    });

    let snapshot = snapshot.expect("活跃 Agent 节点在已建立 Workspace 下必须解析出身份快照");
    assert_eq!(
        snapshot.name, "Alan",
        "身份快照必须使用存储中的规范名称，实际：{}",
        snapshot.name
    );
    assert_eq!(
        snapshot.definition_id, definition_id,
        "身份快照 definition_id 必须与来源一致"
    );
    assert_eq!(
        snapshot.node_id, node_id,
        "身份快照 node_id 必须与来源活跃节点一致"
    );
    assert_eq!(
        snapshot.workspace_id, "workspace-test",
        "身份快照必须携带 Workspace 作用域，实际：{}",
        snapshot.workspace_id
    );
    assert_eq!(
        snapshot.profile,
        AgentProfile::Codex,
        "身份快照 profile 必须来自持久化 definition"
    );
}

/// A-2：来源 node_id 不是该 definition 的活跃节点时，不得解析出身份快照。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent definition = Alan/Codex 1 条
///   source node      = 999（非任何活跃卡片）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建并激活 Alan 卡片，取得真实 definition_id。
///   2. 用 node_id=999 构造来源。
///   3. 尝试解析身份快照。
///
/// 预期结果：
///   - 负断言：返回 None，不得按名称猜测或回退到其他节点身份。
#[test]
fn agent_identity_for_source_rejects_inactive_node() {
    let factory: runtime::PtyFactory =
        Box::new(
            |_cmd, _cols, _rows, _tx| Ok(Box::new(AcceptingPtyBackend) as Box<dyn PtyBackend>),
        );
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("测试应创建临时 workspace");
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
    let (definition_id, _node_id) = created_agent(&create_result);

    let snapshot = app.update(cx, |app, _cx| {
        app.workspace_id = Some("workspace-test".to_string());
        app.agent_identity_for_source(agent::AgentDispatchSource::agent(definition_id, 999))
    });

    assert!(
        snapshot.is_none(),
        "非活跃 node_id 不得解析出身份快照，实际：{snapshot:?}"
    );
}
