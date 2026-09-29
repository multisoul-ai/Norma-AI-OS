use gpui::{Modifiers, TestAppContext};
use std::time::Duration;

use crate::harness::{echo_factory, enter_canvas, make_app};

/// Voice 长转写：Dock 保持紧凑，字幕条默认显示全文，并可临时收起后恢复。
///
/// 数据构造（含关键数值的推导过程）：
///   transcript block = 132 个中文/标点字符
///   long transcript  = 132 × 4 = 528 字符（足以超过 104px 全文正文高度）
///   expanded body height = 104px（line height 16px × 6.5 行，超出内容在正文内滚动）
///   expanded caption max height = body 104px + vertical padding 14px + 10px tolerance = 128px
///   collapsed caption max height = 56px（目标条高 48px，额外预留 8px 渲染容差）
///   compact Voice max width = 180px（图标约 20 + 间距 8 + Listening/快捷键约 110 + 内边距 26）
///   text left inset = 0px（首字必须从文本区域边缘开始，不能被渐隐层遮挡）
///   disclosure label height = 11px font + 3px leading allowance = 14px max
///   disclosure chevron size = 10px × 10px（独立 SVG，不使用字体字符模拟图标）
///   disclosure clicks = collapse(1) + expand(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas，并通过公开 E2E seam 注入一段正在识别的长中文 partial。
///   2. 读取 Voice 快照和真实渲染树，确认字幕默认展开且全文未进入单行截断分支。
///   3. 点击「收起」，检查结尾预览、紧凑高度及文本起点。
///   4. 再次点击「全文」，确认完整内容与初始展开高度恢复。
///
/// 预期结果：
///   - 正断言：Dock 显示 `Listening` 与 `⌘E` 快捷键，且 Voice 控件保持紧凑。
///   - 正断言：字幕默认显示完整原文，并在 128px 边界内滚动而非无限遮挡画布。
///   - 正断言：「收起/全文」使用紧凑系统字排版，独立 Chevron 图标与文字垂直居中。
///   - 正断言：收起后显示结尾预览且保持单行低高度，再次点击恢复全文。
///   - 负断言：Dock 不得包含长转写；默认全文不得渲染单行截断节点；折叠预览不得冒充完整原文或遮挡首字。
#[test]
fn long_voice_transcript_defaults_to_full_and_can_be_collapsed_without_growing_dock() {
    let transcript_block = "请帮我整理今天的项目进展，并根据优先级列出下一步行动。我还想确认语音记录不会因为内容太长而丢失。接下来请检查所有未完成事项，标注负责人和截止日期，并把风险最高的三项单独列出来，最后生成一份可以直接发送给团队的简洁摘要，确保任何已识别的内容都不会从界面或会话中消失。";
    let transcript = transcript_block.repeat(4);
    let expected_tail = "项，标注负责人和截止日期，并把风险最高的三项单独列出来，最后生成一份可以直接发送给团队的简洁摘要，确保任何已识别的内容都不会从界面或会话中消失。";
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Voice caption workspace should create");
    enter_canvas(&app, cx, workspace.path());

    app.update(cx, |app, cx| {
        app.show_voice_recording_transcript_for_e2e(transcript.clone());
        cx.notify();
    });
    cx.run_until_parked();

    let expanded = app.read_with(cx, |app, _cx| app.voice_e2e_snapshot());
    let dock_bounds = cx
        .debug_bounds("workspace-voice-control")
        .expect("recording Voice must keep rendering its compact Dock control");
    let shortcut_bounds = cx
        .debug_bounds("workspace-voice-shortcut")
        .expect("recording Voice Dock must keep the configured shortcut visible");
    let expanded_bounds = cx
        .debug_bounds("workspace-voice-caption")
        .expect("a non-empty recording partial must render the live caption strip");
    let expanded_text_bounds = cx
        .debug_bounds("voice-caption-text")
        .expect("default full caption must reserve a visible scrollable text region");
    let disclosure_label_bounds = cx
        .debug_bounds("voice-caption-expand-label")
        .expect("default full caption must render a dedicated 收起 label");
    let disclosure_chevron_bounds = cx
        .debug_bounds("voice-caption-expand-chevron")
        .expect("default full caption must render a dedicated Chevron icon");
    assert_eq!(
        expanded.dock_label, "Listening",
        "active Voice Dock label must remain the compact status text"
    );
    assert_eq!(
        expanded.dock_shortcut, "⌘E",
        "active Voice Dock must expose the configured Voice shortcut"
    );
    assert!(
        shortcut_bounds.size.width > gpui::px(0.0),
        "configured Voice shortcut must occupy a visible rendered region"
    );
    assert!(
        !expanded.dock_label.contains(transcript.as_str()),
        "compact Dock must not contain the full recognized transcript"
    );
    assert_eq!(
        expanded.caption_text.as_deref(),
        Some(transcript.as_str()),
        "default full caption must retain the exact complete recognized transcript"
    );
    assert_eq!(
        expanded.caption_preview.as_deref(),
        Some(expected_tail),
        "caption state must retain the exact newest 72-character fallback preview"
    );
    assert_ne!(
        expanded.caption_preview.as_deref(),
        Some(transcript.as_str()),
        "72-character fallback preview must remain distinct from the complete transcript"
    );
    assert!(
        dock_bounds.size.width <= gpui::px(180.0),
        "active Voice control must stay within the 180px compact width budget"
    );
    assert!(
        expanded.caption_expanded,
        "each new live caption must start in the full transcript state"
    );
    assert!(
        cx.debug_bounds("voice-caption-text-content").is_none(),
        "default full caption must not render the collapsed single-line truncation node"
    );
    assert!(
        expanded_text_bounds.size.height >= gpui::px(102.0),
        "528-character transcript must fill the intended 104px scroll region"
    );
    assert!(
        expanded_text_bounds.size.height <= gpui::px(104.0),
        "default full transcript body must not exceed the compact 104px scroll-region budget"
    );
    assert!(
        expanded_bounds.size.height <= gpui::px(128.0),
        "default full caption must remain within the 128px bounded scroll-surface budget"
    );
    assert!(
        disclosure_label_bounds.size.height <= gpui::px(14.0),
        "Full transcript label must stay within the 14px compact typography budget"
    );
    assert!(
        disclosure_label_bounds.size.height >= gpui::px(11.0),
        "Full transcript label must remain legible at no less than the intended 11px font size"
    );
    assert_eq!(
        disclosure_chevron_bounds.size.width,
        gpui::px(10.0),
        "Full transcript Chevron must use the exact 10px icon width instead of a font glyph"
    );
    assert_eq!(
        disclosure_chevron_bounds.size.height,
        gpui::px(10.0),
        "Full transcript Chevron must use the exact 10px icon height instead of a font glyph"
    );
    assert!(
        (disclosure_label_bounds.center().y - disclosure_chevron_bounds.center().y).abs()
            <= gpui::px(1.0),
        "default 收起 label and Chevron must be vertically centered within 1px"
    );

    let collapse = cx
        .debug_bounds("voice-caption-expand")
        .expect("default full caption must expose the 收起 control");
    cx.simulate_click(collapse.center(), Modifiers::none());
    cx.run_until_parked();

    let collapsed = app.read_with(cx, |app, _cx| app.voice_e2e_snapshot());
    let collapsed_bounds = cx
        .debug_bounds("workspace-voice-caption")
        .expect("collapsed transcript must keep the caption surface visible");
    let collapsed_text_container_bounds = cx
        .debug_bounds("voice-caption-text")
        .expect("collapsed caption must reserve a visible text region");
    let collapsed_text_bounds = cx
        .debug_bounds("voice-caption-text-content")
        .expect("collapsed caption must render a directly visible text line");
    assert!(
        !collapsed.caption_expanded,
        "clicking 收起 must enter the collapsed one-line state"
    );
    assert_eq!(
        collapsed.caption_text.as_deref(),
        Some(transcript.as_str()),
        "collapsing the caption must preserve the exact complete transcript in state"
    );
    assert_eq!(
        collapsed.caption_preview.as_deref(),
        Some(expected_tail),
        "collapsed caption must show the exact newest 72-character tail"
    );
    assert!(
        collapsed_bounds.size.height <= gpui::px(56.0),
        "collapsed live caption must remain within the 56px one-line height budget"
    );
    assert!(
        collapsed_bounds.size.height < expanded_bounds.size.height,
        "collapsed caption must be shorter than the default full transcript surface"
    );
    assert_eq!(
        collapsed_text_bounds.origin.x, collapsed_text_container_bounds.origin.x,
        "collapsed transcript must begin at the text region edge without a left fade overlay"
    );

    let expand = cx
        .debug_bounds("voice-caption-expand")
        .expect("collapsed caption must expose the 全文 control");
    cx.simulate_click(expand.center(), Modifiers::none());
    cx.run_until_parked();

    let expanded_again = app.read_with(cx, |app, _cx| app.voice_e2e_snapshot());
    let expanded_again_bounds = cx
        .debug_bounds("workspace-voice-caption")
        .expect("restored full transcript must keep the caption surface visible");
    assert!(
        expanded_again.caption_expanded,
        "clicking 全文 must restore the expanded caption state"
    );
    assert_eq!(
        expanded_again.caption_text.as_deref(),
        Some(transcript.as_str()),
        "restoring the full caption must preserve the exact complete transcript"
    );
    assert_eq!(
        expanded_again_bounds.size.height, expanded_bounds.size.height,
        "全文 must restore the original bounded full-caption height"
    );
}

