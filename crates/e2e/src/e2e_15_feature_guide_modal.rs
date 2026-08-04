//! E2E-15：功能导览的高优先级浮层协调与背景 Action 隔离。

use app::{NewTerminal, OpenFeatureGuide, ToggleSettings};
use canvas::CanvasNodeKind;
use gpui::TestAppContext;

use crate::harness::{echo_factory, enter_canvas, make_app, read_runtime};

/// 最终识别字幕协调：TranscriptOnly 字幕存在期间导览必须暂停，字幕清除后恢复。
///
/// 数据构造（含关键数值的推导过程）：
///   selected topic index = 2（workspace_layouts）
///   transcript-only captions = 1（“把下一步交给小张”）
///   simultaneously visible high-priority surfaces = 1（字幕或导览二选一）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从 Help 打开导览并选中第 3 个主题。
///   2. 注入 TranscriptOnly 最终识别结果并触发真实 GPUI 重绘。
///   3. 确认字幕可见、导览转为 Suspended，且 overlay 离开渲染树。
///   4. 清除语音运行时并重绘，确认导览恢复到原主题。
///
/// 预期结果：
///   - 正断言：最终字幕期间导览为 suspended，清除后恢复 visible。
///   - 正断言：恢复后仍为 workspace_layouts，不丢浏览进度。
///   - 负断言：字幕与导览 overlay 不得同时可见，暂停也不能变成会话关闭。
#[test]
fn e2e_15_feature_guide_suspends_for_transcript_only_caption_and_resumes() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());

    cx.dispatch_action(OpenFeatureGuide);
    cx.run_until_parked();
    click_debug(cx, "feature-guide-topic-2");
    app.update(cx, |app, cx| {
        app.show_voice_transcript_only_for_e2e("把下一步交给小张");
        cx.notify();
    });
    cx.refresh()
        .expect("TranscriptOnly blocker 测试应能请求窗口重绘");
    cx.run_until_parked();

    let voice = app.update(cx, |app, _| app.voice_e2e_snapshot());
    let suspended = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert!(
        voice.caption_visible,
        "TranscriptOnly 最终识别文本必须保持字幕可见，测试才覆盖目标闪现场景"
    );
    assert_eq!(
        suspended.presentation, "suspended",
        "最终识别字幕仍可见时，导览必须暂停而不是抢占同一浮层区域"
    );
    assert!(
        !suspended.visible,
        "TranscriptOnly 字幕期间导览不得继续满足 overlay 的 visible 渲染条件"
    );
    assert_ne!(
        suspended.presentation, "closed_for_session",
        "系统级字幕 blocker 不能误判为用户主动关闭当前会话"
    );

    app.update(cx, |app, cx| {
        app.reset_voice_runtime_for_e2e();
        cx.notify();
    });
    cx.refresh()
        .expect("清除 TranscriptOnly 字幕后应能请求窗口重绘");
    cx.run_until_parked();

    let resumed = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert_eq!(
        resumed.presentation, "visible",
        "最终字幕清除后，临时暂停的导览必须自动恢复 visible"
    );
    assert_eq!(
        resumed.topic, "workspace_layouts",
        "字幕暂停与恢复必须保留用户已选中的第 3 个主题"
    );
    assert!(
        cx.debug_bounds("feature-guide-overlay").is_some(),
        "字幕清除后必须重新渲染 feature-guide-overlay"
    );
}

/// 模态 Action 隔离：导览可见时，菜单/快捷键派发不得修改背景 Canvas 或 Settings。
///
/// 数据构造（含关键数值的推导过程）：
///   initial terminal nodes = 0（fresh 临时 workspace）
///   background actions = NewTerminal(1) + ToggleSettings(1) = 2
///   expected terminal delta = 0 - 0 = 0
///   expected settings panels = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入真实 Canvas 并从 Help 打开导览。
///   2. 直接派发原生菜单复用的 NewTerminal 与 ToggleSettings Action。
///   3. 读取 Runtime 节点、Settings 状态与导览表现层。
///
/// 预期结果：
///   - 正断言：导览保持 visible，用户可继续当前流程。
///   - 负断言：不得创建 Terminal，也不得在导览背后打开 Settings。
///   - 负断言：背景 Action 不得隐式完成或关闭导览。
#[test]
fn e2e_15_feature_guide_modal_blocks_background_actions() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::tempdir().expect("应创建模态 Action 隔离临时 Canvas 项目");
    enter_canvas(&app, cx, workspace.path());

    cx.dispatch_action(OpenFeatureGuide);
    cx.run_until_parked();
    cx.dispatch_action(NewTerminal);
    cx.dispatch_action(ToggleSettings);
    cx.run_until_parked();

    let terminal_count = read_runtime(&app, cx, |runtime| {
        runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Term(_)))
            .count()
    });
    let settings = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    let guide = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert_eq!(
        terminal_count, 0,
        "导览可见时 NewTerminal Action 必须被模态边界隔离，实际 Terminal 数为 {terminal_count}"
    );
    assert!(
        !settings.settings_open,
        "导览可见时 ToggleSettings Action 不得穿透到背景打开面板"
    );
    assert!(
        guide.visible,
        "背景 Action 被隔离后，导览必须继续 visible 供用户完成当前流程"
    );
    assert_ne!(
        guide.presentation, "closed_for_session",
        "背景 Action 不得把导览误关闭为 closed_for_session"
    );
}

