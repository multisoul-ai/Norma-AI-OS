use crate::{echo_factory, enter_canvas, make_app, read_runtime};
use agent::AdapterCommand;
use canvas::CanvasNodeKind;
use gpui::TestAppContext;
use norma::protocol::{AgentProfile, CardCommand, CardCommandResult, PreviewType};
use std::path::PathBuf;

/// E2E-NORMA-1：Norma 的生产 Adapter 用一次命令创建 AgentCard 并派发任务
///
/// 数据构造（含关键数值的推导过程）：
///   Fresh project definitions = 0
///   `create_agent(codex, task)` calls = 1
///   Additional `assign_agent_task` calls = 0
///   Expected Canvas AgentCards = 0 + 1 = 1
///   Expected FIFO tasks = 0 + 1 = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless 真实 MultisoulApp 并进入临时项目 Canvas
///   2. 经 production `AgentCardAdapter` 执行一次带 task 的 `create_agent(codex)`
///   3. 从公开 E2E 投影读取卡片、系统生成名称、FIFO 数量与启动等待状态
///
/// 预期结果：
///   - 正断言：同一回执为 accepted，并返回 node_id 与 task_id=1
///   - 正断言：画布只有一张 AgentCard，FIFO 只有一个等待 CLI Ready 的任务
///   - 负断言：系统生成的 Agent 不能名为 Norma，且不能重复创建任务或卡片
#[test]
fn norma_master_combined_create_and_task_reaches_real_app_fifo() {
    let mut gpui = TestAppContext::single();
    let (app, cx) = make_app(&mut gpui, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Norma e2e workspace should be creatable");
    enter_canvas(&app, cx, workspace.path());

    let created = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.execute_norma_adapter_command_for_e2e(
                window,
                cx,
                AdapterCommand::CreateAgent {
                    profile: AgentProfile::Codex,
                    task: Some("检查当前项目有哪些技术栈。".to_string()),
                },
            )
        })
    });
    let created = created.expect("production Norma create_agent should succeed");
    let definition = app.read_with(cx, |app, _cx| {
        app.agent_settings_e2e_snapshot().definitions[0].clone()
    });
    let node_id = created
        .node_id
        .expect("combined create_agent receipt should expose the created AgentCard node id");
    let agent_cards = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .count()
    });
    let task_count = app.update(cx, |app, _cx| app.agent_task_count_for_e2e());
    let waiting_for_ready =
        app.update(cx, |app, _cx| app.agent_submission_waiting_for_e2e(node_id));

    assert_eq!(
        created.status, "accepted",
        "combined create_agent should report FIFO acceptance without claiming task completion"
    );
    assert_eq!(
        created.task_id,
        Some(1),
        "the combined command's first real AgentCard FIFO task should expose task_id=1"
    );
    assert_eq!(
        agent_cards, 1,
        "combined creation and dispatch should produce exactly one AgentCard"
    );
    assert_eq!(
        task_count, 1,
        "one combined command should enqueue exactly one FIFO task without a duplicate assignment"
    );
    assert!(
        waiting_for_ready,
        "the immediately queued task should retain the existing CLI readiness safety gate"
    );
    assert!(
        !definition.name.eq_ignore_ascii_case("Norma"),
        "the Master identity must not be allocated as a Canvas Agent name"
    );
}

