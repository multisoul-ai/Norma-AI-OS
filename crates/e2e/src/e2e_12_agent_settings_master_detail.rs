//! Headless GPUI E2E：验证 Agent Settings 主从双栏、只读摘要与显式编辑流程。

use canvas::CanvasNodeKind;
use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app, project_store, read_runtime};

/// E2E-A-SETTINGS-MASTER-DETAIL-1：Agent Settings 默认只读，Edit 后才显示表单，并保留 Add to Canvas。
///
/// 数据构造（含关键数值的推导过程）：
///   seeded definitions = 2 条：Alan/Codex + Bella/OpenCode
///   theme switches     = 2 次：Paper Blue → Forest Mint → Paper Blue，确保真实经过深浅切换
///   initial Agent cards = 0 张
///   Edit click          = 1 次，只切换右侧详情为编辑态
///   Cancel click        = 1 次，恢复右侧只读摘要
///   Add to Canvas click = 1 次，最终 Agent cards = 0 + 1 = 1 张
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向临时 workspace 写入 Alan 与 Bella，再启动 headless App 并进入 Canvas。
///   2. 打开 Settings → Themes，先切到 Forest Mint，再切回 Paper Blue。
///   3. 进入 Agents，检查双栏、Alan/Codex 列表标签和右侧只读摘要。
///   4. 点击 Edit，检查右侧切到编辑态并渲染 Name、Workflow 与 Save。
///   5. 点击 Cancel 返回只读摘要，再点击 Add to Canvas。
///   6. 从 Runtime 读取 Agent 卡片，核对它绑定 Alan definition。
///
/// 预期结果：
///   - 正断言：进入 Agents 时仍是 Paper Blue 浅色主题，且 Codex 标签有真实渲染 bounds。
///   - 负断言：切回 Paper Blue 后不得残留 dark mode。
///   - 正断言：默认存在 master-detail、左侧列表、只读详情、Edit 与 Add to Canvas。
///   - 负断言：默认不存在编辑表单、Name 输入和 Save，避免低频控件常驻。
///   - 正断言：点击 Edit 后出现编辑详情、Name、Workflow 与 Save。
///   - 正断言：Cancel 后 Add to Canvas 创建恰好 1 张绑定 Alan 的卡片。
///   - 负断言：Add to Canvas 不得创建未绑定 definition 的匿名 Agent 卡片。
#[test]
fn agent_settings_uses_readonly_master_detail_before_explicit_edit_and_can_add_to_canvas() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("Agent Settings 双栏 e2e 应创建临时 workspace");
    let store = project_store(&app, cx, workspace.path()).agent_definition_store();
    let alan = store
        .create(&storage::NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: canvas::AgentProfile::Codex,
            workflow_description: "负责工程实现与验证。".to_string(),
        })
        .expect("测试应创建 Alan/Codex definition");
    store
        .create(&storage::NewAgentDefinition {
            name: "Bella".to_string(),
            runtime_profile: canvas::AgentProfile::OpenCode,
            workflow_description: "负责代码审查。".to_string(),
        })
        .expect("测试应创建 Bella/OpenCode definition");
    enter_canvas(&app, cx, workspace.path());

    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-themes");
    click_debug(cx, "settings-theme-forest-mint");
    click_debug(cx, "settings-theme-paper-blue");
    let light_theme = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        light_theme.active_theme_id, "paper-blue",
        "进入 Agent Settings 前应明确切回 Paper Blue 浅色主题"
    );
    assert!(
        !light_theme.theme_is_dark,
        "切回 Paper Blue 后不得残留 Forest Mint 的 dark mode"
    );
    click_debug(cx, "settings-section-agents");

    assert!(
        cx.debug_bounds("settings-agents-master-detail").is_some(),
        "Agents 页面默认应渲染主从双栏容器"
    );
    assert!(
        cx.debug_bounds("settings-agents-list").is_some(),
        "主从双栏左侧应渲染可选择的 Agent 列表"
    );
    assert!(
        cx.debug_bounds("settings-agent-list-runtime-profile")
            .is_some(),
        "Paper Blue 下 Alan 行必须渲染可见的 Codex Runtime Profile 标签"
    );
    assert!(
        cx.debug_bounds("settings-agent-detail-readonly").is_some(),
        "主从双栏右侧默认应渲染只读详情摘要"
    );
    assert!(
        cx.debug_bounds("settings-agent-edit").is_some(),
        "只读摘要应提供显式 Edit action"
    );
    assert!(
        cx.debug_bounds("settings-agent-add-to-canvas").is_some(),
        "只读摘要应保留 Add to Canvas action"
    );
    assert!(
        cx.debug_bounds("settings-agent-detail-editing").is_none(),
        "默认只读状态不应提前渲染编辑详情"
    );
    assert!(
        cx.debug_bounds("settings-agent-name-input").is_none(),
        "默认只读状态不应常驻 Name 输入框"
    );
    assert!(
        cx.debug_bounds("settings-agent-save").is_none(),
        "默认只读状态不应常驻 Save action"
    );

    click_debug(cx, "settings-agent-edit");

    assert!(
        cx.debug_bounds("settings-agent-detail-editing").is_some(),
        "点击 Edit 后右侧应切换为编辑详情"
    );
    assert!(
        cx.debug_bounds("settings-agent-name-input").is_some(),
        "点击 Edit 后应渲染 Name 输入框"
    );
    assert!(
        cx.debug_bounds("settings-agent-workflow-input").is_some(),
        "点击 Edit 后应渲染 Workflow 输入框"
    );
    assert!(
        cx.debug_bounds("settings-agent-save").is_some(),
        "点击 Edit 后应渲染 Save action"
    );

    click_debug(cx, "settings-agent-cancel-edit");
    click_debug(cx, "settings-agent-add-to-canvas");

    let agent_specs = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter_map(|node| {
                let CanvasNodeKind::Agent(spec) = &node.kind else {
                    return None;
                };
                Some(spec.clone())
            })
            .collect::<Vec<_>>()
    });

    assert_eq!(
        agent_specs.len(),
        1,
        "一次 Add to Canvas 应创建恰好 1 张 Agent 卡片"
    );
    assert_eq!(
        agent_specs[0].definition_id,
        Some(alan.id),
        "Add to Canvas 应创建绑定当前选中 Alan definition 的卡片"
    );
    assert_eq!(
        agent_specs[0].alias.as_deref(),
        Some("Alan"),
        "Add to Canvas 创建的卡片应显示当前选中 Agent 名称 Alan"
    );
    assert!(
        agent_specs[0].definition_id.is_some(),
        "Add to Canvas 不得创建 definition_id=None 的匿名 Agent 卡片"
    );
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("selector `{selector}` should have debug bounds before click"));
    let position = bounds.center();
    cx.simulate_mouse_move(position, None, Modifiers::none());
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
}
