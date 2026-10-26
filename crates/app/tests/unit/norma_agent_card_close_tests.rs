use super::{
    execute_agent_card_command, execute_norma_adapter_command,
    execute_norma_adapter_command_with_source,
};
use agent::AdapterError;
use gpui::TestAppContext;
use norma::protocol::{AgentProfile as NormaAgentProfile, CardCommand, CardCommandResult};
use terminal::{PtyBackend, TestPtyBackend};

fn echo_factory() -> runtime::PtyFactory {
    Box::new(|_cmd, _cols, _rows, tx| Ok(Box::new(TestPtyBackend::echo(tx)) as Box<dyn PtyBackend>))
}

/// Norma Adapter：按 Agent 语音别名关闭空闲 AgentCard，但保留显式 Agent 定义。
///
/// 数据构造（含关键数值的推导过程）：
///   显式 Agent 定义数      = 1（Chloe）
///   Chloe 活跃卡片数       = 1
///   关闭命令数             = 1
///   关闭后活跃卡片数        = 1 - 1 = 0
///   关闭后显式定义数         = 1（关闭卡片不删除身份）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 通过生产 Agent command 创建并激活显式 Chloe/Codex。
///   2. 记录 Chloe 的确定 node_id。
///   3. 通过 Norma Adapter 执行 `DeleteAgentCard { agent: "克洛伊" }`。
///   4. 查询 Runtime document 与 AgentDefinitionsState。
///
/// 预期结果：
///   - 正断言：语音别名关闭成功并返回 Chloe 的实际 node_id。
///   - 正断言：Chloe 显式定义仍存在，之后可以重新激活。
///   - 负断言：Chloe 的旧 AgentCard 不再留在画布或 live registry。
#[test]
fn test_norma_adapter_closes_agent_card_by_speech_alias_and_preserves_definition() {
    let mut gpui = TestAppContext::single();
    let workspace =
        tempfile::TempDir::new().expect("named Agent close temp workspace should be creatable");
    let (app, cx) = gpui.add_window_view(|_window, cx| {
        crate::app::MultisoulApp::new_with_pty_factory(cx, echo_factory())
    });
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    super::prepare_shared_context(&app, cx);

    let created = cx
        .update(|window, app_cx| {
            app.update(app_cx, |app, _cx| {
                execute_agent_card_command(
                    app,
                    window,
                    CardCommand::CreateAgent {
                        name: Some("Chloe".to_string()),
                        profile: NormaAgentProfile::Codex,
                        activate: true,
                    },
                )
            })
        })
        .expect("explicit Chloe Agent definition and card should be created");
    let CardCommandResult::AgentCreated {
        node_id: Some(chloe_node_id),
        ..
    } = created
    else {
        panic!("creating active Chloe should return AgentCreated with node_id");
    };

    let closed = cx
        .update(|window, app_cx| {
            app.update(app_cx, |app, cx| {
                execute_norma_adapter_command(
                    app,
                    window,
                    cx,
                    agent::AdapterCommand::DeleteAgentCard {
                        agent: "克洛伊".to_string(),
                    },
                )
            })
        })
        .expect("Chloe's speech alias should close the idle card through the production Adapter");
    let (card_exists, live_count, definition_exists) = app.read_with(cx, |app, _cx| {
        (
            app.runtime.document.node(chloe_node_id).is_some(),
            app.runtime.live_len(),
            app.agent_definitions
                .definitions()
                .iter()
                .any(|definition| definition.name == "Chloe"),
        )
    });

    assert_eq!(
        closed.status, "succeeded",
        "closing idle Chloe should report a completed Adapter side effect"
    );
    assert_eq!(
        closed.node_id,
        Some(chloe_node_id),
        "speech-alias close must return Chloe's actual active node id"
    );
    assert!(
        definition_exists,
        "closing Chloe's card must preserve the explicit Chloe Agent definition"
    );
    assert!(
        !card_exists,
        "closing Chloe must remove its old AgentCard from the Canvas document"
    );
    assert!(
        live_count == 0,
        "closing the only live Chloe card must leave zero live runtimes; actual: {live_count}"
    );
}