/// E2E-NORMA-2：Norma 应能通过生产 Adapter 自主创建 Browser 与 Preview 卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   Initial Canvas nodes = 0
///   Norma create calls = 2（Browser + Preview）
///   Expected Canvas nodes = 0 + 2 = 2
///   Expected live PTY nodes = 0（Browser/Preview 都是 canvas-only）
///   Browser URL = None → 默认 `https://www.google.com`
///   Preview file = workspace/review.md，正文 8 + 2 + 11 = 21 bytes（`# Review\n\nLooks good.`）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless 真实 MultisoulApp，进入含 review.md 的临时项目。
///   2. 经公开 e2e seam 执行 `CreateBrowser { url: None }`。
///   3. 经相同 seam 执行 `CreatePreview`，再推进异步 Preview 读取任务。
///   4. 从公开 Runtime 投影读取节点、选择集与 live registry。
///
/// 预期结果：
///   - 正断言：两次真实创建均成功并分别返回 node_id=1、node_id=2。
///   - 正断言：Browser 使用默认首页，Preview 指向 review.md 且采用 Markdown。
///   - 负断言：不得创建 Terminal/Agent，也不得注册 PTY；旧 Browser 不得继续被选中。
#[test]
fn norma_master_creates_browser_and_preview_cards_through_production_adapter() {
    let mut gpui = TestAppContext::single();
    let (app, cx) = make_app(&mut gpui, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Norma card e2e workspace should be creatable");
    std::fs::write(
        workspace.path().join("review.md"),
        "# Review\n\nLooks good.",
    )
    .expect("Preview fixture should be writable");
    enter_canvas(&app, cx, workspace.path());

    let browser = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.execute_norma_adapter_command_for_e2e(
                window,
                cx,
                AdapterCommand::CreateBrowser { url: None },
            )
        })
    });
    let preview = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.execute_norma_adapter_command_for_e2e(
                window,
                cx,
                AdapterCommand::CreatePreview {
                    path: PathBuf::from("review.md"),
                    explicit_type: Some(PreviewType::Markdown),
                },
            )
        })
    });
    cx.run_until_parked();

    let browser = browser.expect("production create_browser should succeed");
    let preview = preview.expect("production create_preview should succeed");
    assert_eq!(
        browser.node_id,
        Some(1),
        "the first Norma Browser card should return node_id=1"
    );
    assert_eq!(
        preview.node_id,
        Some(2),
        "the second Norma Preview card should return node_id=2"
    );
    let (nodes, selected, live_len) = read_runtime(&app, cx, |runtime| {
        (
            runtime.document.nodes().to_vec(),
            runtime.selection.iter().collect::<Vec<_>>(),
            runtime.live_len(),
        )
    });
    assert_eq!(
        nodes.len(),
        2,
        "Norma should create exactly two Canvas nodes"
    );
    let CanvasNodeKind::Browser(browser_spec) = &nodes[0].kind else {
        panic!("node 1 should be Browser, actual kind: {:?}", nodes[0].kind);
    };
    assert_eq!(
        browser_spec.url, "https://www.google.com",
        "Browser without URL should open the product default page"
    );
    let CanvasNodeKind::Preview(preview_spec) = &nodes[1].kind else {
        panic!("node 2 should be Preview, actual kind: {:?}", nodes[1].kind);
    };
    assert_eq!(
        PathBuf::from(&preview_spec.path),
        workspace.path().join("review.md"),
        "Preview should resolve the requested relative file against the current workspace"
    );
    assert_eq!(
        preview_spec.explicit_type,
        Some(canvas::PreviewType::Markdown),
        "Preview should use the requested Markdown renderer"
    );
    assert_eq!(
        selected,
        vec![2],
        "the newly created Preview should replace Browser as the selected card"
    );
    assert_eq!(
        live_len, 0,
        "Browser and Preview creation must not register PTY-backed live nodes"
    );
    assert!(
        !nodes.iter().any(|node| matches!(
            node.kind,
            CanvasNodeKind::Term(_) | CanvasNodeKind::Agent(_)
        )),
        "Norma Browser/Preview tools must not create Terminal or Agent cards"
    );
}

/// E2E-NORMA-3：Norma 按 1-based 画布位置关闭卡片时只删除目标卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   Initial Canvas nodes = 0
///   Browser cards        = 3 → 顺序为 node 1、node 2、node 3
///   Requested position   = 2（1-based）→ node 2
///   Remaining cards      = 3 - 1 = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless 真实 MultisoulApp 并进入临时项目 Canvas。
///   2. 经 production Adapter 顺序创建三张 Browser 卡片。
///   3. 经公开 e2e seam 执行 `DeleteCanvasCardAtPosition { position: 2 }`。
///   4. 从公开 Runtime 投影读取三个确定 node_id 与剩余节点数。
///
/// 预期结果：
///   - 正断言：关闭回执成功并返回实际删除的 node_id=2。
///   - 正断言：node 1 与 node 3 继续存在，剩余卡片数为 2。
///   - 负断言：node 2 不得继续存在，也不得误删相邻卡片。
#[test]
fn norma_master_closes_only_the_canvas_card_at_spoken_position() {
    let mut gpui = TestAppContext::single();
    let (app, cx) = make_app(&mut gpui, echo_factory());
    let workspace =
        tempfile::TempDir::new().expect("Norma position-close e2e workspace should be creatable");
    enter_canvas(&app, cx, workspace.path());

    for url in [
        "https://first.example",
        "https://second.example",
        "https://third.example",
    ] {
        cx.update(|window, app_cx| {
            app.update(app_cx, |app, cx| {
                app.execute_norma_adapter_command_for_e2e(
                    window,
                    cx,
                    AdapterCommand::CreateBrowser {
                        url: Some(url.to_string()),
                    },
                )
            })
        })
        .expect("each e2e Browser fixture should be created through the production Adapter");
    }

    let closed = cx
        .update(|window, app_cx| {
            app.update(app_cx, |app, cx| {
                app.execute_norma_adapter_command_for_e2e(
                    window,
                    cx,
                    AdapterCommand::DeleteCanvasCardAtPosition { position: 2 },
                )
            })
        })
        .expect("position=2 should close the second Canvas card");
    let (node_1_exists, node_2_exists, node_3_exists, remaining) =
        read_runtime(&app, cx, |runtime| {
            (
                runtime.document.node(1).is_some(),
                runtime.document.node(2).is_some(),
                runtime.document.node(3).is_some(),
                runtime.document.nodes().len(),
            )
        });

    assert_eq!(
        closed.status, "succeeded",
        "production Adapter should report the completed position-close side effect"
    );
    assert_eq!(
        closed.node_id,
        Some(2),
        "1-based position 2 must resolve to and return node_id=2"
    );
    assert!(
        node_1_exists,
        "the first Canvas card must remain after closing the second card"
    );
    assert!(
        !node_2_exists,
        "the second Canvas card must be absent after its successful close"
    );
    assert!(
        node_3_exists,
        "the third Canvas card must remain after closing the second card"
    );
    assert_eq!(
        remaining, 2,
        "closing one of three Canvas cards must leave exactly two cards"
    );
}

