//! E2E-03：Settings 模块真实点击覆盖。
//!
//! 本测试不直接调用 `toggle_settings_panel`、`settings_panel.select`、
//! `begin_recording` 或 `restore_default` 等处理函数；它只通过
//! `VisualTestContext::simulate_click` 点击已渲染控件的 debug bounds，并通过
//! `simulate_keystrokes` 或 `simulate_input` 输入真实按键与文本。Voice 文本输入必须经过
//! GPUI 当前聚焦节点及平台 `InputHandler`，测试不得直接调用 Voice state 或输入 helper。

use gpui::{
    point, px, ClipboardItem, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point,
    ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext,
};

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
///   New Terminal 默认快捷键       = `cmd-1`
///   测试覆盖快捷键                = `cmd-alt-n`（带 modifier，非 bare printable）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `make_app + enter_canvas` 创建 sandbox/headless GPUI 窗口并进入 Canvas。
///   2. 点击 `toolbar-settings` → Settings overlay/panel 渲染。
///   3. 点击 `settings-section-keyboard` → active section 变为 Keyboard。
///   4. 点击 `settings-shortcut-new-terminal` → keycap on_click 开始录制。
///   5. 输入 `cmd-alt-n` → `ShortcutSettingsState::record_keystroke` 保存覆盖。
///   6. 点击 `settings-shortcut-restore-new-terminal` → 恢复默认 `cmd-1`。
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
    let initial = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(
        !initial.new_terminal_modified,
        "New Terminal should start at its default shortcut, snapshot: {:?}",
        initial
    );
    assert_eq!(
        initial.new_terminal_shortcut.as_deref(),
        Some("cmd-1"),
        "New Terminal 的 Settings 默认键位应显示 cmd-1，snapshot: {:?}",
        initial
    );

    click_debug(cx, "settings-shortcut-new-terminal");
    let recording = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        recording.recording_command,
        Some("new_terminal"),
        "clicking the New Terminal keycap should begin recording, snapshot: {:?}",
        recording
    );
    cx.simulate_keystrokes("cmd-alt-n");
    let modified = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
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
    let restored = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(
        !restored.new_terminal_modified,
        "clicking Restore should clear New Terminal modification, snapshot: {:?}",
        restored
    );
    assert_eq!(
        restored.new_terminal_shortcut.as_deref(),
        Some("cmd-1"),
        "恢复默认后 New Terminal 应回到 cmd-1，snapshot: {:?}",
        restored
    );
    assert!(
        cx.debug_bounds("settings-shortcut-new-terminal").is_some(),
        "restoring the shortcut should keep the New Terminal keycap visible"
    );

    click_debug(cx, "settings-section-voice");
    click_debug(cx, "settings-voice-mode-continuous");
    scroll_debug(cx, "settings-content-scroll", -650.0);
    let voice = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        voice.active_section, "Voice",
        "点击 Voice 后应切换到 Voice Settings，snapshot: {:?}",
        voice
    );
    assert_eq!(
        voice.voice_interaction_mode, "continuous_listening",
        "点击 Continuous Listening 后应立即切换新交互模式"
    );
    assert!(
        cx.debug_bounds("settings-asr-model-0").is_some(),
        "Voice Settings 应始终显示第一个受控 ASR 模型"
    );
    assert!(
        cx.debug_bounds("settings-asr-model-1").is_some(),
        "Voice Settings 应显示 Zipformer 作为第二个受控 ASR 模型"
    );
    assert!(
        cx.debug_bounds("settings-agent-provider-openai").is_some(),
        "Voice Settings 应渲染 OpenAI Provider 选择按钮，snapshot: {:?}",
        voice
    );
    click_debug(cx, "settings-agent-provider-openai");
    let openai = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        openai.agent_provider, "openai",
        "点击 OpenAI 后全局 Agent Provider 应立即切换"
    );
    assert_ne!(
        openai.agent_provider, "ollama",
        "OpenAI 被选中后不应继续报告 Ollama"
    );

    click_debug(cx, "settings-close");
    let closed = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(
        !closed.settings_open,
        "after clicking close, the settings panel should be closed, snapshot: {:?}",
        closed
    );
}