/// Norma Adapter：忙碌 Chloe 必须连续两次明确关闭，第一次不能中断或删除。
///
/// 数据构造（含关键数值的推导过程）：
///   Chloe AgentCard       = 1 张
///   派发任务              = 1 个 → coordinator busy=true
///   关闭请求              = 2 次
///   第一次后卡片数         = 1（仅进入确认）
///   第二次后卡片数         = 1 - 1 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建并激活显式 Chloe/Codex。
///   2. 通过生产 Adapter 向 Chloe 派发一个任务，使其进入忙碌或排队状态。
///   3. 第一次执行 `DeleteAgentCard("Chloe")`，读取拒绝回执与卡片状态。
///   4. 第二次执行相同关闭命令，读取成功回执、卡片和定义状态。
///
/// 预期结果：
///   - 负断言：第一次关闭被拒绝，Chloe 卡片仍存在且定义未删除。
///   - 正断言：第二次关闭成功并返回 Chloe 的 node_id。
///   - 负断言：第二次后旧卡片不存在，但显式 Chloe 定义继续存在。
#[test]
fn test_norma_adapter_requires_second_close_for_busy_named_agent() {
    let mut gpui = TestAppContext::single();
    let workspace =
        tempfile::TempDir::new().expect("busy Agent close temp workspace should be creatable");
    let (app, cx) = gpui.add_window_view(|_window, cx| {
        crate::app::MultisoulApp::new_with_pty_factory(cx, echo_factory())
    });
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    super::prepare_shared_context(&app, cx);

    let created = cx
        .update(|window, app_cx| {
            app.update(app_cx, |app, _cx| {
                execute_agent_card_command(
                    app,
                    window,
                    CardCommand::CreateAgent {
                        name: Some("Chloe".to_string()),
                        profile: NormaAgentProfile::Codex,
                        activate: true,
                    },
                )
            })
        })
        .expect("busy-close fixture should create active Chloe");
    let CardCommandResult::AgentCreated {
        node_id: Some(chloe_node_id),
        ..
    } = created
    else {
        panic!("creating active Chloe should return AgentCreated with node_id");
    };
    let assigned = cx
        .update(|window, app_cx| {
            app.update(app_cx, |app, cx| {
                execute_norma_adapter_command(
                    app,
                    window,
                    cx,
                    agent::AdapterCommand::AssignAgentTask {
                        agent: "Chloe".to_string(),
                        task: "检查当前项目。".to_string(),
                    },
                )
            })
        })
        .expect("Chloe task should be accepted before testing busy close");
    assert_eq!(
        assigned.status, "accepted",
        "the fixture task must reach Chloe and make the coordinator busy"
    );

    let first_close = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            execute_norma_adapter_command(
                app,
                window,
                cx,
                agent::AdapterCommand::DeleteAgentCard {
                    agent: "Chloe".to_string(),
                },
            )
        })
    });
    let (exists_after_first, definition_after_first) = app.read_with(cx, |app, _cx| {
        (
            app.runtime.document.node(chloe_node_id).is_some(),
            app.agent_definitions
                .definitions()
                .iter()
                .any(|definition| definition.name == "Chloe"),
        )
    });
    let second_close = cx
        .update(|window, app_cx| {
            app.update(app_cx, |app, cx| {
                execute_norma_adapter_command(
                    app,
                    window,
                    cx,
                    agent::AdapterCommand::DeleteAgentCard {
                        agent: "Chloe".to_string(),
                    },
                )
            })
        })
        .expect("second explicit close should interrupt tasks and remove Chloe's card");
    let (exists_after_second, definition_after_second) = app.read_with(cx, |app, _cx| {
        (
            app.runtime.document.node(chloe_node_id).is_some(),
            app.agent_definitions
                .definitions()
                .iter()
                .any(|definition| definition.name == "Chloe"),
        )
    });

    assert!(
        matches!(first_close, Err(AdapterError::Rejected(_))),
        "first busy close must request confirmation without deleting; actual: {first_close:?}"
    );
    assert!(
        exists_after_first,
        "Chloe's AgentCard must remain after the first busy close request"
    );
    assert!(
        definition_after_first,
        "Chloe's explicit definition must remain during close confirmation"
    );
    assert_eq!(
        second_close.status, "succeeded",
        "second explicit busy close should complete the delete side effect"
    );
    assert_eq!(
        second_close.node_id,
        Some(chloe_node_id),
        "second close must report Chloe's actual deleted node id"
    );
    assert!(
        !exists_after_second,
        "Chloe's old AgentCard must be absent after the confirmed second close"
    );
    assert!(
        definition_after_second,
        "confirmed card close must not delete Chloe's explicit Agent definition"
    );
}