/// 跨主题导览：从 Settings 切换深浅主题后，帮助中心都必须保持可打开且使用当前主题。
///
/// 数据构造（含关键数值的推导过程）：
///   theme switches = 2（paper-blue → forest-mint → paper-blue）
///   guide opens    = 2（每次主题切换后各打开 1 次）
///   expected visible overlays = 1（任一时刻仅当前导览浮层可见）
///   expected topic entries    = orchestration(1) + product demos(3) = 4
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas，通过 Settings → Themes 切换到 Forest Mint 并关闭设置。
///   2. 派发 OpenFeatureGuide，确认深色主题仍生效且导览标题、主题入口可见。
///   3. 关闭导览，重新进入 Settings，切回 Paper Blue 并关闭设置。
///   4. 再次派发 OpenFeatureGuide，确认浅色主题生效且同一导览结构仍可见。
///
/// 预期结果：
///   - 正断言：Forest Mint 与 Paper Blue 下导览都为 visible，并渲染标题和主题入口。
///   - 正断言：两次打开都沿用 Settings 当前选中的稳定主题 id 与深浅模式。
///   - 负断言：深色主题不得回报 Paper Blue，切回浅色后不得残留 dark mode。
#[test]
fn e2e_15_feature_guide_remains_readable_surface_across_light_and_dark_themes() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::tempdir().expect("应创建跨主题导览临时 Canvas 项目");
    enter_canvas(&app, cx, workspace.path());

    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-themes");
    click_debug(cx, "settings-theme-forest-mint");
    let forest_settings = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        forest_settings.active_theme_id, "forest-mint",
        "打开深色导览前必须确实切换到 Forest Mint"
    );
    assert!(
        forest_settings.theme_is_dark,
        "Forest Mint 必须报告 dark mode，测试才覆盖深色主题分支"
    );
    assert_ne!(
        forest_settings.active_theme_id, "paper-blue",
        "Forest Mint 选中后不得仍回报默认 Paper Blue"
    );
    click_debug(cx, "settings-close");

    cx.dispatch_action(OpenFeatureGuide);
    cx.run_until_parked();
    let forest_guide = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert!(
        forest_guide.visible,
        "Forest Mint 下帮助中心必须保持 visible"
    );
    assert!(
        cx.debug_bounds("feature-guide-title").is_some(),
        "Forest Mint 下必须渲染帮助中心标题"
    );
    assert!(
        cx.debug_bounds("feature-guide-topic-0").is_some(),
        "Forest Mint 下必须渲染第 1 个功能主题入口"
    );
    assert!(
        cx.debug_bounds("feature-guide-topic-3").is_some(),
        "Forest Mint 下必须渲染第 4 个功能主题入口，证明四项导航完整"
    );
    assert!(
        cx.debug_bounds("feature-guide-topic-4").is_none(),
        "Forest Mint 下不得渲染不存在的第 5 个主题入口"
    );

    click_debug(cx, "feature-guide-close");
    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-themes");
    click_debug(cx, "settings-theme-paper-blue");
    let paper_settings = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        paper_settings.active_theme_id, "paper-blue",
        "第二次打开导览前必须切回 Paper Blue"
    );
    assert!(
        !paper_settings.theme_is_dark,
        "切回 Paper Blue 后不得残留 Forest Mint 的 dark mode"
    );
    assert_ne!(
        paper_settings.active_theme_id, "forest-mint",
        "Paper Blue 选中后不得仍回报 Forest Mint"
    );
    click_debug(cx, "settings-close");

    cx.dispatch_action(OpenFeatureGuide);
    cx.run_until_parked();
    let paper_guide = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert!(paper_guide.visible, "Paper Blue 下帮助中心必须保持 visible");
    assert!(
        cx.debug_bounds("feature-guide-overlay").is_some(),
        "Paper Blue 下必须重新渲染帮助中心 overlay"
    );
    assert!(
        cx.debug_bounds("feature-guide-title").is_some(),
        "Paper Blue 下必须重新渲染帮助中心标题"
    );
    assert_ne!(
        paper_guide.presentation, "closed_for_session",
        "跨主题关闭与重开不得把帮助中心误标记为会话永久关闭"
    );
}

fn click_debug(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("点击前应找到 {selector} 的真实 debug bounds"));
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    cx.run_until_parked();
}
