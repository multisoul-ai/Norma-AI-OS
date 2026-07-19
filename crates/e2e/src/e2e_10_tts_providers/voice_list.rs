use super::*;

/// E2E-TTS-VOICE-LIST-1：大型 MiniMax 音色目录必须在固定 viewport 内虚拟滚动。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog Voices = 120（全部为 System，稳定覆盖远大于首屏容量的目录）
///   viewport height ≤ 360 px（约 5 × 64 px Voice 行 + 上下留白）
///   initial visible tail rows = 0（索引 119 不得出现在首屏 viewport）
///   scroll delta = -20,000 px（120 × 64 = 7,680 px，足以滚到末尾）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 注入 120 条账户音色，打开 Voice → Spoken Replies → MiniMax。
///   2. 读取音色列表 viewport，并检查首行与末行的初始渲染状态。
///   3. 在列表内部向下滚动 20,000 px，等待虚拟列表重绘可见区间。
///   4. 再次检查末行与列表 viewport。
///
/// 预期结果：
///   - 正断言：列表高度有上界，首屏存在第 0 行，滚到底后第 119 行进入 viewport。
///   - 负断言：第 119 行不得出现在首屏；内部滚动不得带动外层 Settings 页面。
#[test]
fn e2e_minimax_large_voice_catalog_uses_bounded_virtual_scroll() {
    let _tts_guard = VoiceModelInstallE2eGuard::acquire();
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::tempdir().expect("应创建大型 MiniMax 目录 e2e workspace");
    app.update(cx, |app, cx| {
        app.assume_minimax_connected_with_system_voices_for_e2e(120, cx)
    });
    enter_canvas(&app, cx, workspace.path());
    cx.simulate_keystrokes("cmd-,");
    click(cx, "settings-section-voice");
    click(cx, "voice-ready-change-tts");
    click(cx, "settings-tts-picker-provider-minimax");
    scroll_and_assert_visible(cx, "settings-tts-minimax-voice-list");

    let viewport = cx
        .debug_bounds("settings-tts-minimax-voice-list")
        .expect("大型音色目录必须渲染独立列表 viewport");
    assert!(
        f32::from(viewport.size.height) <= 360.0,
        "音色列表 viewport 高度不得超过 360px，实际为 {:?}",
        viewport.size.height
    );
    assert!(
        cx.debug_bounds("settings-tts-minimax-voice-0-0").is_some(),
        "大型目录首屏必须渲染第 0 条 System Voice"
    );
    assert!(
        cx.debug_bounds("settings-tts-minimax-voice-0-119")
            .is_none(),
        "大型目录首屏不得预渲染第 119 条 System Voice"
    );

    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), px(-20_000.0))),
        ..Default::default()
    });
    cx.run_until_parked();

    let tail = cx
        .debug_bounds("settings-tts-minimax-voice-0-119")
        .expect("滚到底后虚拟列表必须渲染第 119 条 System Voice");
    assert!(
        tail.top() >= viewport.top() && tail.bottom() <= viewport.bottom(),
        "滚到底后第 119 条 System Voice 必须位于列表 viewport 内；tail={tail:?}, viewport={viewport:?}"
    );
    assert_eq!(
        cx.debug_bounds("settings-tts-minimax-voice-list"),
        Some(viewport),
        "内部滚动前后音色列表 viewport 必须保持固定，不得拉长 Settings 页面"
    );
}
