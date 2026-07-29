use gpui::{Modifiers, TestAppContext};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// Voice 长转写：Dock 保持紧凑，字幕条保留全文，并可展开查看完整内容。
///
/// 数据构造（含关键数值的推导过程）：
///   long transcript = 132 个中文/标点字符（超过两行字幕 96 字符的预览预算 36 字符）
///   collapsed caption max height = 96px（原型条高约 86px，额外预留 10px 渲染容差）
///   compact Voice max width = 180px（图标约 20 + 间距 8 + Listening/快捷键约 110 + 内边距 26）
///   disclosure clicks = expand(1) + collapse(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas，并通过公开 E2E seam 注入一段正在识别的长中文 partial。
///   2. 读取 Voice 快照和真实渲染树，检查 Dock 与折叠字幕条。
///   3. 点击 `voice-caption-expand`，再次读取快照和字幕条边界。
///   4. 再次点击同一语义按钮，确认 `Show less` 可以收回字幕条。
///
/// 预期结果：
///   - 正断言：Dock 显示 `Listening` 与 `⌘E` 快捷键，且 Voice 控件保持紧凑。
///   - 正断言：字幕条保存完整原文，折叠态显示结尾预览并保持低高度。
///   - 正断言：点击后进入全文展开态，字幕容器高度增加，再次点击可收回。
///   - 负断言：Dock 不得包含长转写，折叠预览不得冒充完整原文或只显示开头。
#[test]
fn long_voice_transcript_uses_expandable_caption_without_growing_dock() {
    let transcript = "请帮我整理今天的项目进展，并根据优先级列出下一步行动。我还想确认语音记录不会因为内容太长而丢失。接下来请检查所有未完成事项，标注负责人和截止日期，并把风险最高的三项单独列出来，最后生成一份可以直接发送给团队的简洁摘要，确保任何已识别的内容都不会从界面或会话中消失。";
    let expected_tail = "不会因为内容太长而丢失。接下来请检查所有未完成事项，标注负责人和截止日期，并把风险最高的三项单独列出来，最后生成一份可以直接发送给团队的简洁摘要，确保任何已识别的内容都不会从界面或会话中消失。";
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
        "collapsed caption must show the exact newest 96-character tail"
    );
    assert_ne!(
        collapsed.caption_preview.as_deref(),
        Some(transcript),
        "collapsed two-line preview must not pretend the full long transcript fits"
    );
    assert!(
        dock_bounds.size.width <= gpui::px(180.0),
        "active Voice control must stay within the 180px compact width budget"
    );
    assert!(
        collapsed_bounds.size.height <= gpui::px(96.0),
        "collapsed live caption must remain a low-profile strip"
    );
    assert!(
        !collapsed.caption_expanded,
        "live caption must start in the collapsed two-line state"
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
        "expanded caption must be taller than the collapsed two-line strip"
    );

    let collapse = cx
        .debug_bounds("voice-caption-expand")
        .expect("expanded caption must expose the Show less control");
    cx.simulate_click(collapse.center(), Modifiers::none());
    cx.run_until_parked();

    let collapsed_again = app.read_with(cx, |app, _cx| app.voice_e2e_snapshot());
    let collapsed_again_bounds = cx
        .debug_bounds("workspace-voice-caption")
        .expect("collapsed transcript must keep the caption surface visible");
    assert!(
        !collapsed_again.caption_expanded,
        "clicking Show less must return to the collapsed two-line state"
    );
    assert_eq!(
        collapsed_again.caption_text.as_deref(),
        Some(transcript),
        "collapsing the caption must preserve the exact complete transcript"
    );
    assert_eq!(
        collapsed_again_bounds.size.height, collapsed_bounds.size.height,
        "Show less must restore the original compact caption height"
    );
}
