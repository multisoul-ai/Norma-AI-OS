//! Headless GPUI E2E：验证 dirty Agent 通过 Save → Add to Canvas 原子流程创建卡片。

use canvas::CanvasNodeKind;
use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app, project_store, read_runtime};

/// E2E-A-SETTINGS-SAVE-ADD-1：dirty 且有效的 Agent 点击 Add to Canvas 时应先保存再建卡。
///
/// 数据构造（含关键数值的推导过程）：
///   seeded definitions = 1 条：Alan/Codex
///   name edit          = Alan → Alana，增加 1 个字符并形成 dirty
///   initial cards      = 0 张
///   Add click          = 1 次，最终 cards = 0 + 1 = 1 张
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Alan definition，打开 Settings → Agents。
///   2. 把 Name 从 Alan 修改为 Alana，确认 dirty 保存栏出现。
///   3. 不单独点击 Save，直接点击 Add to Canvas。
///   4. 读取 Settings 快照、持久化 definition 与 Runtime Agent card。
///
/// 预期结果：
///   - 正断言：Add 会先把 Alana 保存到 definition，再创建恰好 1 张绑定卡片。
///   - 正断言：新卡片 alias 使用已保存的新名称 Alana。
///   - 负断言：完成后不得继续 dirty，也不得创建仍叫 Alan 的旧名称卡片。
#[test]
fn agent_settings_dirty_add_to_canvas_saves_before_spawning_card() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace =
        tempfile::tempdir().expect("Agent Settings Save → Add e2e 应创建临时 workspace");
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

    click_debug(cx, "settings-agent-name-input");
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("Alana");
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("settings-agent-save").is_some(),
        "修改名称后应进入 dirty，并显示 Save Changes"
    );

    click_debug(cx, "settings-agent-add-to-canvas");
    let snapshot = app.update(cx, |app, _cx| app.agent_settings_e2e_snapshot());
    let cards = read_runtime(&app, cx, |runtime| {
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

    assert!(
        !snapshot.dirty,
        "Save → Add 完成后 draft 应恢复干净状态，不能继续 dirty"
    );
    assert_eq!(
        snapshot.definitions[0].name, "Alana",
        "Add 前必须先把新名称 Alana 保存到 definition"
    );
    assert_ne!(
        snapshot.definitions[0].name, "Alan",
        "Save → Add 完成后 definition 不得仍保留旧名称 Alan"
    );
    assert_eq!(
        cards.len(),
        1,
        "一次 dirty Add to Canvas 应在保存成功后创建恰好 1 张卡片"
    );
    assert_eq!(
        cards[0].definition_id,
        Some(alan.id),
        "新卡片必须绑定原 Alan definition 的稳定 id"
    );
    assert_eq!(
        cards[0].alias.as_deref(),
        Some("Alana"),
        "新卡片 alias 必须使用已保存的新名称 Alana"
    );
    assert_ne!(
        cards[0].alias.as_deref(),
        Some("Alan"),
        "新卡片不得使用保存前的旧名称 Alan"
    );
}

/// E2E-A-SETTINGS-ENABLED-1：Enabled toggle 应进入 dirty，并控制 Add to Canvas 可用性。
///
/// 数据构造（含关键数值的推导过程）：
///   seeded definitions = 1 条：Alan(enabled)
///   toggle clicks      = 2 次：enabled → disabled → enabled
///   save clicks        = 1 次：持久化 disabled
///   Agent cards        = 0 张，测试期间不得创建卡片
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 enabled 的 Alan，打开统一 Inspector。
///   2. 点击 Enabled toggle，检查 dirty 与 disabled Add action。
///   3. 点击 Save Changes，检查 definition 持久化为 disabled。
///   4. 再次点击 toggle，检查 Add action 随 draft 恢复可用，但仍等待保存。
///
/// 预期结果：
///   - 正断言：toggle 后 draft 进入 disabled + dirty，保存后 definition 也为 disabled。
///   - 正断言：再次启用后普通 Add to Canvas action 恢复。
///   - 负断言：disabled 时不得静默保持 enabled，且必须替换成明确的 disabled action。
#[test]
fn agent_settings_enabled_toggle_controls_dirty_state_and_add_availability() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("Agent Settings Enabled e2e 应创建临时 workspace");
    let store = project_store(&app, cx, workspace.path()).agent_definition_store();
    store
        .create(&storage::NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: canvas::AgentProfile::Codex,
            workflow_description: "负责工程实现。".to_string(),
        })
        .expect("测试应创建 enabled 的 Alan definition");
    enter_canvas(&app, cx, workspace.path());
    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-agents");

    click_debug(cx, "settings-agent-toggle-enabled");
    let disabled_draft = app.update(cx, |app, _cx| app.agent_settings_e2e_snapshot());
    assert!(
        !disabled_draft.draft_enabled && disabled_draft.dirty,
        "关闭 Enabled 后 draft 应同时为 disabled 与 dirty"
    );
    assert!(
        cx.debug_bounds("settings-agent-add-to-canvas-disabled")
            .is_some(),
        "disabled draft 应显示不可用的 Add to Canvas 状态"
    );
    click_debug(cx, "settings-agent-save");
    let saved_disabled = app.update(cx, |app, _cx| app.agent_settings_e2e_snapshot());
    assert!(
        !saved_disabled.definitions[0].enabled,
        "Save Changes 后 Alan definition 应持久化为 disabled"
    );
    assert!(
        !saved_disabled.dirty,
        "保存 disabled 状态后 draft 应恢复干净"
    );

    click_debug(cx, "settings-agent-toggle-enabled");
    let enabled_draft = app.update(cx, |app, _cx| app.agent_settings_e2e_snapshot());
    assert!(
        enabled_draft.draft_enabled && enabled_draft.dirty,
        "再次打开 toggle 后 draft 应恢复 enabled，并等待保存"
    );
    assert!(
        cx.debug_bounds("settings-agent-add-to-canvas").is_some(),
        "enabled draft 应恢复可点击的 Add to Canvas action"
    );
}

