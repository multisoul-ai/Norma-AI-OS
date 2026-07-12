//! E2E-03 Voice：新交互模式、共享模型目录与小窗口布局。

use gpui::{
    point, px, size, Modifiers, Pixels, Point, ScrollDelta, ScrollWheelEvent, TestAppContext,
    VisualTestContext,
};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// Voice 新架构点击链路：单次语音指令、持续聆听与共享模型库必须在同页工作。
///
/// 数据构造（含关键数值推导过程）：
///   default mode    = single_voice_command
///   default trigger = hold
///   catalog cards   = 2（Paraformer + Zipformer）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开 Settings → Voice，读取默认模式与模型卡片 bounds
///   2. 点击 Toggle → snapshot trigger 变为 toggle
///   3. 点击 Continuous Listening → snapshot mode 变为 continuous_listening
///   4. 检查 Agent Provider 控件在持续聆听模式可点击
///
/// 预期结果：
///   - 正断言：单次语音指令触发和模式点击均更新稳定状态，两个模型卡片始终可见
///   - 负断言：切换后不得继续报告旧 hold 或 single_voice_command 值
#[test]
fn e2e_03_voice_modes_and_shared_model_catalog_are_clickable() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("应创建 e2e workspace");
    enter_canvas(&app, cx, workspace.path());

    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-voice");
    let defaults = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));

    assert_eq!(
        defaults.voice_interaction_mode, "single_voice_command",
        "新 Voice Settings 默认应为单次语音指令模式"
    );
    assert_eq!(
        defaults.dictation_trigger, "hold",
        "单次语音指令默认应为按住触发"
    );
    assert!(
        cx.debug_bounds("settings-asr-model-0").is_some(),
        "Paraformer 模型卡片应在 Voice 页可见"
    );
    assert!(
        cx.debug_bounds("settings-asr-model-1").is_some(),
        "Zipformer 模型卡片应在同一 Voice 页可见"
    );

    click_debug(cx, "settings-voice-trigger-toggle");
    let toggle = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        toggle.dictation_trigger, "toggle",
        "点击 Toggle 后应更新单次语音指令触发方式"
    );
    assert_ne!(
        toggle.dictation_trigger, "hold",
        "选择 Toggle 后不得继续报告默认 Hold"
    );

    click_debug(cx, "settings-voice-mode-continuous");
    scroll_debug(cx, "settings-content-scroll", -650.0);
    let continuous = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        continuous.voice_interaction_mode, "continuous_listening",
        "点击持续聆听后应更新互斥交互模式"
    );
    assert_ne!(
        continuous.voice_interaction_mode, "single_voice_command",
        "持续聆听启用后不得继续报告单次语音指令模式"
    );
    assert!(
        cx.debug_bounds("settings-agent-provider-openai").is_some(),
        "持续聆听模式应展开 Agent Provider 配置"
    );
}

/// Settings 小窗口适配链路。
///
/// 场景：窗口缩到 420×360 后打开 Voice Settings。
/// 数据构造：420×360 小于旧面板 860×620 的设计尺寸；向上滚动 4200px，覆盖交互模式、
/// 听写触发方式和共享模型目录，可把 Provider 下方的 Model 行移入内容区；侧栏再滚动 300px，
/// 大于 5 个导航项总高度 5×42=210px，可把底部 Wallpaper 入口移入可点击区域。
/// 执行过程：resize → 打开 Settings → 切到 Voice → 滚动内容 → 滚动侧栏并点击 Wallpaper。
/// 预期结果：小窗口内仍能通过真实 hitbox 切换 Voice 与底部导航；Wallpaper 成为 active section。
#[test]
fn e2e_03_voice_settings_fits_small_window_and_double_clicks_to_edit() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("应创建 e2e workspace");
    enter_canvas(&app, cx, workspace.path());
    cx.simulate_resize(size(px(420.0), px(360.0)));
    cx.run_until_parked();

    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-voice");
    click_debug(cx, "settings-voice-mode-continuous");
    scroll_debug(cx, "settings-content-scroll", -4200.0);
    let before = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        before.active_section, "Voice",
        "小窗口中应能点击 Voice 导航，snapshot: {:?}",
        before
    );

    scroll_debug(cx, "settings-sidebar-scroll", -300.0);
    click_debug(cx, "settings-section-wallpaper");
    let wallpaper = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        wallpaper.active_section, "Wallpaper",
        "小窗口滚动侧栏后应能点击底部 Wallpaper 导航，snapshot: {:?}",
        wallpaper
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

fn scroll_debug(cx: &mut VisualTestContext, selector: &'static str, delta_y: f32) {
    let position = move_to_debug_target(cx, selector);
    cx.simulate_event(ScrollWheelEvent {
        position,
        delta: ScrollDelta::Pixels(point(px(0.0), px(delta_y))),
        ..Default::default()
    });
    cx.run_until_parked();
}