/// 豆包关闭忙碌 AgentCard：用户语音删除应一次完成，显式 Agent 定义仍保留。
///
/// 数据构造（含关键数值的推导过程）：Chloe 卡片 1 张、任务 1 个；
/// 豆包关闭请求 1 次，预期卡片数 = 1 - 1 = 0，定义数保持 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建并激活 Chloe/Codex，派发任务使 coordinator 标记忙碌。
///   2. 用 DoubaoVoice 来源执行一次 DeleteAgentCard。
///   3. 检查回执、卡片和定义状态。
///
/// 预期结果：一次删除成功；旧卡片不存在；定义仍存在。
#[test]
fn test_doubao_voice_closes_busy_agent_card_without_second_confirmation() {
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("应创建豆包忙碌删除测试项目");
    let (app, cx) = gpui.add_window_view(|_window, cx| {
        crate::app::MultisoulApp::new_with_pty_factory(cx, echo_factory())
    });
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    super::prepare_shared_context(&app, cx);
    let created = cx
        .update(|window, app_cx| {
            app.update(app_cx, |app, _| {
                execute_agent_card_command(
                    app,
                    window,
                    CardCommand::CreateAgent {
                        name: Some("Chloe".into()),
                        profile: NormaAgentProfile::Codex,
                        activate: true,
                    },
                )
            })
        })
        .expect("应创建 Chloe 卡片");
    let CardCommandResult::AgentCreated {
        node_id: Some(node_id),
        ..
    } = created
    else {
        panic!("创建 Chloe 后应返回真实 node_id");
    };
    let assigned = cx
        .update(|window, app_cx| {
            app.update(app_cx, |app, cx| {
                execute_norma_adapter_command(
                    app,
                    window,
                    cx,
                    agent::AdapterCommand::AssignAgentTask {
                        agent: "Chloe".into(),
                        task: "检查当前项目".into(),
                    },
                )
            })
        })
        .expect("任务应成功派发");
    assert_eq!(assigned.status, "accepted", "任务应使 Chloe 进入忙碌状态");

    let closed = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            execute_norma_adapter_command_with_source(
                app,
                Some(window),
                cx,
                agent::AdapterCommand::DeleteAgentCard {
                    agent: "Chloe".into(),
                },
                agent::AgentInputSource::DoubaoVoice,
            )
        })
    });
    let (card_exists, definition_exists) = app.read_with(cx, |app, _| {
        (
            app.runtime.document.node(node_id).is_some(),
            app.agent_definitions
                .definitions()
                .iter()
                .any(|definition| definition.name == "Chloe"),
        )
    });
    assert!(
        closed.is_ok(),
        "豆包一次关闭忙碌卡片应成功；实际：{closed:?}"
    );
    assert!(!card_exists, "一次关闭后 Chloe 旧卡片不应存在");
    assert!(definition_exists, "关闭卡片不应删除 Chloe 定义");
}
