use gpui::{Modifiers, TestAppContext};
use std::time::Duration;

use crate::harness::{echo_factory, enter_canvas, make_app};

/// Voice 长转写：Dock 保持紧凑，字幕条保留全文，并可展开查看完整内容。
///
/// 数据构造（含关键数值的推导过程）：
///   long transcript = 132 个中文/标点字符（超过单行字幕 72 字符的预览预算 60 字符）
///   collapsed caption max height = 56px（目标条高 48px，额外预留 8px 渲染容差）
///   compact Voice max width = 180px（图标约 20 + 间距 8 + Listening/快捷键约 110 + 内边距 26）
///   text left inset = 0px（首字必须从文本区域边缘开始，不能被渐隐层遮挡）
///   disclosure clicks = expand(1) + collapse(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas，并通过公开 E2E seam 注入一段正在识别的长中文 partial。
///   2. 读取 Voice 快照和真实渲染树，检查 Dock、折叠字幕条及文本起点。
///   3. 点击 `voice-caption-expand`，再次读取快照和字幕条边界。
///   4. 再次点击同一语义按钮，确认「收起」可以收回字幕条。
///
/// 预期结果：
///   - 正断言：Dock 显示 `Listening` 与 `⌘E` 快捷键，且 Voice 控件保持紧凑。
///   - 正断言：字幕条保存完整原文，折叠态显示结尾预览、首字未遮挡并保持单行低高度。
///   - 正断言：点击后进入全文展开态，字幕容器高度增加，再次点击可收回。
///   - 负断言：Dock 不得包含长转写，折叠预览不得冒充完整原文、只显示开头或遮挡首字。
#[test]
fn long_voice_transcript_uses_expandable_caption_without_growing_dock() {
    let transcript = "请帮我整理今天的项目进展，并根据优先级列出下一步行动。我还想确认语音记录不会因为内容太长而丢失。接下来请检查所有未完成事项，标注负责人和截止日期，并把风险最高的三项单独列出来，最后生成一份可以直接发送给团队的简洁摘要，确保任何已识别的内容都不会从界面或会话中消失。";
    let expected_tail = "项，标注负责人和截止日期，并把风险最高的三项单独列出来，最后生成一份可以直接发送给团队的简洁摘要，确保任何已识别的内容都不会从界面或会话中消失。";
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Voice caption workspace should create");
    enter_canvas(&app, cx, workspace.path());

    app.update(cx, |app, cx| {
        app.show_voice_recording_transcript_for_e2e(transcript);
        cx.notify();
    });
    cx.run_until_parked();

    let collapsed = app.read_with(cx, |app, _cx| app.voice_e2e_snapshot());
    let dock_bounds = cx
        .debug_bounds("workspace-voice-control")
        .expect("recording Voice must keep rendering its compact Dock control");
    let shortcut_bounds = cx
        .debug_bounds("workspace-voice-shortcut")
        .expect("recording Voice Dock must keep the configured shortcut visible");
    let collapsed_bounds = cx
        .debug_bounds("workspace-voice-caption")
        .expect("a non-empty recording partial must render the live caption strip");
    let collapsed_text_container_bounds = cx
        .debug_bounds("voice-caption-text")
        .expect("collapsed caption must reserve a visible text region");
    let collapsed_text_bounds = cx
        .debug_bounds("voice-caption-text-content")
        .expect("collapsed caption must render a directly visible text line");
    assert_eq!(
        collapsed.dock_label, "Listening",
        "active Voice Dock label must remain the compact status text"
    );
    assert_eq!(
        collapsed.dock_shortcut, "⌘E",
        "active Voice Dock must expose the configured Voice shortcut"
    );
    assert!(
        shortcut_bounds.size.width > gpui::px(0.0),
        "configured Voice shortcut must occupy a visible rendered region"
    );
    assert!(
        !collapsed.dock_label.contains(transcript),
        "compact Dock must not contain the full recognized transcript"
    );
    assert_eq!(
        collapsed.caption_text.as_deref(),
        Some(transcript),
        "caption state must retain the complete recognized transcript"
    );
    assert_eq!(
        collapsed.caption_preview.as_deref(),
        Some(expected_tail),
        "collapsed caption must show the exact newest 72-character tail"
    );
    assert_ne!(
        collapsed.caption_preview.as_deref(),
        Some(transcript),
        "collapsed one-line preview must not pretend the full long transcript fits"
    );
    assert!(
        dock_bounds.size.width <= gpui::px(180.0),
        "active Voice control must stay within the 180px compact width budget"
    );
    assert!(
        collapsed_bounds.size.height <= gpui::px(56.0),
        "collapsed live caption must remain within the 56px one-line height budget"
    );
    assert_eq!(
        collapsed_text_bounds.origin.x, collapsed_text_container_bounds.origin.x,
        "collapsed transcript must begin at the text region edge without a left fade overlay"
    );
    assert!(
        !collapsed.caption_expanded,
        "live caption must start in the collapsed one-line state"
    );

    let expand = cx
        .debug_bounds("voice-caption-expand")
        .expect("collapsed caption must expose the Full transcript control");
    cx.simulate_click(expand.center(), Modifiers::none());
    cx.run_until_parked();

    let expanded = app.read_with(cx, |app, _cx| app.voice_e2e_snapshot());
    let expanded_bounds = cx
        .debug_bounds("workspace-voice-caption")
        .expect("expanded transcript must keep the caption surface visible");
    assert!(
        expanded.caption_expanded,
        "clicking Full transcript must enter the expanded caption state"
    );
    assert_eq!(
        expanded.caption_text.as_deref(),
        Some(transcript),
        "expanding the caption must preserve the exact complete transcript"
    );
    assert!(
        expanded_bounds.size.height > collapsed_bounds.size.height,
        "expanded caption must be taller than the collapsed one-line strip"
    );

    let collapse = cx
        .debug_bounds("voice-caption-expand")
        .expect("expanded caption must expose the 收起 control");
    cx.simulate_click(collapse.center(), Modifiers::none());
    cx.run_until_parked();

    let collapsed_again = app.read_with(cx, |app, _cx| app.voice_e2e_snapshot());
    let collapsed_again_bounds = cx
        .debug_bounds("workspace-voice-caption")
        .expect("collapsed transcript must keep the caption surface visible");
    assert!(
        !collapsed_again.caption_expanded,
        "clicking 收起 must return to the collapsed one-line state"
    );
    assert_eq!(
        collapsed_again.caption_text.as_deref(),
        Some(transcript),
        "collapsing the caption must preserve the exact complete transcript"
    );
    assert_eq!(
        collapsed_again_bounds.size.height, collapsed_bounds.size.height,
        "收起 must restore the original compact caption height"
    );
}

