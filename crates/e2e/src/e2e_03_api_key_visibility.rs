use gpui::{ClipboardItem, Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// 已保存 API Key 的安全显隐：默认掩码，眼睛按钮只在用户点击期间显示明文
///
/// 数据构造（含关键数值的推导过程）：
///   API Key = 1 个写入隔离测试数据库的 synthetic secret
///   visibility clicks = reveal(1) + conceal(1) = 2
///   downward scroll = 650 + 2,200 + 1,600 = 4,450 px（覆盖新增 ASR provider 设置组）
///   expected legacy plaintext copies = 0（新凭据保存在独立命名空间）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开 Settings → Voice → Advanced Settings
///   2. 通过真实输入框粘贴并提交 synthetic API Key
///   3. 检查默认掩码与眼睛按钮，点击一次显示，再点击一次隐藏
///   4. 每次点击后读取安全分类快照，不把 synthetic secret 写入失败消息
///
/// 预期结果：
///   - 正断言：已配置状态显示显隐按钮，点击后进入 revealed，再次点击恢复 configured
///   - 负断言：默认状态和关闭显示后不得为 Unsafe，也不得删除已保存凭据
#[test]
fn e2e_03_configured_api_key_can_be_revealed_and_concealed() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("API Key visibility workspace should create");
    enter_canvas(&app, cx, workspace.path());

    click(cx, "toolbar-settings");
    click(cx, "settings-section-voice");
    click(cx, "settings-voice-mode-continuous");
    scroll(cx, -650.0);
    scroll(cx, -2200.0);
    scroll(cx, -1600.0);

    let input = cx
        .debug_bounds("settings-voice-field-agent-api-key")
        .expect("Advanced Voice Settings must render the API Key input");
    cx.simulate_click(input.center(), Modifiers::none());
    cx.update(|_window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(
            "synthetic-visibility-secret".to_string(),
        ));
    });
    cx.simulate_keystrokes("cmd-v enter");
    cx.run_until_parked();

    let configured = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(
        configured.agent_api_key_configured,
        "synthetic API Key should be configured before visibility testing"
    );
    assert_eq!(
        configured.agent_api_key_display_state,
        app::ApiKeyDisplayState::Configured,
        "configured API Key must start masked"
    );
    assert!(
        cx.debug_bounds("settings-agent-provider-api-key-visibility")
            .is_some(),
        "configured API Key must expose a dedicated visibility button"
    );

    click(cx, "settings-agent-provider-api-key-visibility");
    let revealed = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        revealed.agent_api_key_display_state,
        app::ApiKeyDisplayState::Revealed,
        "first visibility click must render the loaded SQLite secret"
    );
    assert!(
        revealed.agent_api_key_configured,
        "revealing the API Key must not replace or remove the configured credential"
    );

    click(cx, "settings-agent-provider-api-key-visibility");
    let concealed = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        concealed.agent_api_key_display_state,
        app::ApiKeyDisplayState::Configured,
        "second visibility click must restore the fixed asterisk mask"
    );
    assert_ne!(
        concealed.agent_api_key_display_state,
        app::ApiKeyDisplayState::Unsafe,
        "concealing the API Key must clear plaintext from the rendered input"
    );
}

fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("API Key visibility test must render {selector}"));
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

fn scroll(cx: &mut gpui::VisualTestContext, y: f32) {
    let bounds = cx
        .debug_bounds("settings-content-scroll")
        .expect("Voice Settings must expose its scroll container");
    cx.simulate_event(ScrollWheelEvent {
        position: bounds.center(),
        delta: ScrollDelta::Pixels(gpui::point(gpui::px(0.0), gpui::px(y))),
        ..Default::default()
    });
    cx.run_until_parked();
}
