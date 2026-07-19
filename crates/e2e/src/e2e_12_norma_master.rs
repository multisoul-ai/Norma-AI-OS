use crate::{echo_factory, enter_canvas, make_app, read_runtime};
use agent::AdapterCommand;
use canvas::CanvasNodeKind;
use gpui::TestAppContext;
use norma::protocol::{AgentProfile, PreviewType};
use std::path::PathBuf;

/// E2E-NORMA-1：Norma 的生产 Adapter 用内置名称创建 AgentCard，并直接派发单 Agent 任务
///
/// 数据构造（含关键数值的推导过程）：
///   Fresh project definitions = 0
///   Create calls = 1（Codex）
///   Assign calls = 1（创建所得内置名称）
///   Expected Canvas AgentCards = 1 + 0 reused by assignment = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless 真实 MultisoulApp 并进入临时项目 Canvas
///   2. 经 production `AgentCardAdapter` 执行 `create_agent(codex)`
///   3. 从 Agent 设置投影读取系统生成的名称，再立即执行 `assign_agent_task`
///
/// 预期结果：
///   - 正断言：创建成功、任务 accepted 且返回 task_id=1
///   - 正断言：画布只有一张 AgentCard，派发复用该卡片
///   - 负断言：系统生成的 Agent 不能名为 Norma
#[test]
fn norma_master_create_then_direct_assign_reaches_real_app_fifo() {
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
                },
            )
        })
    });
    let created = created.expect("production Norma create_agent should succeed");
    let definition = app.read_with(cx, |app, _cx| {
        app.agent_settings_e2e_snapshot().definitions[0].clone()
    });
    let assigned = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            app.execute_norma_adapter_command_for_e2e(
                window,
                cx,
                AdapterCommand::AssignAgentTask {
                    agent: definition.name.clone(),
                    task: "检查当前项目有哪些技术栈。".to_string(),
                },
            )
        })
    });
    let assigned = assigned.expect("production Norma assign_agent_task should succeed directly");
    let agent_cards = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .count()
    });

    assert_eq!(
        created.status, "succeeded",
        "create_agent should report a real successful App mutation"
    );
    assert_eq!(
        assigned.status, "accepted",
        "assign_agent_task should report acceptance without waiting for task completion"
    );
    assert_eq!(
        assigned.task_id,
        Some(1),
        "the first real AgentCard FIFO task should expose task_id=1"
    );
    assert_eq!(
        agent_cards, 1,
        "direct assignment should reuse the single AgentCard opened during creation"
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