/// E2E-A-SETTINGS-DIRTY-CLOSE-1：dirty Inspector 不能被关闭并静默丢弃修改。
///
/// 数据构造（含关键数值的推导过程）：
///   seeded definitions = 1 条：Alan
///   name edit          = Alan → Alana，形成 1 份 dirty draft
///   close attempts     = 2 次：dirty 时 1 次，Discard 后 1 次
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开 Alan Inspector，把 Name 修改为 Alana。
///   2. 点击 Settings 关闭按钮，读取 open 与 draft 状态。
///   3. 点击 Discard 恢复 Alan，再次关闭 Settings。
///
/// 预期结果：
///   - 正断言：dirty 时 Settings 保持打开，Alana draft 与 dirty 状态都保留。
///   - 正断言：Discard 后第二次关闭成功。
///   - 负断言：第一次关闭不得静默恢复 Alan 或把 Settings 直接关闭。
#[test]
fn agent_settings_dirty_draft_blocks_close_until_discarded() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace =
        tempfile::tempdir().expect("Agent Settings dirty close e2e 应创建临时 workspace");
    let store = project_store(&app, cx, workspace.path()).agent_definition_store();
    store
        .create(&storage::NewAgentDefinition {
            name: "Alan".to_string(),
            runtime_profile: canvas::AgentProfile::Codex,
            workflow_description: "负责工程实现。".to_string(),
        })
        .expect("测试应创建 Alan definition");
    enter_canvas(&app, cx, workspace.path());
    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-agents");
    click_debug(cx, "settings-agent-name-input");
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("Alana");
    cx.run_until_parked();

    click_debug(cx, "settings-close");
    let blocked_settings = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    let blocked_agent = app.update(cx, |app, _cx| app.agent_settings_e2e_snapshot());
    assert!(
        blocked_settings.settings_open,
        "dirty Inspector 第一次点击关闭时 Settings 必须保持打开"
    );
    assert!(
        blocked_agent.dirty,
        "阻止关闭后 Alana draft 必须继续保持 dirty"
    );
    assert_eq!(
        blocked_agent.draft_name, "Alana",
        "阻止关闭后不得静默把用户输入 Alana 恢复成 Alan"
    );
    assert_ne!(
        blocked_agent.draft_name, "Alan",
        "dirty close guard 不得丢弃未保存的新名称"
    );

    click_debug(cx, "settings-agent-discard");
    click_debug(cx, "settings-close");
    let closed = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(
        !closed.settings_open,
        "Discard 恢复干净后第二次点击关闭应成功关闭 Settings"
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
