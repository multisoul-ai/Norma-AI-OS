//! Ego Lite 外部窗口卡片：真实 GPUI 菜单与 Canvas 渲染树的 L1 验收。

use canvas::{BrowserSurfaceKind, CanvasNodeKind, EgoCardPhase};
use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// E2E-EGO-01：用户从 New 菜单创建外部 Ego 卡片，重复打开复用该卡，并且忙碌任务不可关闭。
///
/// 数据构造（含关键数值的推导过程）：
///   临时 workspace = 1；受控 Task Space = 1（id="e2e-space-7"）
///   菜单点击 = 4：New→Ego（首次）+ New→Ego（重复）
///   Ego 卡片上限 = 1；绑定测试 Agent node_id = 73
///   关闭点击 = 1：AgentWorking 阶段必须拒绝，因此剩余卡片数 = 1 - 0 = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在真实 headless GPUI Canvas 中为 Ego helper 注入确定性结果，避免触碰本机真实 Task Space。
///   2. 点击底部 New 菜单的 Ego 项，推进异步结果并读取画布节点与渲染树。
///   3. 再次通过相同菜单打开 Ego，验证它复用同一张卡且没有自动进入 Master。
///   4. 将该卡推进到 AgentWorking，点击卡片关闭按钮并读取剩余节点与工作区错误。
///
/// 预期结果：
///   - 正断言：恰好一张 EgoLite Browser 卡，精确记录受控 Task Space ID，渲染外部窗口卡片与打开按钮。
///   - 正断言：重复入口仍只有一张卡，且 Master 为空，证明没有自动接管视觉主位。
///   - 正断言：工作态点击关闭后卡片仍保留，并显示清晰的任务进行中错误。
///   - 负断言：不得渲染 Native Browser surface；不得创建第二张 Ego 卡；不得在忙碌时删除卡片。
#[test]
fn e2e_ego_card_is_external_singleton_and_busy_close_is_blocked() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("Ego e2e 应创建隔离 workspace");
    enter_canvas(&app, cx, workspace.path());
    app.update(cx, |app, _cx| {
        app.set_next_ego_task_space_for_e2e("e2e-space-7", "Norma · Ego · E2E");
    });

    click_debug(cx, "toolbar-new-terminal");
    click_debug(cx, "new-menu-ego");

    let (first_nodes, first_phase, first_master) = app.read_with(cx, |app, _cx| {
        (
            app.runtime.document.nodes().to_vec(),
            app.runtime
                .ego_node_id()
                .and_then(|id| app.runtime.ego_task_state(id)),
            app.ego_card_e2e_snapshot().master_id,
        )
    });
    let ego_nodes: Vec<_> = first_nodes
        .iter()
        .filter(|node| {
            matches!(
                &node.kind,
                CanvasNodeKind::Browser(spec)
                    if matches!(&spec.surface, BrowserSurfaceKind::EgoLite(_))
            )
        })
        .collect();
    assert_eq!(
        ego_nodes.len(),
        1,
        "首次 New → Ego 后必须恰好创建一张 EgoLite 卡片"
    );
    let ego_node = ego_nodes
        .first()
        .expect("已断言 EgoLite 卡片数量为 1，首张卡片必须可读取");
    let ego_node_id = ego_node.id;
    let task_space_id = match &ego_node.kind {
        CanvasNodeKind::Browser(spec) => match &spec.surface {
            BrowserSurfaceKind::EgoLite(ego) => ego.task_space_id.as_deref(),
            BrowserSurfaceKind::Native => None,
        },
        _ => None,
    };

    assert_eq!(
        first_nodes.len(),
        1,
        "首次 New → Ego 不得同时创建其它画布节点"
    );
    assert_eq!(
        task_space_id,
        Some("e2e-space-7"),
        "Ego 卡片必须保存 helper 返回的精确 Task Space ID"
    );
    assert_eq!(
        first_phase.map(|state| state.phase),
        Some(EgoCardPhase::ReadyForUser),
        "受控 helper 完成后 Ego 卡片必须进入 ReadyForUser"
    );
    assert_eq!(
        first_master, None,
        "用户从 New 菜单创建 Ego 时不得自动设置 Master"
    );
    assert!(
        cx.debug_bounds("ego-card").is_some(),
        "Ego 卡片必须作为真实 GPUI 外部窗口状态卡渲染"
    );
    assert!(
        cx.debug_bounds("ego-open-button").is_some(),
        "Ego 状态卡必须提供打开独立 Ego Lite 窗口的按钮"
    );
    assert!(
        cx.debug_bounds("browser-native-surface").is_none(),
        "Ego 外部窗口模式不得误渲染原生 Browser surface"
    );

    click_debug(cx, "toolbar-new-terminal");
    click_debug(cx, "new-menu-ego");
    let reused = app.read_with(cx, |app, _cx| app.ego_card_e2e_snapshot());

    assert_eq!(
        reused.node_ids,
        vec![ego_node_id],
        "重复 New → Ego 必须复用首张卡片，不能创建第二张"
    );
    assert_eq!(
        reused.task_space_ids,
        vec![Some("e2e-space-7".to_string())],
        "重复入口不得替换或丢失已有 Task Space ID"
    );
    assert_eq!(
        reused.master_id, None,
        "重复 New → Ego 也不得悄悄自动进入 Master"
    );

    app.update(cx, |app, cx| {
        app.set_ego_agent_working_for_e2e(ego_node_id, 73, cx)
            .expect("E2E 应能把已就绪 Ego 卡推进到 AgentWorking");
    });
    cx.run_until_parked();
    click_debug(cx, "node-close");
    let (after_close, close_error) = app.read_with(cx, |app, _cx| {
        (app.ego_card_e2e_snapshot(), app.workspace_error_for_e2e())
    });

    assert_eq!(
        after_close.node_ids,
        vec![ego_node_id],
        "AgentWorking 的 Ego 卡片点击关闭后必须仍存在"
    );
    assert_eq!(
        after_close.phases,
        vec!["agent_working"],
        "忙碌关闭被拒绝后卡片阶段必须仍是 agent_working"
    );
    assert!(
        close_error
            .as_deref()
            .is_some_and(|message| message.contains("任务仍在进行中")),
        "忙碌关闭必须给用户明确的任务进行中反馈，实际：{close_error:?}"
    );
    assert_ne!(
        after_close.node_ids,
        Vec::<u64>::new(),
        "忙碌关闭不得把唯一 Ego 卡片删除为空画布"
    );
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("Ego e2e 点击目标必须已渲染：{selector}"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}