/// Voice 等待与回复切换：字幕卡片保持紧凑并具有活动波形，助手开始回复后立即隐藏。
///
/// 数据构造（含关键数值的推导过程）：
///   transcript = 15 个中文字符（非空，因此 Thinking 阶段必须渲染字幕卡片）
///   caption width budget = 560px（较旧 720px 减少 160px，避免遮挡画布）
///   caption height budget = 56px（目标条高 48px，额外预留 8px 渲染容差）
///   animation advance = 160ms（1.2 cycles/s × 0.160s = 0.192 cycle，足以改变波形相位）
///   surface opacity = 722‰（alpha 184 / 255 × 1000 ≈ 722，保留背景内容可见性）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas，通过公开 E2E seam 注入一个带转写文本的单次 Voice Thinking 回合。
///   2. 检查真实卡片边界、半透明表面配置及起始波形相位。
///   3. 精确推进 160ms 动画时钟，确认波形相位变化且卡片仍可见。
///   4. 通过同一公开 seam 把该回合切换为 Speaking，确认卡片可见状态关闭并停止动画。
///
/// 预期结果：
///   - 正断言：Thinking 时卡片可见、宽高符合紧凑预算、表面为半透明且波形会推进。
///   - 正断言：Speaking 时对话进入回复阶段，卡片可见状态与动画相位均被清除。
///   - 负断言：Thinking 卡片不得保留旧 720px 宽度或不透明表面；Speaking 时不得保持可见。
#[test]
fn compact_caption_animates_while_thinking_and_hides_when_assistant_starts_replying() {
    let transcript = "请总结本周项目进展";
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Voice caption workspace should create");
    enter_canvas(&app, cx, workspace.path());

    app.update(cx, |app, cx| {
        app.show_voice_thinking_for_e2e(transcript);
        app.reset_animation_display_callback_clock_for_e2e(cx);
        cx.notify();
    });
    cx.run_until_parked();

    let thinking = app.read_with(cx, |app, _cx| app.voice_e2e_snapshot());
    let thinking_bounds = cx
        .debug_bounds("workspace-voice-caption")
        .expect("Thinking with a transcript must render the compact voice caption");
    assert_eq!(
        thinking.dialogue_phase, "thinking",
        "Voice turn must start in Thinking before the assistant begins replying"
    );
    assert!(
        thinking.caption_visible,
        "Thinking with a non-empty transcript must keep the voice caption visible"
    );
    assert!(
        thinking_bounds.size.width <= gpui::px(560.0),
        "compact caption must not exceed the 560px width budget"
    );
    assert!(
        thinking_bounds.size.height <= gpui::px(56.0),
        "compact caption must not exceed the 56px collapsed height budget"
    );
    assert_eq!(
        thinking.caption_surface_opacity_milli, 722,
        "caption surface must use the specified translucent 72.2% opacity"
    );
    assert_ne!(
        thinking.caption_surface_opacity_milli, 1_000,
        "caption surface must not be fully opaque"
    );

    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(160), cx);
    });
    let animated = app.read_with(cx, |app, _cx| app.voice_e2e_snapshot());
    assert_ne!(
        animated.caption_waveform_phase_milli, thinking.caption_waveform_phase_milli,
        "advancing 160ms while Thinking must move the caption waveform"
    );
    assert!(
        animated.caption_visible,
        "waveform animation must not hide the caption before the assistant starts replying"
    );

    app.update(cx, |app, cx| {
        app.show_voice_replying_for_e2e(transcript);
        cx.notify();
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(16), cx);
    });
    cx.run_until_parked();

    let replying = app.read_with(cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(
        replying.dialogue_phase, "speaking",
        "assistant reply seam must transition the Voice turn into Speaking"
    );
    assert!(
        !replying.caption_visible,
        "assistant Speaking must hide the voice caption so the reply owns the workspace"
    );
    assert_eq!(
        replying.caption_waveform_phase_milli, 0,
        "hidden caption must reset its waveform phase instead of consuming animation frames"
    );
}
