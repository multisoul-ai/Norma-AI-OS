use super::{execute_agent_card_command, execute_high_level_agent_command};
use crate::motion::DispatchVisualOrigin;
use canvas::CanvasRect;
use gpui::TestAppContext;
use norma::protocol::{
    AgentProfile, AgentTaskAssignment, AgentTaskReceiptStatus, CardCommand, CardCommandResult,
};

/// DR-A2A-T-1：同 Workspace 的 Agent CLI 委派成功后，视觉回执必须从来源 Agent 指向目标 Agent。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent definitions = 2（浩宇/Codex + 老乔/Claude）
///   active source roots = 1（浩宇正在执行用户根任务）
///   delegated assignments = 1（浩宇 → 老乔）
///   expected business receipts = 1；expected visual effects = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless App，在同一 Canvas 创建并激活浩宇与老乔两张 Agent Card。
///   2. 为浩宇注册一个真实外部根任务，使 CLI 子委派具有可信 parent lineage。
///   3. 通过生产高层入口以 `AgentDispatchSource::agent(浩宇)` 向老乔派发任务。
///   4. 同时读取业务 receipt 与 MotionStore 的公开 presentation seam。
///
/// 预期结果：
///   - 正断言：业务层仍返回唯一 Queued receipt，目标是老乔节点。
///   - 正断言：视觉层只创建一个 effect，来源是浩宇节点、目标是老乔节点。
///   - 负断言：Agent 委派不得退化为 Command Dock 起点，也不得误命中来源卡片。
#[test]
fn test_agent_dispatch_receipt_spawns_visual_effect_from_source_agent_to_target_agent() {
    let factory: runtime::PtyFactory = Box::new(|_cmd, _cols, _rows, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)) as Box<dyn terminal::PtyBackend>)
    });
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new()
        .expect("Agent-to-Agent visual test should create a temp workspace");
    let (app, cx) = gpui
        .add_window_view(|_window, cx| crate::app::MultisoulApp::new_with_pty_factory(cx, factory));
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });

    let (source_definition_id, source_node_id, target_node_id) = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            let source = execute_agent_card_command(
                app,
                window,
                CardCommand::CreateAgent {
                    name: Some("浩宇".to_string()),
                    profile: AgentProfile::Codex,
                    activate: true,
                },
            )
            .expect("Agent-to-Agent visual test must create source 浩宇");
            let target = execute_agent_card_command(
                app,
                window,
                CardCommand::CreateAgent {
                    name: Some("老乔".to_string()),
                    profile: AgentProfile::Claude,
                    activate: true,
                },
            )
            .expect("Agent-to-Agent visual test must create target 老乔");
            let CardCommandResult::AgentCreated {
                definition_id: source_definition_id,
                node_id: Some(source_node_id),
                ..
            } = source
            else {
                panic!("来源浩宇必须返回 definition_id 与 active node_id");
            };
            let CardCommandResult::AgentCreated {
                node_id: Some(target_node_id),
                ..
            } = target
            else {
                panic!("目标老乔必须返回 active node_id");
            };
            assert!(
                app.register_external_agent_root(
                    source_node_id,
                    Some("用户交给浩宇的根任务".to_string()),
                ),
                "浩宇必须先注册可信运行根任务，才能发起 Agent CLI 子委派"
            );
            (source_definition_id, source_node_id, target_node_id)
        })
    });

    let dispatch_result = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            let area = app.visible_canvas_rect(window);
            let zoom = app.runtime.document.viewport.zoom;
            execute_high_level_agent_command(
                app,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "老乔".to_string(),
                        task: "检查当前流星效果实现".to_string(),
                    }],
                    force_untracked: false,
                },
                area,
                zoom,
                agent::AgentDispatchSource::agent(source_definition_id, source_node_id),
                None,
            )
        })
    });
    let presentations = app.update(cx, |app, _cx| app.motion.dispatch_receipt_presentations());
    let receipt = match &dispatch_result {
        Ok(CardCommandResult::AgentTasksDispatched { receipts }) if receipts.len() == 1 => {
            &receipts[0]
        }
        other => panic!("浩宇向老乔委派必须返回唯一业务 receipt，实际：{other:?}"),
    };

    assert_eq!(
        receipt.status,
        AgentTaskReceiptStatus::Queued,
        "老乔尚未 ready 时业务层必须继续返回真实 Queued receipt"
    );
    assert_eq!(
        receipt.node_id, target_node_id,
        "业务 receipt 必须精确命中老乔节点"
    );
    assert_eq!(
        presentations.len(),
        1,
        "唯一可接受的 Agent 子委派必须创建且只创建一个视觉 effect"
    );
    assert_eq!(
        presentations[0].origin,
        DispatchVisualOrigin::AgentNode(source_node_id),
        "Agent-to-Agent 流星必须从真实来源浩宇节点发射"
    );
    assert_eq!(
        presentations[0].target_node_id, target_node_id,
        "Agent-to-Agent 流星必须命中真实目标老乔节点"
    );
    assert_ne!(
        presentations[0].origin,
        DispatchVisualOrigin::CommandDock,
        "Agent CLI 委派不得伪装成 Command Dock 发射"
    );
    assert_ne!(
        presentations[0].target_node_id, source_node_id,
        "Agent-to-Agent 流星不得误命中来源浩宇卡片"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}

