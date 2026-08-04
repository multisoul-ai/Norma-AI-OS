//! Headless GPUI E2E：验证 Agent Settings 的显式删除入口与确认对话框。

use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app, project_store, read_runtime};

/// E2E-A-SETTINGS-DELETE-1：已保存 Agent 必须通过显式按钮和确认对话框才能删除。
///
/// 数据构造（含关键数值的推导过程）：
///   seeded definitions   = 2 条：Alan + Bella
///   delete button clicks = 2 次：第一次验证 Esc 取消，第二次再次打开确认对话框
///   confirm clicks       = 1 次，definitions 从 2 - 1 = 1 条
///   escape presses       = 1 次，关闭对话框但 definitions 仍为 2 条
///   active Agent cards   = 0 张，满足可删除前置条件
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向临时 workspace 写入 Alan 与 Bella，再打开 Settings → Agents。
///   2. 点击 Inspector 底部的 `Delete Agent…`。
///   3. 检查确认对话框存在，且 Alan 在确认前仍保留。
///   4. 按 Esc 关闭确认对话框，确认 definitions 未变化；再重新打开对话框。
///   5. 点击对话框确认按钮，再读取 Agent Settings 快照。
///
/// 预期结果：
///   - 正断言：删除按钮和 `Delete Alan?` 确认对话框均可见。
///   - 正断言：确认前 Alan 与 Bella 两条 definition 都存在。
///   - 正断言：Esc 可取消模态确认且不删除任何 Agent。
///   - 正断言：确认后只保留 Bella，并自动把 Bella 加载到 Inspector。
///   - 负断言：点击删除按钮本身不得立即删除 Alan；确认后 Alan 不得残留。
#[test]
fn agent_settings_delete_requires_visible_dialog_confirmation() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("Agent Settings 删除 e2e 应创建临时 workspace");
    let store = project_store(&app, cx, workspace.path()).agent_definition_store();
    let alan = store
        .create(&storage::NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: canvas::AgentProfile::Codex,
            workflow_description: "负责工程实现。".to_string(),
        })
        .expect("测试应创建待删除的 Alan definition");
    let bella = store
        .create(&storage::NewAgentDefinition {
            name: "Bella".to_string(),
            runtime_profile: canvas::AgentProfile::Claude,
            workflow_description: "负责需求分析。".to_string(),
        })
        .expect("测试应创建应保留的 Bella definition");
    enter_canvas(&app, cx, workspace.path());
    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-agents");

    assert!(
        cx.debug_bounds("settings-agent-delete").is_some(),
        "已保存 Agent 的 Inspector 底部必须显示 Delete Agent action"
    );
    click_debug(cx, "settings-agent-delete");

    assert!(
        cx.debug_bounds("settings-agent-delete-dialog").is_some(),
        "点击 Delete Agent 后必须显示确认对话框"
    );
    let before_confirm = app.update(cx, |app, _cx| app.agent_settings_e2e_snapshot());
    assert_eq!(
        before_confirm.definitions.len(),
        2,
        "只打开确认对话框时不得提前删除 Alan"
    );
    assert!(
        before_confirm
            .definitions
            .iter()
            .any(|definition| definition.id == alan.id),
        "确认前待删除的 Alan definition 必须仍然存在"
    );
    assert!(
        before_confirm
            .definitions
            .iter()
            .any(|definition| definition.id == bella.id),
        "确认前应保留的 Bella definition 必须存在"
    );

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    let after_escape = app.update(cx, |app, _cx| app.agent_settings_e2e_snapshot());
    assert!(
        cx.debug_bounds("settings-agent-delete-dialog").is_none(),
        "按 Esc 后 Delete Agent 确认对话框必须关闭"
    );
    assert_eq!(
        after_escape.definitions.len(),
        2,
        "按 Esc 取消删除后 Alan 与 Bella 两条 definition 都必须保留"
    );
    assert!(
        after_escape
            .definitions
            .iter()
            .any(|definition| definition.id == alan.id),
        "按 Esc 取消删除后 Alan definition 不得被移除"
    );
    assert!(
        after_escape
            .definitions
            .iter()
            .any(|definition| definition.id == bella.id),
        "按 Esc 取消删除后 Bella definition 必须继续保留"
    );

    click_debug(cx, "settings-agent-delete");
    assert!(
        cx.debug_bounds("settings-agent-delete-dialog").is_some(),
        "Esc 取消后再次点击 Delete Agent 应重新打开确认对话框"
    );
    click_debug(cx, "settings-agent-delete-confirm");
    let after_confirm = app.update(cx, |app, _cx| app.agent_settings_e2e_snapshot());
    assert_eq!(
        after_confirm.definitions.len(),
        1,
        "确认删除后 definitions 应从 2 条减少为 1 条"
    );
    assert_eq!(
        after_confirm.definitions[0].id, bella.id,
        "确认删除 Alan 后必须保留 Bella"
    );
    assert_eq!(
        after_confirm.draft_name, "Bella",
        "删除当前 Agent 后 Inspector 应自动加载 Bella"
    );
    assert!(
        !after_confirm
            .definitions
            .iter()
            .any(|definition| definition.id == alan.id),
        "确认删除后 Alan definition 不得继续残留"
    );
}