/// Voice 等待与回复切换：字幕在 Thinking 与 Speaking 都显示原文，声纹随状态持续反馈。
///
/// 数据构造（含关键数值的推导过程）：
///   transcript = 9 个中文字符（「请总结本周项目进展」，非空，因此两个处理阶段都必须显示）
///   caption width budget = 560px（较旧 720px 减少 160px，避免遮挡画布）
///   caption height budget = body 104px + vertical padding 14px + 10px tolerance = 128px
///   Thinking animation advance = 160ms（1.2 cycles/s × 0.160s = 0.192 cycle，足以改变波形相位）
///   Speaking animation advance = 16ms（1.2 cycles/s × 0.016s = 0.0192 cycle；停止后仍应保持 0）
///   surface opacity = 722‰（alpha 184 / 255 × 1000 ≈ 722，保留背景内容可见性）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas，通过公开 E2E seam 注入一个带转写文本的单次 Voice Thinking 回合。
///   2. 检查「Thinking…」状态、完整原文、真实卡片边界、半透明表面及起始波形相位。
///   3. 精确推进 160ms 动画时钟，确认波形相位变化且卡片仍可见。
///   4. 通过同一公开 seam 把该回合切换为 Speaking，再推进 16ms 动画时钟。
///
/// 预期结果：
///   - 正断言：Thinking 时卡片标记「Thinking…」、默认全文、宽高有界且波形会推进。
///   - 正断言：Speaking 时卡片标记「Replying...」并继续显示完全相同的原文。
///   - 负断言：Thinking 卡片不得使用不透明表面；Speaking 不得隐藏字幕或冻结声纹。
#[test]
fn voice_caption_remains_visible_and_animated_while_replying() {
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
    assert_eq!(
        thinking.dock_label, "Thinking…",
        "Thinking caption must expose the processing-state label"
    );
    assert_eq!(
        thinking.caption_text.as_deref(),
        Some(transcript),
        "Thinking caption must retain the exact submitted voice transcript"
    );
    assert!(
        thinking.caption_expanded,
        "Thinking with a new transcript must default to the full caption state"
    );
    assert!(
        thinking_bounds.size.width <= gpui::px(560.0),
        "compact caption must not exceed the 560px width budget"
    );
    assert!(
        thinking_bounds.size.height <= gpui::px(128.0),
        "full caption must not exceed the 128px bounded scroll-surface budget"
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
    let replying_bounds = cx
        .debug_bounds("workspace-voice-caption")
        .expect("Speaking must keep the submitted transcript visibly rendered");
    assert_eq!(
        replying.dialogue_phase, "speaking",
        "assistant reply seam must transition the Voice turn into Speaking"
    );
    assert!(
        replying.caption_visible,
        "assistant Speaking must keep the submitted voice transcript visible"
    );
    assert_eq!(
        replying.dock_label, "Replying...",
        "Speaking caption must expose the reply-state label instead of Thinking"
    );
    assert_eq!(
        replying.caption_text.as_deref(),
        Some(transcript),
        "Speaking caption must preserve the exact transcript shown during Thinking"
    );
    assert!(
        replying_bounds.size.height <= gpui::px(128.0),
        "Speaking caption must retain the same bounded full-caption height budget"
    );
    assert_ne!(
        replying.caption_waveform_phase_milli, 0,
        "Speaking caption must keep the reply waveform moving"
    );
}

/// Voice Dock 无字幕动效：一开始收音就能看到声纹，减少动态效果时保持静态反馈。
///
/// 数据构造（含关键数值的推导过程）：160 ms × 1.2 cycles/s = 0.192 周期，足以观察相位变化。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas 并注入空的录音 partial，确认无字幕但有 Dock 声纹。
///   2. 推进 160 ms，确认相位变化。
///   3. 启用减少动态效果，再推进 160 ms，确认相位归零且声纹仍可见。
///
/// 预期结果：录音时有可见声纹且相位推进；减少动态效果后静态声纹仍存在，动画停止。
#[test]
fn voice_dock_waveform_runs_without_caption_and_respects_reduced_motion() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Voice waveform workspace should create");
    enter_canvas(&app, cx, workspace.path());
    app.update(cx, |app, cx| {
        app.show_voice_recording_transcript_for_e2e("");
        cx.notify();
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("workspace-voice-waveform").is_some(),
        "recording Dock must render the selected five-bar waveform before ASR partials arrive"
    );
    assert!(
        cx.debug_bounds("workspace-voice-caption").is_none(),
        "empty ASR partial must not fabricate a transcript caption"
    );
    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(160), cx)
    });
    let animated = app.read_with(cx, |app, _| app.voice_e2e_snapshot());
    assert_ne!(
        animated.caption_waveform_phase_milli, 0,
        "Dock waveform must animate even when no caption has appeared"
    );
    app.update(cx, |app, cx| {
        app.set_feature_guide_reduce_motion_for_e2e(true, cx);
        app.advance_animations_for_e2e(Duration::from_millis(160), cx);
        cx.notify();
    });
    cx.run_until_parked();
    let reduced = app.read_with(cx, |app, _| app.voice_e2e_snapshot());
    assert_eq!(
        reduced.caption_waveform_phase_milli, 0,
        "reduced motion must stop the voice waveform phase"
    );
    assert!(
        cx.debug_bounds("workspace-voice-waveform").is_some(),
        "reduced motion must retain a static voice status glyph"
    );
}