/// DR-A2A-T-2：来源 Agent Card 离开可见 Canvas 时只跳过视觉副作用，业务指派仍须成功。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent definitions = 2（浩宇/Codex + 老乔/Claude）
///   source bounds = (-10_000, -10_000, 240, 160)，中心 = (-9_880, -9_920)
///   delegated assignments = 1；expected business receipts = 1
///   expected visual effects = 0；expected unavailable skips = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建并激活浩宇、老乔，为浩宇注册可信根任务。
///   2. 保留业务节点身份，但把浩宇 presentation rect 移到 viewport 外。
///   3. 继续通过生产高层入口从浩宇向可见的老乔派发任务。
///   4. 分别读取业务 receipt、视觉 presentation 与 unavailable 诊断。
///
/// 预期结果：
///   - 正断言：业务层仍返回唯一 Queued receipt，目标仍是老乔。
///   - 正断言：视觉层记录一次来源不可见跳过。
///   - 负断言：不得生成伪造的 Dock→老乔或离屏→老乔流星。
#[test]
fn test_offscreen_source_skips_agent_visual_effect_without_changing_dispatch_receipt() {
    let factory: runtime::PtyFactory = Box::new(|_cmd, _cols, _rows, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)) as Box<dyn terminal::PtyBackend>)
    });
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::TempDir::new()
        .expect("Offscreen source visual test should create a temp workspace");
    let (app, cx) = gpui
        .add_window_view(|_window, cx| crate::app::MultisoulApp::new_with_pty_factory(cx, factory));
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });

    let (source_definition_id, source_node_id, target_node_id) = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            let source = execute_agent_card_command(
                app,
                window,
                CardCommand::CreateAgent {
                    name: Some("浩宇".to_string()),
                    profile: AgentProfile::Codex,
                    activate: true,
                },
            )
            .expect("Offscreen source visual test must create source 浩宇");
            let target = execute_agent_card_command(
                app,
                window,
                CardCommand::CreateAgent {
                    name: Some("老乔".to_string()),
                    profile: AgentProfile::Claude,
                    activate: true,
                },
            )
            .expect("Offscreen source visual test must create target 老乔");
            let CardCommandResult::AgentCreated {
                definition_id: source_definition_id,
                node_id: Some(source_node_id),
                ..
            } = source
            else {
                panic!("离屏来源浩宇必须返回 definition_id 与 active node_id");
            };
            let CardCommandResult::AgentCreated {
                node_id: Some(target_node_id),
                ..
            } = target
            else {
                panic!("可见目标老乔必须返回 active node_id");
            };
            assert!(
                app.register_external_agent_root(
                    source_node_id,
                    Some("用户交给浩宇的根任务".to_string()),
                ),
                "离屏前浩宇必须注册可信运行根任务，保证业务委派仍合法"
            );
            let offscreen = CanvasRect::new(-10_000.0, -10_000.0, 240.0, 160.0);
            app.runtime
                .document
                .set_node_bounds(source_node_id, offscreen);
            app.motion.sync_drag_rect(source_node_id, offscreen);
            (source_definition_id, source_node_id, target_node_id)
        })
    });

    let dispatch_result = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            let area = app.visible_canvas_rect(window);
            let zoom = app.runtime.document.viewport.zoom;
            execute_high_level_agent_command(
                app,
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment {
                        name: "老乔".to_string(),
                        task: "离屏来源也要保持业务指派".to_string(),
                    }],
                    force_untracked: false,
                },
                area,
                zoom,
                agent::AgentDispatchSource::agent(source_definition_id, source_node_id),
                None,
            )
        })
    });
    let (presentations, diagnostics) = app.update(cx, |app, _cx| {
        (
            app.motion.dispatch_receipt_presentations(),
            app.motion.diagnostics(),
        )
    });
    let receipt = match &dispatch_result {
        Ok(CardCommandResult::AgentTasksDispatched { receipts }) if receipts.len() == 1 => {
            &receipts[0]
        }
        other => panic!("离屏来源的合法委派仍必须返回唯一业务 receipt，实际：{other:?}"),
    };

    assert_eq!(
        receipt.status,
        AgentTaskReceiptStatus::Queued,
        "来源离屏不能改变老乔尚未 ready 时的真实 Queued 业务状态"
    );
    assert_eq!(
        receipt.node_id, target_node_id,
        "来源离屏不能改变业务 receipt 的真实目标老乔"
    );
    assert!(
        presentations.is_empty(),
        "来源浩宇离屏时不得生成伪造的 Dock 或离屏飞行 effect"
    );
    assert_eq!(
        diagnostics.dispatch_unavailable_skips, 1,
        "来源中心离屏应精确记录一次视觉 unavailable skip"
    );
    assert_ne!(
        receipt.node_id, source_node_id,
        "视觉被跳过时业务 receipt 也不得误写回来源浩宇"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}