/// E2E-A-SETTINGS-DELETE-2：存在 active Agent card 时删除必须进入阻塞态。
///
/// 数据构造（含关键数值的推导过程）：
///   seeded definitions = 1 条：Alan
///   initial cards      = 0 张
///   Add click          = 1 次，active cards = 0 + 1 = 1 张
///   delete attempts    = 1 次，definitions 仍为 1 条
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Alan definition，打开 Settings → Agents。
///   2. 点击 Add to Canvas，生成绑定 Alan 的 active card。
///   3. 点击显式 Delete Agent，检查阻塞对话框。
///   4. 读取快照并关闭阻塞提示，再点击 Show on Canvas。
///
/// 预期结果：
///   - 正断言：阻塞对话框与 OK action 可见，Alan definition 仍存在。
///   - 正断言：已有 active card 后 Header 用 `On Canvas` 状态替换 Add action。
///   - 正断言：Show on Canvas 会关闭 Settings，并聚焦唯一的关联卡片。
///   - 负断言：active card 存在时不得显示 Delete Agent 确认 action，也不得删除 Alan。
#[test]
fn agent_settings_delete_is_blocked_while_definition_has_active_card() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("Agent Settings 删除阻塞 e2e 应创建临时 workspace");
    let store = project_store(&app, cx, workspace.path()).agent_definition_store();
    let alan = store
        .create(&storage::NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: canvas::AgentProfile::Codex,
            workflow_description: "负责工程实现。".to_string(),
        })
        .expect("测试应创建 Alan definition");
    enter_canvas(&app, cx, workspace.path());
    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-agents");
    click_debug(cx, "settings-agent-add-to-canvas");

    assert!(
        cx.debug_bounds("settings-agent-on-canvas").is_some(),
        "Add to Canvas 后 Inspector 应即时显示 On Canvas 状态"
    );
    assert!(
        cx.debug_bounds("settings-agent-show-on-canvas").is_some(),
        "存在 active card 时 Inspector 应提供 Show on Canvas action"
    );
    let indicator_bounds_after_spawn = cx.debug_bounds("settings-agent-on-canvas-indicator");
    assert!(
        indicator_bounds_after_spawn.is_some(),
        "已有 active card 时 Header 应以明确的 On Canvas 状态替换 Add action；indicator={indicator_bounds_after_spawn:?}"
    );

    click_debug(cx, "settings-agent-delete");
    assert!(
        cx.debug_bounds("settings-agent-delete-dialog").is_some(),
        "active card 存在时点击 Delete Agent 应显示阻塞对话框"
    );
    assert!(
        cx.debug_bounds("settings-agent-delete-cancel").is_some(),
        "阻塞对话框应提供可关闭的 OK action"
    );
    assert!(
        cx.debug_bounds("settings-agent-delete-confirm").is_none(),
        "active card 存在时不得提供真正执行删除的确认 action"
    );
    let blocked = app.update(cx, |app, _cx| app.agent_settings_e2e_snapshot());
    assert_eq!(
        blocked.definitions.len(),
        1,
        "删除阻塞态不得移除唯一的 Alan definition"
    );
    assert_eq!(
        blocked.definitions[0].id, alan.id,
        "删除阻塞态必须保留 active card 所绑定的 Alan definition"
    );
    assert!(
        blocked
            .definitions
            .iter()
            .any(|definition| definition.id == alan.id),
        "active card 存在时 Alan definition 必须继续存在"
    );

    click_debug(cx, "settings-agent-delete-cancel");
    click_debug(cx, "settings-agent-show-on-canvas");
    let settings = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    let (focus, selected_ids) = read_runtime(&app, cx, |runtime| {
        (
            runtime.focus.mode(),
            runtime.selection.iter().collect::<Vec<_>>(),
        )
    });
    assert!(
        !settings.settings_open,
        "Show on Canvas 应关闭 Settings，让用户回到 Canvas"
    );
    assert_eq!(
        selected_ids.len(),
        1,
        "Show on Canvas 后应只选中 1 张关联 Agent card"
    );
    assert_eq!(
        focus,
        runtime::InputMode::NodeFocused(selected_ids[0]),
        "Show on Canvas 后焦点必须落到被选中的关联 Agent card"
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
