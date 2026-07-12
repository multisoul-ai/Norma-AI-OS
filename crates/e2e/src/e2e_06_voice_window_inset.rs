use audio::{PcmAudioChunk, PcmFormat};
use gpui::TestAppContext;
use std::time::Duration;

use crate::harness::{echo_factory, enter_canvas, make_app};

fn pcm16_mono_chunk(amplitude: i16, duration: Duration) -> PcmAudioChunk {
    let frame_count = (duration.as_secs_f64() * 16_000.0) as usize;
    let bytes = std::iter::repeat_n(amplitude.to_le_bytes(), frame_count)
        .flatten()
        .collect();
    PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, bytes)
        .expect("e2e 注入的 PCM 必须是合法 16kHz mono PCM16")
}

/// E2E-06：AI 助手实际播放时，连续窗口内缘光应随 PCM 能量增强且在 Settings 下层保持可见。
///
/// 数据构造（含关键数值推导）：headless Canvas 窗口注入一次助手播放开始；
///   淡入时长为 120ms；16kHz PCM 的 120ms 静音 = 16_000 * 120 / 1_000 = 1_920 frames；
///   后续 128ms 满幅 PCM = 16_000 * 128 / 1_000 = 2_048 frames，amplitude = 32_767，
///   即 128ms / 16ms = 8 个能量窗口；连续消费前 7 个窗口 = 112ms，仍保留一个高能量窗口。
///   静态底光为 12% / 30px，响亮 PCM 的产品峰值上限为 55% / 38px，正常收束为 220ms。
///   峰值共享斜边的两段 50% 抗锯齿覆盖会从 55% 降至 47.4375%，故补偿量必须大于 0 且小于完整边。
///
/// 执行过程：
///   1. 进入 Canvas，注入播放开始和 120ms 静音 PCM，再推进 120ms 使底光完成淡入。
///   2. 注入 128ms 满幅 PCM 并连续推进 7 个 16ms 动画帧，读取响亮语音时的可视快照。
///   3. 打开 Settings，验证增强后的内缘光仍在根级渲染树且位于 Settings 下层。
///   4. 完成播放并推进 220ms，验证内缘光收束消失。
///
/// 预期结果：
///   - 正断言：120ms 后白光激活、透明度为 12%、总内扩为 30px。
///   - 正断言：满幅 PCM 会把透明度与内扩提升到静态底光以上。
///   - 正断言：满幅 PCM 时角部抗锯齿补偿必须激活，但弱于完整边，避免暗缝或亮线。
///   - 正断言：Settings 打开后白光仍存在，但 Settings 面板正常显示。
///   - 负断言：响亮 PCM 不得超过 55% / 38px；正常结束 220ms 后透明度、内扩和角部补偿都归零。
#[test]
fn e2e_06_voice_window_inset_tracks_pcm_energy_without_obscuring_settings() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("e2e workspace tempdir should create");
    enter_canvas(&app, cx, workspace.path());

    app.update(cx, |app, cx| {
        app.begin_assistant_playback_for_e2e(cx);
        app.queue_assistant_pcm_for_e2e(pcm16_mono_chunk(0, Duration::from_millis(120)), cx);
        app.advance_animations_for_e2e(Duration::from_millis(120), cx);
    });
    let visible = app.read_with(cx, |app, _cx| app.voice_window_inset_e2e_snapshot());
    assert!(
        visible.active,
        "播放开始并淡入 120ms 后窗口内缘光应处于激活状态"
    );
    assert_eq!(visible.opacity_milli, 120, "安静底光透明度应为 12%");
    assert_eq!(visible.inset_px_milli, 30_000, "安静底光向内延展应为 30px");
    assert!(
        cx.debug_bounds("voice-window-inset").is_some(),
        "激活后根级渲染树必须包含非交互的窗口内缘光叠层"
    );

    app.update(cx, |app, cx| {
        app.queue_assistant_pcm_for_e2e(pcm16_mono_chunk(i16::MAX, Duration::from_millis(128)), cx);
        for _ in 0..7 {
            app.advance_animations_for_e2e(Duration::from_millis(16), cx);
        }
    });
    let loud = app.read_with(cx, |app, _cx| app.voice_window_inset_e2e_snapshot());
    assert!(
        loud.opacity_milli > visible.opacity_milli,
        "满幅 PCM 应使透明度高于 12% 底光，visible={visible:?}, loud={loud:?}"
    );
    assert!(
        loud.inset_px_milli > visible.inset_px_milli,
        "满幅 PCM 应使内扩高于 30px 底光，visible={visible:?}, loud={loud:?}"
    );
    assert!(
        loud.opacity_milli <= 550,
        "满幅 PCM 不得超过 55% 峰值透明度，loud={loud:?}"
    );
    assert!(
        loud.inset_px_milli <= 38_000,
        "满幅 PCM 不得超过 38px 峰值内扩，loud={loud:?}"
    );
    assert!(
        loud.corner_seam_guard_alpha_milli > 0,
        "满幅 PCM 时四角共享斜边必须激活抗锯齿补偿，避免出现未闭合暗缝；loud={loud:?}"
    );
    assert!(
        loud.corner_seam_guard_alpha_milli < loud.opacity_milli,
        "角部补偿必须弱于完整边，避免把共享斜边补成额外亮线；loud={loud:?}"
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
    assert_eq!(
        finished.corner_seam_guard_alpha_milli, 0,
        "播放结束后不得残留角部补偿，以免形成静止的斜线伪影"
    );
}