/// Voice Provider 三个文本框应通过真实聚焦与平台文本输入保存配置。
///
/// 场景：用户打开 Voice Settings，依次双击 Base URL、Model、API Key 输入框并输入新值。
/// 数据构造：Base URL 使用 `https://api.example.test/v1`，Model 使用 `gpt-e2e-mini`；
/// API Key 使用仅存在于测试内存的 `sk-e2e-secret`，快照只暴露 configured 布尔值。
/// 执行过程：每个字段均从渲染树 debug bounds 双击；Base URL 与 Model 通过
/// `simulate_input` 走当前焦点的 GPUI 文本输入，API Key 通过 Cmd+V 走用户粘贴路径；
/// 每项输入后按 Enter 提交。
/// 预期结果：Base URL 与 Model 精确替换旧值而非追加；API Key 仅报告已配置；三个字段
/// 提交后均退出编辑态，证明可见输入框、焦点、输入路由和提交形成完整闭环。
#[test]
fn e2e_03_voice_provider_fields_accept_text_input() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let _tmp = tempfile::TempDir::new().expect("tempdir for workspace root should create");
    enter_canvas(&app, cx, _tmp.path());

    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-voice");
    click_debug(cx, "settings-voice-mode-continuous");
    scroll_debug(cx, "settings-content-scroll", -650.0);
    open_voice_advanced(cx);

    double_click_debug(cx, "settings-voice-field-agent-base-url");
    cx.simulate_input("https://api.example.test/v1");
    cx.simulate_keystrokes("enter");
    let base_url = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        base_url.agent_base_url, "https://api.example.test/v1",
        "Base URL 输入框应以平台文本输入精确替换旧值，snapshot: {:?}",
        base_url
    );
    assert_eq!(
        base_url.voice_editing_field, None,
        "Base URL 回车提交后应退出编辑态，snapshot: {:?}",
        base_url
    );

    scroll_debug(cx, "settings-content-scroll", 500.0);
    double_click_debug(cx, "settings-voice-field-agent-model");
    cx.simulate_input("gpt-e2e-mini");
    cx.simulate_keystrokes("enter");
    let model = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        model.agent_model, "gpt-e2e-mini",
        "Model 输入框应以平台文本输入精确替换旧值，snapshot: {:?}",
        model
    );
    assert_eq!(
        model.voice_editing_field, None,
        "Model 回车提交后应退出编辑态，snapshot: {:?}",
        model
    );

    scroll_debug(cx, "settings-content-scroll", -500.0);
    double_click_debug(cx, "settings-voice-field-agent-api-key");
    cx.update(|_window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("sk-e2e-secret".to_string()));
    });
    cx.simulate_keystrokes("cmd-v enter");
    cx.run_until_parked();
    let api_key = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(
        api_key.agent_api_key_configured,
        "API Key 输入框粘贴并提交后应仅报告已配置，snapshot: {:?}",
        api_key
    );
    assert_eq!(
        api_key.agent_api_key_display_state,
        app::ApiKeyDisplayState::Configured,
        "API Key 回车提交后应清除明文并显示安全的 configured 状态"
    );
    assert_eq!(
        api_key.voice_editing_field, None,
        "API Key 回车提交后应退出编辑态，snapshot: {:?}",
        api_key
    );
}

/// Voice Provider 的原生文本输入区域必须覆盖可见字段，不能退化成零尺寸 IME 锚点。
///
/// 场景：用户打开 Voice Settings，分别单击 Base URL 与 Model 并输入文本；当前缺陷会让
/// 输入法出现，但非空文本在字段中只渲染为 `...`，看起来像没有输入。
/// 数据构造：输入值使用 `visible-text`；实际 surface 尺寸由 paint 后的 bounds 换算为
/// 整数像素，不写死具体面板宽度，只要求宽高均非零。
/// 执行过程：从真实 debug hitbox 单击 Base URL，检查 paint 后的原生输入区域；再单击
/// Model、通过 `simulate_input` 输入文本，并再次检查其原生输入区域。
/// 预期结果：两个输入区域均非零，Base URL 的 shaped text 等于真实配置值，Model 输入后的
/// shaped text 精确等于 `visible-text` 且不等于 `...`；Model 同时保持编辑态。
#[test]
fn e2e_03_voice_provider_native_input_surfaces_are_visible() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let _tmp = tempfile::TempDir::new().expect("tempdir for workspace root should create");
    enter_canvas(&app, cx, _tmp.path());

    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-voice");
    click_debug(cx, "settings-voice-mode-continuous");
    scroll_debug(cx, "settings-content-scroll", -650.0);
    open_voice_advanced(cx);
    click_debug(cx, "settings-voice-field-agent-base-url");

    let base_url = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    let base_url_size = base_url
        .agent_base_url_input_surface_size
        .expect("Base URL paint 后应记录原生输入 surface 尺寸");
    assert!(
        base_url_size.0 > 0 && base_url_size.1 > 0,
        "Base URL 的原生输入区域宽高应非零，actual={base_url_size:?}, snapshot: {:?}",
        base_url
    );
    assert_eq!(
        base_url.agent_base_url_rendered_text,
        Some(base_url.agent_base_url.clone()),
        "Base URL 实际 shaped text 应等于配置值，不能只剩省略号，snapshot: {:?}",
        base_url
    );
    assert_ne!(
        base_url.agent_base_url_rendered_text,
        Some("...".to_string()),
        "Base URL 不得继续渲染为旧版省略号，snapshot: {:?}",
        base_url
    );

    scroll_debug(cx, "settings-content-scroll", 500.0);
    click_debug(cx, "settings-voice-field-agent-model");
    cx.simulate_input("visible-text");
    let model = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    let model_size = model
        .agent_model_input_surface_size
        .expect("Model paint 后应记录原生输入 surface 尺寸");
    assert!(
        model_size.0 > 0 && model_size.1 > 0,
        "Model 的原生输入区域宽高应非零，actual={model_size:?}, snapshot: {:?}",
        model
    );
    assert_eq!(
        model.agent_model_rendered_text,
        Some("visible-text".to_string()),
        "Model 输入后的实际 shaped text 应精确显示新内容，snapshot: {:?}",
        model
    );
    assert_ne!(
        model.agent_model_rendered_text,
        Some("...".to_string()),
        "Model 输入后不得继续渲染为旧版省略号，snapshot: {:?}",
        model
    );
    assert_eq!(
        model.voice_editing_field,
        Some("agent_model"),
        "输入文本后 Model 应继续处于编辑态，snapshot: {:?}",
        model
    );
}

