use super::execute_agent_card_command;
use crate::motion::DispatchVisualOrigin;
use gpui::TestAppContext;
use norma::protocol::{AgentProfile, CardCommand, CardCommandResult};

/// DR-T-5：GPT-Live 用 reply_agent 给已有卡片发消息时，与豆包一样必须放出派发流星。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent cards = 1（Alan/Codex，位于 Canvas 可见区）
///   reply_agent 调用 = 1（来源 GptLiveVoice）
///   expected visual effects = 1（目标即 Alan 卡片）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless App，进入 Canvas 并创建 Alan Agent Card。
///   2. 以 `AgentInputSource::GptLiveVoice` 经生产入口执行 `ReplyAgent`。
///   3. 读取 MotionStore 的 dispatch presentations。
///
/// 预期结果：
///   - 正断言：回复回执指向 Alan，视觉层恰好一个从 Command Dock 起飞、命中 Alan 的 effect。
///   - 负断言：GPT-Live 来源不得被输入源过滤器漏掉（此前为 0 个 effect）。
#[test]
fn test_gpt_live_reply_agent_spawns_dispatch_visual_effect() {
    let factory: runtime::PtyFactory = Box::new(|_cmd, _cols, _rows, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)) as Box<dyn terminal::PtyBackend>)
    });
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new().expect("临时工作区");
    let (app, cx) = gpui
        .add_window_view(|_window, cx| crate::app::MultisoulApp::new_with_pty_factory(cx, factory));
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    super::prepare_shared_context(&app, cx);
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
    let Ok(CardCommandResult::AgentCreated {
        node_id: Some(alan),
        ..
    }) = created
    else {
        panic!("测试前置必须创建 Alan 卡片：{created:?}");
    };
    let reply = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.window_active = true;
            crate::app::agent_runtime::execute_norma_adapter_command_with_source(
                app,
                Some(window),
                cx,
                agent::AdapterCommand::ReplyAgent {
                    agent: "Alan".into(),
                    message: "继续方案".into(),
                },
                agent::AgentInputSource::GptLiveVoice,
            )
        })
    });
    let presentations = app.update(cx, |app, _cx| app.motion.dispatch_receipt_presentations());

    assert_eq!(
        reply.as_ref().ok().and_then(|reply| reply.node_id),
        Some(alan),
        "回复回执必须指向 Alan：{reply:?}"
    );
    assert_eq!(
        presentations.len(),
        1,
        "GPT-Live 回复必须放出且只放出一个流星"
    );
    assert_eq!(presentations[0].origin, DispatchVisualOrigin::CommandDock);
    assert_eq!(presentations[0].target_node_id, alan, "流星必须命中 Alan");
}
