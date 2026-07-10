//! E2E-03：Settings 模块真实点击覆盖。
//!
//! 本测试不直接调用 `toggle_settings_panel`、`settings_panel.select`、
//! `begin_recording` 或 `restore_default` 等处理函数；它只通过
//! `VisualTestContext::simulate_click` 点击已渲染控件的 debug bounds，并通过
//! `simulate_keystrokes` 输入真实按键序列。这样覆盖的是 GPUI hitbox →
//! mouse down/up → on_click listener → app state 的完整链路。

use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// Settings 面板点击链路：打开面板、切换 Keyboard、录制 New Terminal 快捷键、恢复默认、关闭。
///
/// 场景：用户在 Canvas 顶部工具栏点击 Settings，进入 Keyboard Shortcuts，
/// 点击 New Terminal keycap 进入录制，键入 `cmd-alt-n` 保存覆盖，再点击该行
/// Restore 按钮恢复默认，再切到 Voice 选择 OpenAI，最后点击右上角关闭按钮。
///
/// 数据构造（含关键数值推导过程）：
///   toolbar-settings              = 30px × 30px（`chrome_icon_button` 固定尺寸）
///   settings-panel                >= 860px × 620px（`PANEL_MIN_W/H`）
///   settings-section-keyboard     = 42px 高（settings sidebar section button）
///   settings-shortcut-new-terminal = 88px min-width × 32px 高（keycap）
///   New Terminal 默认快捷键       = `cmd-n`
///   测试覆盖快捷键                = `cmd-alt-n`（带 modifier，非 bare printable）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `make_app + enter_canvas` 创建 sandbox/headless GPUI 窗口并进入 Canvas。
///   2. 点击 `toolbar-settings` → Settings overlay/panel 渲染。
///   3. 点击 `settings-section-keyboard` → active section 变为 Keyboard。
///   4. 点击 `settings-shortcut-new-terminal` → keycap on_click 开始录制。
///   5. 输入 `cmd-alt-n` → `ShortcutSettingsState::record_keystroke` 保存覆盖。
///   6. 点击 `settings-shortcut-restore-new-terminal` → 恢复默认 `cmd-n`。
///   7. 点击 Voice → OpenAI，验证统一 Agent Provider 选择。
///   8. 点击 `settings-close` → Settings panel 关闭且 close hitbox 消失。
///
/// 预期结果：
///   - 正断言：每个可点击控件在点击前都存在 debug bounds，证明点击目标来自真实渲染树。
///   - 正断言：保存覆盖后 restore 按钮出现，证明 keycap click + keystroke 生效。
///   - 正断言：恢复默认后 New Terminal keycap 仍存在，证明 Settings 面板仍可操作。
///   - 正断言：点击 OpenAI 后 snapshot.provider=openai。
///   - 负断言：恢复默认后 `new_terminal_modified == false`，证明覆盖已清除。
///   - 负断言：关闭后 `settings_open == false`，证明面板已关闭。
///
/// 注意：GPUI 0.2.2 的 `debug_bounds` 是 click targeting 工具，不是 absence oracle：
/// `Frame::clear` 不清理 `debug_bounds`，因此元素消失后的负断言必须走 app snapshot。
#[test]
fn e2e_03_settings_keyboard_shortcut_can_be_changed_and_restored_by_clicks() {
    let mut cx = TestAppContext::single();

    let (app, cx) = make_app(&mut cx, echo_factory());
    let _tmp = tempfile::TempDir::new().expect("tempdir for workspace root should create");
    enter_canvas(&app, cx, _tmp.path());

    click_debug(cx, "toolbar-settings");
    assert!(
        cx.debug_bounds("settings-panel").is_some(),
        "clicking toolbar-settings should render the settings panel"
    );
    assert!(
        cx.debug_bounds("settings-close").is_some(),
        "opened settings panel should expose a close hit target"
    );

    click_debug(cx, "settings-section-keyboard");
    assert!(
        cx.debug_bounds("settings-shortcut-new-terminal").is_some(),
        "clicking the Keyboard section should render the New Terminal keycap"
    );
    let initial = app.update(cx, |app, _cx| app.settings_e2e_snapshot());
    assert!(
        !initial.new_terminal_modified,
        "New Terminal should start at its default shortcut, snapshot: {:?}",
        initial
    );

    click_debug(cx, "settings-shortcut-new-terminal");
    let recording = app.update(cx, |app, _cx| app.settings_e2e_snapshot());
    assert_eq!(
        recording.recording_command,
        Some("new_terminal"),
        "clicking the New Terminal keycap should begin recording, snapshot: {:?}",
        recording
    );
    cx.simulate_keystrokes("cmd-alt-n");
    let modified = app.update(cx, |app, _cx| app.settings_e2e_snapshot());
    assert_eq!(
        modified.recording_command, None,
        "recording should clear after accepting cmd-alt-n, snapshot: {:?}",
        modified
    );
    assert!(
        modified.new_terminal_modified,
        "cmd-alt-n should mark New Terminal modified, snapshot: {:?}",
        modified
    );
    assert!(
        cx.debug_bounds("settings-shortcut-restore-new-terminal")
            .is_some(),
        "after recording cmd-alt-n, the New Terminal row should expose Restore"
    );

    click_debug(cx, "settings-shortcut-restore-new-terminal");
    let restored = app.update(cx, |app, _cx| app.settings_e2e_snapshot());
    assert!(
        !restored.new_terminal_modified,
        "clicking Restore should clear New Terminal modification, snapshot: {:?}",
        restored
    );
    assert!(
        cx.debug_bounds("settings-shortcut-new-terminal").is_some(),
        "restoring the shortcut should keep the New Terminal keycap visible"
    );

    click_debug(cx, "settings-section-voice");
    let voice = app.update(cx, |app, _cx| app.settings_e2e_snapshot());
    assert_eq!(
        voice.active_section, "Voice",
        "点击 Voice 后应切换到 Voice Settings，snapshot: {:?}",
        voice
    );
    assert!(
        cx.debug_bounds("settings-agent-provider-openai").is_some(),
        "Voice Settings 应渲染 OpenAI Provider 选择按钮，snapshot: {:?}",
        voice
    );
    click_debug(cx, "settings-agent-provider-openai");
    let openai = app.update(cx, |app, _cx| app.settings_e2e_snapshot());
    assert_eq!(
        openai.agent_provider, "openai",
        "点击 OpenAI 后全局 Agent Provider 应立即切换"
    );
    assert_ne!(
        openai.agent_provider, "ollama",
        "OpenAI 被选中后不应继续报告 Ollama"
    );

    click_debug(cx, "settings-close");
    let closed = app.update(cx, |app, _cx| app.settings_e2e_snapshot());
    assert!(
        !closed.settings_open,
        "after clicking close, the settings panel should be closed, snapshot: {:?}",
        closed
    );
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("selector `{selector}` should have debug bounds before click"));
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}
