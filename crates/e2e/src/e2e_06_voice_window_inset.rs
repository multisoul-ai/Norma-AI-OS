use gpui::TestAppContext;
use std::time::Duration;

use crate::harness::{echo_factory, enter_canvas, make_app};

/// E2E-06：AI 助手开始实际播放后，窗口内缘光应覆盖工作区并在 Settings 打开时保持于其下层。
///
/// 数据构造（含关键数值推导）：headless Canvas 窗口注入一次助手播放开始；
///   淡入时长为 120ms，静态底光透明度为 12%，向内延展为 40px，正常收束为 220ms。
///
/// 执行过程：
///   1. 进入 Canvas，注入播放开始并推进 120ms，使底光完成淡入。
///   2. 打开 Settings，验证底光仍在根级渲染树且位于 Settings 下层。
///   3. 完成播放并推进 220ms，验证底光收束消失。
///
/// 预期结果：
///   - 正断言：120ms 后白光激活、透明度为 12%、总内扩为 40px。
///   - 正断言：Settings 打开后白光仍存在，但 Settings 面板正常显示。
///   - 负断言：正常结束 220ms 后白光透明度和内扩都归零。
#[test]
fn e2e_06_voice_window_inset_tracks_playback_without_obscuring_settings() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("e2e workspace tempdir should create");
    enter_canvas(&app, cx, workspace.path());

    app.update(cx, |app, cx| {
        app.begin_assistant_playback_for_e2e(cx);
        app.advance_animations_for_e2e(Duration::from_millis(120), cx);
    });
    let visible = app.read_with(cx, |app, _cx| app.voice_window_inset_e2e_snapshot());
    assert!(
        visible.active,
        "播放开始并淡入 120ms 后窗口内缘光应处于激活状态"
    );
    assert_eq!(visible.opacity_milli, 120, "安静底光透明度应为 12%");
    assert_eq!(visible.inset_px_milli, 40_000, "安静底光向内延展应为 40px");
    assert!(
        cx.debug_bounds("voice-window-inset").is_some(),
        "激活后根级渲染树必须包含非交互的窗口内缘光叠层"
    );

    let toolbar = cx
        .debug_bounds("toolbar-settings")
        .expect("Canvas 顶部工具栏应提供 Settings 点击目标");
    cx.simulate_click(toolbar.center(), Default::default());
    assert!(
        cx.debug_bounds("settings-panel").is_some(),
        "打开 Settings 后应显示设置面板"
    );
    assert!(
        cx.debug_bounds("voice-window-inset").is_some(),
        "Settings 打开时窗口内缘光应仍存在于根级渲染树，供其作为面板下层反馈"
    );

    app.update(cx, |app, cx| {
        app.finish_assistant_playback_for_e2e(cx);
        app.advance_animations_for_e2e(Duration::from_millis(220), cx);
    });
    let finished = app.read_with(cx, |app, _cx| app.voice_window_inset_e2e_snapshot());
    assert!(
        !finished.active,
        "正常播放完成并收束 220ms 后窗口内缘光必须消失"
    );
    assert_eq!(finished.opacity_milli, 0, "结束后不应残留白光透明度");
    assert_eq!(finished.inset_px_milli, 0, "结束后不应残留向内延展距离");
}