/// Voice 字幕回合边界：回复结束隐藏旧原文，下一次录音只显示新的 partial。
///
/// 数据构造（含关键数值的推导过程）：
///   completed transcript = 9 个中文字符（「请总结本周项目进展」）
///   next partial         = 8 个中文字符（「补充检查测试结果」）
///   dialogue transitions = 3 次（Thinking → Speaking → Idle）
///   next recording       = 1 次（Idle → Recording）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas，注入旧原文并把回合推进到 Speaking。
///   2. 通过公开 E2E seam 完成回复，检查旧字幕及渲染节点都已隐藏。
///   3. 开始下一次录音并注入新 partial，检查字幕重新出现且只包含新内容。
///
/// 预期结果：
///   - 正断言：回合完成后进入 Idle，字幕状态与渲染节点都清空。
///   - 正断言：下一次 Recording 显示「Listening」和新的完整 partial。
///   - 负断言：新字幕不得继续显示上一回合原文。
#[test]
fn voice_caption_hides_after_turn_and_next_recording_replaces_old_transcript() {
    let completed_transcript = "请总结本周项目进展";
    let next_partial = "补充检查测试结果";
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Voice caption workspace should create");
    enter_canvas(&app, cx, workspace.path());

    app.update(cx, |app, cx| {
        app.show_voice_replying_for_e2e(completed_transcript);
        app.finish_voice_reply_for_e2e();
        cx.notify();
    });
    cx.run_until_parked();

    let completed = app.read_with(cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(
        completed.dialogue_phase, "idle",
        "completed one-shot Voice reply must leave the dialogue in Idle"
    );
    assert!(
        !completed.caption_visible,
        "completed Voice reply must hide the submitted transcript"
    );
    assert_eq!(
        completed.caption_text, None,
        "completed Voice reply must clear the visible caption projection"
    );
    assert!(
        cx.debug_bounds("workspace-voice-caption").is_none(),
        "completed Voice reply must remove the caption from the rendered tree"
    );

    app.update(cx, |app, cx| {
        app.show_voice_recording_transcript_for_e2e(next_partial);
        cx.notify();
    });
    cx.run_until_parked();

    let next_recording = app.read_with(cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(
        next_recording.phase, "recording",
        "next Voice input must enter Recording before showing its partial"
    );
    assert_eq!(
        next_recording.dock_label, "Listening",
        "next Voice input must mark the replacement caption as Listening"
    );
    assert_eq!(
        next_recording.caption_text.as_deref(),
        Some(next_partial),
        "next Voice input must display the exact new partial transcript"
    );
    assert_ne!(
        next_recording.caption_text.as_deref(),
        Some(completed_transcript),
        "next Voice input must not retain the completed turn transcript"
    );
    assert!(
        cx.debug_bounds("workspace-voice-caption").is_some(),
        "next non-empty partial must render a replacement caption"
    );
}