/// Voice Provider 字段单击后应立即进入编辑态并接受平台文本输入。
///
/// 场景：用户打开 Voice Settings，单击 Model 值区域，直接输入新模型并回车提交。
/// 数据构造：输入值为 `single-click-model`，用于区分单击是否真的进入编辑态。
/// 执行过程：通过真实 debug hitbox 单击字段，调用 GPUI `simulate_input`，再发送 Enter。
/// 预期结果：Model 精确替换为新值，且提交后退出编辑态；如果字段仍要求双击，本测试
/// 会在输入前后的状态断言处失败。
#[test]
fn e2e_03_voice_provider_field_accepts_text_after_single_click() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let _tmp = tempfile::TempDir::new().expect("tempdir for workspace root should create");
    enter_canvas(&app, cx, _tmp.path());

    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-voice");
    click_debug(cx, "settings-voice-mode-continuous");
    scroll_debug(cx, "settings-content-scroll", -650.0);
    click_debug(cx, "settings-voice-field-agent-model");

    let editing = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        editing.voice_editing_field,
        Some("agent_model"),
        "单击 Model 值区域应立即进入编辑态，snapshot: {:?}",
        editing
    );

    cx.simulate_input("single-click-model");
    cx.simulate_keystrokes("enter");
    let committed = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        committed.agent_model, "single-click-model",
        "单击后平台文本输入应替换 Model 值，snapshot: {:?}",
        committed
    );
    assert_eq!(
        committed.voice_editing_field, None,
        "单击输入回车提交后应退出编辑态，snapshot: {:?}",
        committed
    );
}

/// Voice Provider 文本框必须支持平台级全选后替换，而不是把粘贴内容追加到草稿。
///
/// 场景：用户单击 Model 文本框，先输入一段模型名，再用 Cmd+A 全选并粘贴新模型名。
/// 数据构造：初次输入 `first-model`，替换文本为 `replacement-model`；两者不同且后者
/// 不包含前者，可明确区分“全选替换”和“末尾追加”。
/// 执行过程：通过真实 debug hitbox 单击 Model → `simulate_input` 输入首值 → Cmd+A →
/// 写入测试剪贴板 → Cmd+V → Enter；全程走当前聚焦 GPUI 文本输入与快捷键链路。
/// 预期结果：最终模型精确等于 `replacement-model`，不得残留 `first-model`；提交后退出
/// 编辑态，证明文本选择、粘贴、提交和 Settings 状态形成完整闭环。
#[test]
fn e2e_03_voice_provider_text_input_select_all_replaces_content() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let _tmp = tempfile::TempDir::new().expect("tempdir for workspace root should create");
    enter_canvas(&app, cx, _tmp.path());

    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-voice");
    click_debug(cx, "settings-voice-mode-continuous");
    scroll_debug(cx, "settings-content-scroll", -650.0);
    click_debug(cx, "settings-voice-field-agent-model");

    cx.simulate_input("first-model");
    cx.simulate_keystrokes("cmd-a");
    cx.update(|_window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("replacement-model".to_string()));
    });
    cx.simulate_keystrokes("cmd-v enter");

    let committed = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        committed.agent_model, "replacement-model",
        "Cmd+A 后粘贴应替换完整 Model，而不是追加旧文本，snapshot: {:?}",
        committed
    );
    assert!(
        !committed.agent_model.contains("first-model"),
        "全选替换后不得残留首段 Model 文本，snapshot: {:?}",
        committed
    );
    assert_eq!(
        committed.voice_editing_field, None,
        "Model 回车提交后应退出编辑态，snapshot: {:?}",
        committed
    );
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    let position = move_to_debug_target(cx, selector);
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
}

fn move_to_debug_target(cx: &mut VisualTestContext, selector: &'static str) -> Point<Pixels> {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("selector `{selector}` should have debug bounds before click"));
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    bounds.center()
}

fn double_click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    let position = move_to_debug_target(cx, selector);
    cx.simulate_event(MouseDownEvent {
        position,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        position,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 2,
    });
    cx.run_until_parked();
}

fn scroll_debug(cx: &mut VisualTestContext, selector: &'static str, delta_y: f32) {
    let position = move_to_debug_target(cx, selector);
    cx.simulate_event(ScrollWheelEvent {
        position,
        delta: ScrollDelta::Pixels(point(px(0.0), px(delta_y))),
        ..Default::default()
    });
    cx.run_until_parked();
}

fn open_voice_advanced(cx: &mut VisualTestContext) {
    click_debug(cx, "settings-voice-advanced-toggle");
    scroll_debug(cx, "settings-content-scroll", -500.0);
}