/// E2E-NORMA-4：“关闭 Chloe”只结束活跃 AgentCard，不删除显式 Agent 定义。
///
/// 数据构造（含关键数值的推导过程）：
///   Explicit definitions = 1（Chloe）
///   Active AgentCards    = 1
///   Close calls          = 1
///   Remaining cards      = 1 - 1 = 0
///   Remaining definitions = 1（身份保留）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless 真实 MultisoulApp 并进入临时项目 Canvas。
///   2. 经生产 Agent command 创建并激活显式 Chloe/Codex。
///   3. 经 production Norma Adapter 执行 `DeleteAgentCard { agent: "Chloe" }`。
///   4. 从公开 Runtime 与 Agent Settings 投影读取卡片和定义。
///
/// 预期结果：
///   - 正断言：关闭成功并返回 Chloe 原活跃卡片的 node_id。
///   - 正断言：Chloe 显式定义仍存在且保持 enabled。
///   - 负断言：旧 Chloe AgentCard 不再存在，画布中没有其他 AgentCard。
#[test]
fn norma_master_closes_chloe_card_but_preserves_chloe_definition() {
    let mut gpui = TestAppContext::single();
    let (app, cx) = make_app(&mut gpui, echo_factory());
    let workspace =
        tempfile::TempDir::new().expect("Norma Chloe-close e2e workspace should be creatable");
    enter_canvas(&app, cx, workspace.path());

    let created = cx
        .update(|window, app_cx| {
            app.update(app_cx, |app, _cx| {
                app.execute_agent_card_command_for_e2e(
                    window,
                    CardCommand::CreateAgent {
                        name: Some("Chloe".to_string()),
                        profile: AgentProfile::Codex,
                        activate: true,
                    },
                )
            })
        })
        .expect("explicit Chloe definition and active AgentCard should be created");
    let CardCommandResult::AgentCreated {
        node_id: Some(chloe_node_id),
        ..
    } = created
    else {
        panic!("active Chloe creation must return AgentCreated with a node_id");
    };

    let closed = cx
        .update(|window, app_cx| {
            app.update(app_cx, |app, cx| {
                app.execute_norma_adapter_command_for_e2e(
                    window,
                    cx,
                    AdapterCommand::DeleteAgentCard {
                        agent: "Chloe".to_string(),
                    },
                )
            })
        })
        .expect("idle Chloe AgentCard should close through the production Norma Adapter");
    let (old_card_exists, remaining_agent_cards) = read_runtime(&app, cx, |runtime| {
        (
            runtime.document.node(chloe_node_id).is_some(),
            runtime
                .document
                .nodes()
                .iter()
                .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
                .count(),
        )
    });
    let definitions = app.read_with(cx, |app, _cx| app.agent_settings_e2e_snapshot().definitions);

    assert_eq!(
        closed.status, "succeeded",
        "closing idle Chloe should report a completed production side effect"
    );
    assert_eq!(
        closed.node_id,
        Some(chloe_node_id),
        "named close must return Chloe's actual active node id"
    );
    assert_eq!(
        definitions.len(),
        1,
        "closing the only AgentCard must preserve the one explicit Agent definition"
    );
    assert_eq!(
        definitions[0].name, "Chloe",
        "the preserved explicit definition must still be named Chloe"
    );
    assert!(
        definitions[0].enabled,
        "the preserved Chloe definition must remain enabled for later reactivation"
    );
    assert!(
        !old_card_exists,
        "Chloe's old AgentCard must be absent after the successful close"
    );
    assert_eq!(
        remaining_agent_cards, 0,
        "closing the only Chloe card must leave no active AgentCard on the Canvas"
    );
}
