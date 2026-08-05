//! Orchestration Settings：固定验收数据、组件展开和安全卸载确认的渲染树闭环。

use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// Orchestration 固定状态：五个 Provider 的状态、操作与 Cursor 四组件必须精确呈现。
///
/// 数据构造（含关键数值的推导过程）：
///   Provider = 5：Codex + Claude + Cursor + OpenCode + Grok。
///   Ready = 2：Codex + OpenCode；Not installed = 1：Claude。
///   Needs repair = 1：Cursor，其组件 = 3×Ready + Hook×Needs repair。
///   Not supported = 1：Grok；Cursor 展开组件数 = 4。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless App、进入 Canvas 并打开 Settings → Orchestration。
///   2. 注入与 canonical golden image 一致的固定验收数据并读取公开投影。
///   3. 验证 Provider 顺序、汇总状态、操作文案与 Cursor 四组件。
///   4. 点击 Cursor disclosure 两次，验证折叠与重新展开。
///   5. 点击 Ready Codex 的 Uninstall，验证先进入 Confirm 而不立即执行文件删除。
///   6. 点击 Grok 的禁用 Unavailable，验证它不能改变待确认 Provider。
///
/// 预期结果：
///   - 正断言：五个 Provider 与 canonical 状态、操作逐项一致。
///   - 正断言：Cursor 展开详情精确显示 Skill/CLI/Authorization Ready、Hook Needs repair。
///   - 正断言：Ready 卸载需要二次确认，展开按钮可逆。
///   - 负断言：Grok 不得触发安装或覆盖 Codex 的待确认卸载状态。
#[test]
fn e2e_03_orchestration_renders_canonical_states_and_safe_actions() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::tempdir().expect("Orchestration e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-orchestration");
    app.update(cx, |app, cx| {
        app.use_canonical_orchestration_preview_for_e2e(cx)
    });
    cx.run_until_parked();

    let initial = app.update(cx, |app, _cx| app.orchestration_settings_e2e_snapshot());
    assert_eq!(
        initial
            .providers
            .iter()
            .map(|provider| provider.provider)
            .collect::<Vec<_>>(),
        vec![
            "Codex",
            "Claude Code",
            "Cursor Agent CLI",
            "OpenCode",
            "Grok Build",
        ],
        "Provider 顺序必须与 canonical golden image 完全一致，snapshot={initial:?}"
    );
    assert_eq!(
        initial
            .providers
            .iter()
            .map(|provider| (provider.status, provider.action))
            .collect::<Vec<_>>(),
        vec![
            ("Ready", "Uninstall"),
            ("Not installed", "Install"),
            ("Needs repair", "Repair"),
            ("Ready", "Uninstall"),
            ("Not supported", "Unavailable"),
        ],
        "五个 Provider 的汇总状态和单一主操作必须精确匹配验收数据"
    );
    let cursor = &initial.providers[2];
    assert_eq!(
        (cursor.skill, cursor.cli, cursor.hook, cursor.authorization,),
        ("Ready", "Ready", "Needs repair", "Ready"),
        "Cursor 展开详情必须逐项暴露四组件真实状态"
    );
    assert_eq!(
        initial.expanded_provider,
        Some("Cursor Agent CLI"),
        "canonical 场景必须默认展开 Cursor Agent CLI"
    );
    assert!(
        cx.debug_bounds("settings-orchestration-cursor-action")
            .is_some(),
        "Cursor Repair 主操作必须存在稳定 debug selector"
    );
    assert!(
        cx.debug_bounds("settings-orchestration-grok-action")
            .is_some(),
        "Grok Unavailable 禁用操作仍必须可被自动化定位"
    );

    click_debug(cx, "settings-orchestration-cursor-disclosure");
    let collapsed = app.update(cx, |app, _cx| app.orchestration_settings_e2e_snapshot());
    assert_eq!(
        collapsed.expanded_provider, None,
        "第一次点击 Cursor disclosure 必须折叠组件详情"
    );
    click_debug(cx, "settings-orchestration-cursor-disclosure");
    let expanded = app.update(cx, |app, _cx| app.orchestration_settings_e2e_snapshot());
    assert_eq!(
        expanded.expanded_provider,
        Some("Cursor Agent CLI"),
        "第二次点击 Cursor disclosure 必须重新展开同一 Provider"
    );

    click_debug(cx, "settings-orchestration-codex-action");
    let confirming = app.update(cx, |app, _cx| app.orchestration_settings_e2e_snapshot());
    assert_eq!(
        confirming.pending_uninstall,
        Some("Codex"),
        "首次点击 Ready Provider 只能进入 Codex 卸载确认"
    );
    assert_eq!(
        confirming.providers[0].action, "Confirm",
        "Codex 待确认时主操作必须明确变为 Confirm"
    );

    click_debug(cx, "settings-orchestration-grok-action");
    let after_grok = app.update(cx, |app, _cx| app.orchestration_settings_e2e_snapshot());
    assert_eq!(
        after_grok.pending_uninstall,
        Some("Codex"),
        "点击禁用 Grok 操作不得触发安装或改变 Codex 的待确认状态"
    );
    assert_ne!(
        after_grok.providers[4].action, "Install",
        "Grok 首版不得暴露可执行 Install 操作"
    );
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("selector `{selector}` 应在点击前存在渲染 bounds"));
    let position = bounds.center();
    cx.simulate_mouse_move(position, None, Modifiers::none());
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
}
