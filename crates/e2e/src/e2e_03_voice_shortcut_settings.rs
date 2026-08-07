//! E2E-03：Voice Interaction 快捷键可在 Settings → Keyboard Shortcuts 中录制与恢复。

use gpui::{
    point, px, Modifiers, Pixels, Point, ScrollDelta, ScrollWheelEvent, TestAppContext,
    VisualTestContext,
};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// Voice Interaction 快捷键：Settings 中可录制新键位并恢复默认 Cmd+E。
///
/// 场景：用户打开 Keyboard Shortcuts，滚动到 Voice 分组，点击 Voice Interaction
/// keycap 录制 `cmd-alt-e`，再点击 Restore 恢复默认。
///
/// 数据构造（含关键数值推导过程）：
///   Voice Interaction 默认快捷键 = `cmd-e`
///   测试覆盖快捷键              = `cmd-alt-e`（GPUI 规范化后为 `alt-cmd-e`）
///   内容区滚动偏移              = -900px（露出页面底部 Voice 分组）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开 Settings → Keyboard Shortcuts。
///   2. 滚动内容区以露出 `settings-shortcut-voice-interaction`。
///   3. 点击 keycap 开始录制，输入 `cmd-alt-e` 保存。
///   4. 点击 Restore 恢复默认 `cmd-e`。
///
/// 预期结果：
///   - 正断言：录制后 shortcut=`alt-cmd-e` 且 modified=true。
///   - 正断言：Restore 后 shortcut=`cmd-e` 且 modified=false。
///   - 负断言：录制完成后 recording_command 必须为 None。
#[test]
fn e2e_03_voice_interaction_shortcut_can_be_changed_and_restored() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let _tmp = tempfile::TempDir::new().expect("tempdir for workspace root should create");
    enter_canvas(&app, cx, _tmp.path());

    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-keyboard");
    scroll_debug(cx, "settings-content-scroll", -900.0);
    assert!(
        cx.debug_bounds("settings-shortcut-voice-interaction")
            .is_some(),
        "Keyboard Shortcuts 应渲染可编辑的 Voice Interaction keycap"
    );

    let initial = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        initial.voice_interaction_shortcut.as_deref(),
        Some("cmd-e"),
        "Voice Interaction 默认应显示 cmd-e，snapshot: {initial:?}"
    );
    assert!(
        !initial.voice_interaction_modified,
        "Voice Interaction 初始不应处于 modified，snapshot: {initial:?}"
    );

    click_debug(cx, "settings-shortcut-voice-interaction");
    let recording = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        recording.recording_command,
        Some("voice_interaction"),
        "点击 Voice Interaction keycap 应开始录制，snapshot: {recording:?}"
    );
    cx.simulate_keystrokes("cmd-alt-e");
    let modified = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        modified.recording_command, None,
        "接受 cmd-alt-e 后录制态应清除，snapshot: {modified:?}"
    );
    assert_eq!(
        modified.voice_interaction_shortcut.as_deref(),
        Some("alt-cmd-e"),
        "Voice Interaction 应保存为 GPUI 规范化后的 alt-cmd-e，snapshot: {modified:?}"
    );
    assert!(
        modified.voice_interaction_modified,
        "改键后 Voice Interaction 应标记 modified，snapshot: {modified:?}"
    );

    click_debug(cx, "settings-shortcut-restore-voice-interaction");
    let restored = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        restored.voice_interaction_shortcut.as_deref(),
        Some("cmd-e"),
        "Restore 后 Voice Interaction 应回到 cmd-e，snapshot: {restored:?}"
    );
    assert!(
        !restored.voice_interaction_modified,
        "Restore 后 Voice Interaction 不应再标记 modified，snapshot: {restored:?}"
    );
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    let position = move_to_debug_target(cx, selector);
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
}

fn move_to_debug_target(cx: &mut VisualTestContext, selector: &'static str) -> Point<Pixels> {
    cx.run_until_parked();
    scroll_settings_target_into_view(cx, selector);
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("selector `{selector}` should have debug bounds before click"));
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    bounds.center()
}

fn scroll_settings_target_into_view(cx: &mut VisualTestContext, selector: &'static str) {
    if selector == "settings-content-scroll" {
        return;
    }
    for _ in 0..3 {
        let (Some(content), Some(target)) = (
            cx.debug_bounds("settings-content-scroll"),
            cx.debug_bounds(selector),
        ) else {
            return;
        };
        let visible_top = f32::from(content.origin.y) + 24.0;
        let visible_bottom = f32::from(content.origin.y) + f32::from(content.size.height) - 24.0;
        let target_top = f32::from(target.origin.y);
        let target_bottom = target_top + f32::from(target.size.height);
        let delta = if target_top < visible_top {
            visible_top - target_top
        } else if target_bottom > visible_bottom {
            visible_bottom - target_bottom
        } else {
            return;
        };
        cx.simulate_event(ScrollWheelEvent {
            position: content.center(),
            delta: ScrollDelta::Pixels(point(px(0.0), px(delta))),
            ..Default::default()
        });
        cx.run_until_parked();
    }
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
