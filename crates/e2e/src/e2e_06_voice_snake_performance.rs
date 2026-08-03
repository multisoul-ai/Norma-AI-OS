use audio::{PcmAudioChunk, PcmFormat};
use gpui::TestAppContext;
use runtime::{AgentTaskState, AgentTaskUpdate};
use std::time::Duration;

use crate::harness::{
    create_codex_agent, create_terminal, echo_factory, enter_canvas, make_app, read_runtime,
};

fn pcm16_mono_chunk(amplitude: i16, duration: Duration) -> PcmAudioChunk {
    let frame_count = (duration.as_secs_f64() * 16_000.0) as usize;
    let bytes = std::iter::repeat_n(amplitude.to_le_bytes(), frame_count)
        .flatten()
        .collect();
    PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, bytes)
        .expect("性能测试注入的 PCM 必须是合法 16kHz mono PCM16")
}

/// E2E-06 性能回归：持续 Voice 内缘光不得把六卡 Working Snake 的根 Canvas 提升到 60fps。
///
/// 数据构造（含关键数值的推导过程）：
///   cards                  = 5 Terminal + 1 Codex Agent = 6
///   initialization settle  = 1_000ms > 卡片初始化总时长 992ms
///   snake fade settle       = 150ms
///   voice PCM               = 16_000Hz × 2_000ms = 32_000 frames
///   PCM bytes               = 32_000 frames × 2 bytes = 64_000 bytes
///   scheduler warmup        = 42 × 12ms = 504ms > idle 250ms，也 > fade-in 120ms
///   measurement window      = 83 × 12ms + 4ms = 1_000ms
///   ambient interval        = 67ms → floor(1_000 / 67) = 14 个 cadence 边界，验收上限 15
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建五张 Terminal 和一张 Codex Agent，推进 1_000ms 排除初始化动画。
///   2. 注入 Thinking 并推进 150ms，使 Working Snake 进入稳定 ambient 阶段。
///   3. 开始助手播报并排入 2_000ms PCM，分步预热 504ms，使 Voice 内缘光持续活动。
///   4. 清零真实 Canvas render 计数，再分步推进 1_000ms mock clock。
///   5. 读取根动画调度、Voice 快照和 Canvas render 计数。
///
/// 预期结果：
///   - 正断言：场景精确包含六张卡片，Working Snake 与 Voice 内缘光均保持活动。
///   - 正断言：根调度保持 67ms ambient timer，Canvas 在 1 秒内至少重绘 1 次且至多 15 次。
///   - 负断言：Voice 内缘光不得令根调度请求逐显示帧，也不得把 Canvas 提升到约 60fps。
#[test]
fn e2e_06_voice_inset_does_not_raise_six_card_working_canvas_above_fifteen_fps() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Voice + Snake 性能测试应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    for _ in 0..5 {
        create_terminal(&app, cx);
    }
    let node_id = create_codex_agent(&app, cx);

    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(1_000), cx);
        let changed = app.runtime.apply_agent_task_update(AgentTaskUpdate {
            node_id,
            profile: canvas::AgentProfile::Codex,
            provider_event: "UserPromptSubmit".to_string(),
            to: AgentTaskState::Thinking,
            task_title: None,
            session_id: None,
        });
        assert!(
            changed,
            "Voice + Snake 场景的 Thinking 更新应写入 Codex Agent"
        );
        cx.notify();
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(150), cx);
        app.begin_assistant_playback_for_e2e(cx);
        app.queue_assistant_pcm_for_e2e(pcm16_mono_chunk(8_000, Duration::from_millis(2_000)), cx);
    });
    cx.run_until_parked();

    // warmup = 42 × 12ms = 504ms，覆盖 250ms idle 旧 timer 和 120ms Voice 淡入。
    for _ in 0..42 {
        cx.cx
            .background_executor
            .advance_clock(Duration::from_millis(12));
        app.update(cx, |app, cx| {
            let _ = app
                .consume_voice_window_inset_display_callback_for_e2e(Duration::from_millis(12), cx);
        });
        cx.run_until_parked();
    }
    app.update(cx, |app, _cx| app.reset_canvas_render_count_for_e2e());

    // measurement = 83 × 12ms + 4ms = 1_000ms；分步推进才能逐次触发 mock timer。
    for _ in 0..83 {
        cx.cx
            .background_executor
            .advance_clock(Duration::from_millis(12));
        app.update(cx, |app, cx| {
            let _ = app
                .consume_voice_window_inset_display_callback_for_e2e(Duration::from_millis(12), cx);
        });
        cx.run_until_parked();
    }
    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(4));
    cx.run_until_parked();

    let card_count = read_runtime(&app, cx, |runtime| runtime.document.nodes().len());
    let schedule = app.read_with(cx, |app, _cx| app.animation_schedule_snapshot_for_e2e());
    let voice = app.read_with(cx, |app, _cx| app.voice_window_inset_e2e_snapshot());
    let canvas_render_count = app.read_with(cx, |app, _cx| app.canvas_render_count_for_e2e());

    assert_eq!(
        card_count, 6,
        "性能回归场景必须精确包含 5 Terminal + 1 Agent = 6 张卡片"
    );
    assert!(
        schedule.canvas_ambient_active,
        "Voice 播报期间稳定 Working Snake 必须继续报告 ambient 活动"
    );
    assert!(
        voice.active,
        "2_000ms PCM 的测量窗口内 Voice 内缘光必须保持可见"
    );
    assert!(
        !schedule.next_frame_requested,
        "Voice 内缘光必须脱离根动画调度，不能令整个 Canvas 请求逐显示帧"
    );
    assert_eq!(
        schedule.timer_interval,
        Some(Duration::from_millis(67)),
        "根 Canvas 应保持 ceil(1_000ms / 15fps) = 67ms ambient timer"
    );
    assert!(
        canvas_render_count >= 1,
        "1_000ms 测量窗口内 Working Snake 至少应触发一次 Canvas 重绘，不能冻结"
    );
    assert!(
        canvas_render_count <= 15,
        "Voice + Snake 同时运行时根 Canvas 重绘必须 <= 15 次，实际为 {canvas_render_count}"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}
