//! E2E-15：功能导览的永久 Help 入口与首次项目自动展示。

use app::OpenFeatureGuide;
use gpui::{Modifiers, TestAppContext};

use crate::harness::{echo_factory, enter_canvas, make_app, make_app_with_settings_store};

/// Help 功能导览：用户可切换主题、推进下一项、跳过，并在跳过后从 Help 再次打开。
/// 数据构造（含关键数值的推导过程）：
///   guide topics = orchestration(1) + new demos(2) + existing demos(3) = 6
///   initial topic index = 0（agent_orchestration）；first media index = 1（voice_group_control）
///   new collaboration index = 2；layout index = 4；primary advance = 4 + 1 = 5（agent_notifications）
///   playback toggles = pause(1) + play(1) = 2
///   reduced-motion transitions = enable(1) + disable(1) = 2
///   explicit endings = Skip(1) + Close(1) + Complete(1) = 3
///   visible surface threshold = 100×100px（远小于 680×560px 最小面板，用于排除零尺寸布局）
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 Launchpad 通过公开 OpenFeatureGuide action 模拟 Help › Feature Guide…。
///   2. 确认编排首屏无伪媒体，再进入语音群控，暂停、播放一次真实 Demo。
///   3. 开启减少动态效果，确认只留官网海报，再关闭以恢复播放。
///   4. 点击第 3 项验证 Agent 协作，再切到第 5 项并通过主按钮推进到第 6 项。
///   5. 点击 Skip，确认弹窗关闭、视频停止且状态为 suppressed。
///   6. 再次派发 OpenFeatureGuide，确认永久 Help 入口不受 suppressed 限制。
///   7. 点击 Close，确认只关闭当前会话且保留 suppressed 决策。
///   8. 第三次打开后直达最后主题，点击“开始使用”完成导览。
/// 预期结果：
///   - 正断言：overlay 与 panel 均大于 100×100px，且标题、主题按钮、主按钮、Skip、Close 均来自真实 GPUI 渲染树。
///   - 正断言：新主题严格位于 orchestration → voice group control → Agent collaboration，旧主题顺延且 Help 可重复打开。
///   - 正断言：官网海报可见，视频可暂停与重新播放。
///   - 负断言：减少动态效果模式不得继续播放，并必须显示静态状态说明。
///   - 负断言：panel 不得错误扩张成与全屏 overlay 相同的尺寸。
///   - 负断言：Skip 后导览不可见，且重新打开不能把状态误重置为 not_seen。
#[test]
fn e2e_15_feature_guide_help_entry_switches_topics_and_reopens_after_skip() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());

    cx.dispatch_action(OpenFeatureGuide);
    cx.run_until_parked();

    let overlay_bounds = cx
        .debug_bounds("feature-guide-overlay")
        .expect("Help action 必须在真实渲染树中打开 feature-guide-overlay");
    let panel_bounds = cx
        .debug_bounds("feature-guide-panel")
        .expect("导览可见时必须在真实渲染树中创建 feature-guide-panel");
    assert!(
        overlay_bounds.size.width > gpui::px(100.0) && overlay_bounds.size.height > gpui::px(100.0),
        "导览 overlay 必须覆盖非零窗口区域，实际尺寸: {:?}",
        overlay_bounds.size
    );
    assert!(
        panel_bounds.size.width > gpui::px(100.0) && panel_bounds.size.height > gpui::px(100.0),
        "导览 panel 必须具有可见的非零尺寸，实际尺寸: {:?}",
        panel_bounds.size
    );
    assert_ne!(
        overlay_bounds.size, panel_bounds.size,
        "模态 panel 不应错误地扩张成与全屏 overlay 相同的尺寸"
    );
    assert!(
        cx.debug_bounds("feature-guide-title").is_some(),
        "打开导览后必须渲染稳定标题节点供用户定位当前功能"
    );
    let first = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert_eq!(
        first.topic, "agent_orchestration",
        "首次手动打开应从第 1 项 Agent Orchestration 开始"
    );
    assert!(
        first.visible,
        "Help action 后导览必须可见，不能只更新内部状态"
    );
    assert!(
        !first.poster_available && !first.media_playing,
        "编排首屏不得伪造海报或启动产品 Demo"
    );
    click_debug(cx, "feature-guide-topic-1");
    let opened = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert_eq!(
        opened.topic, "voice_group_control",
        "Feature Guide 第 2 项必须是官网 Voice group control Demo"
    );
    assert!(
        opened.poster_available,
        "第 2 项必须解析到官网语音群控 WebP 海报"
    );
    assert!(
        opened.media_playing,
        "允许动态效果时，当前官网 Demo 必须在打开后自动播放"
    );
    assert!(
        cx.debug_bounds("feature-guide-demo-poster").is_some(),
        "官网 WebP 海报必须作为视频首帧回退进入真实 GPUI 渲染树"
    );
    assert!(
        cx.debug_bounds("feature-guide-topic-5").is_some(),
        "编排加五段 Demo 后第 6 个 Agent Notifications 入口必须存在"
    );
    assert!(
        cx.debug_bounds("feature-guide-topic-6").is_none(),
        "六步导览不得继续渲染不存在的第 7 个主题入口"
    );

    click_debug(cx, "feature-guide-playback");
    let paused = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert!(
        !paused.media_playing,
        "点击“暂停演示”后必须停止前台取帧，不能只更换按钮文案"
    );
    assert!(
        paused.visible,
        "暂停只控制演示媒体，不能关闭整个 Feature Guide"
    );
    click_debug(cx, "feature-guide-playback");
    let replayed = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert!(
        replayed.media_playing,
        "点击“播放演示”后必须恢复官网视频播放"
    );

    app.update(cx, |app, cx| {
        app.set_feature_guide_reduce_motion_for_e2e(true, cx)
    });
    cx.refresh()
        .expect("开启减少动态效果后应能请求 Feature Guide 重绘");
    cx.run_until_parked();
    let reduced = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert!(
        !reduced.media_playing,
        "开启减少动态效果后，官网 MP4 必须暂停并只保留静态海报"
    );
    assert!(
        cx.debug_bounds("feature-guide-demo-poster").is_some(),
        "减少动态效果模式仍必须渲染官网 WebP 海报"
    );
    assert!(
        cx.debug_bounds("feature-guide-reduced-motion").is_some(),
        "静态海报模式必须以可见状态说明替代连续动态播放"
    );
    app.update(cx, |app, cx| {
        app.set_feature_guide_reduce_motion_for_e2e(false, cx)
    });
    cx.refresh()
        .expect("关闭减少动态效果后应能请求 Feature Guide 重绘");
    cx.run_until_parked();
    let motion_restored = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert!(
        motion_restored.media_playing,
        "关闭减少动态效果后，未手动暂停的当前官网 Demo 应恢复播放"
    );

    click_debug(cx, "feature-guide-topic-2");
    let collaboration = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert_eq!(
        collaboration.topic, "agent_collaboration",
        "Feature Guide 第 3 项必须是 Agents delegate tasks to each other"
    );
    assert_ne!(
        collaboration.topic, "live_node_shortcuts",
        "插入新 Demo 后第 3 项不得仍是旧 Live Node Shortcuts"
    );

    click_debug(cx, "feature-guide-topic-4");
    let layouts = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert_eq!(
        layouts.topic, "workspace_layouts",
        "点击第 5 个主题应切换到顺延后的官网 Command-M 布局 Demo"
    );
    assert_ne!(
        layouts.topic, "live_node_shortcuts",
        "切换主题后不应仍停留在默认 Live Node 快捷键 Demo"
    );

    click_debug(cx, "feature-guide-primary");
    let notifications = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert_eq!(
        notifications.topic, "agent_notifications",
        "第 5 个主题的主按钮应推进到第 6 个 Agent 通知 Demo"
    );

    click_debug(cx, "feature-guide-skip");
    let skipped = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert_eq!(
        skipped.status, "suppressed",
        "用户点击 Skip 后必须保存 suppressed，而不是 completed"
    );
    assert!(
        !skipped.visible,
        "点击 Skip 后导览必须退出当前渲染层，不能继续遮挡应用"
    );
    assert!(
        !skipped.media_playing,
        "点击 Skip 后必须释放演示视频播放，不能在背景持续解码"
    );
    assert_ne!(
        skipped.status, "not_seen",
        "用户明确跳过后不得回退为 not_seen 导致下次自动弹出"
    );

    cx.dispatch_action(OpenFeatureGuide);
    cx.run_until_parked();
    let reopened = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert!(
        reopened.visible,
        "suppressed 只禁止自动展示，Help 手动入口仍必须可重复打开"
    );
    assert_eq!(
        reopened.topic, "agent_notifications",
        "Help 再次打开应保留用户最后查看的 Agent 通知主题"
    );

    click_debug(cx, "feature-guide-close");
    let closed = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert_eq!(
        closed.presentation, "closed_for_session",
        "Close 只应关闭当前会话的导览表现层"
    );
    assert_eq!(
        closed.status, "suppressed",
        "Close 不得覆盖用户之前明确保存的 suppressed 决策"
    );

    cx.dispatch_action(OpenFeatureGuide);
    cx.run_until_parked();
    click_debug(cx, "feature-guide-topic-5");
    click_debug(cx, "feature-guide-primary");
    let completed = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert_eq!(
        completed.status, "completed",
        "最后主题的“开始使用”必须将导览状态保存为 completed"
    );
    assert!(
        !completed.visible,
        "完成导览后必须回到原应用界面，不能继续保留模态层"
    );
    assert_ne!(
        completed.status, "suppressed",
        "用户明确完成后，最终状态不能继续停留在早先的 suppressed"
    );
}

/// 首次项目自动导览：新用户在 Canvas ready 后看到弹窗，已有默认测试用户不会被打扰。
/// 数据构造（含关键数值的推导过程）：
///   app instances = existing/default(1) + fresh(1) = 2
///   recent workspace count = existing semantic(>=1) / fresh(0)
///   canvas-ready signals = 1 per app
/// 执行过程（逐步说明系统如何处理）：
///   1. 默认 in-memory App 进入 Canvas，保持 suppressed 测试基线。
///   2. 第二个 App 显式注入 fresh eligibility，进入 Canvas 并发出一次 ready 信号。
///   3. 比较两个 App 的导览表现层和状态。
///
/// 预期结果：
///   - 正断言：fresh App 自动进入 visible/in_progress 并渲染 overlay。
///   - 负断言：默认既有用户 App 不显示导览，避免升级后打扰。
#[test]
fn e2e_15_feature_guide_auto_opens_only_for_fresh_first_project() {
    let mut existing_cx = TestAppContext::single();
    let (existing, existing_cx) = make_app(&mut existing_cx, echo_factory());
    let existing_root = tempfile::tempdir().expect("应创建既有用户 Canvas 临时项目");
    enter_canvas(&existing, existing_cx, existing_root.path());
    existing.update(existing_cx, |app, cx| {
        app.present_feature_guide_after_canvas_ready_for_e2e(cx)
    });
    existing_cx.run_until_parked();
    let existing_snapshot = existing.update(existing_cx, |app, _| app.feature_guide_e2e_snapshot());

    assert!(
        !existing_snapshot.visible,
        "默认既有用户测试基线必须保持不自动展示导览"
    );
    assert_eq!(
        existing_snapshot.status, "suppressed",
        "既有用户的自动展示抑制状态不能被 Canvas ready 改写"
    );

    let mut fresh_cx = TestAppContext::single();
    let (fresh, fresh_cx) = make_app(&mut fresh_cx, echo_factory());
    let fresh_root = tempfile::tempdir().expect("应创建新用户首个 Canvas 临时项目");
    fresh.update(fresh_cx, |app, _| app.enable_fresh_feature_guide_for_e2e());
    enter_canvas(&fresh, fresh_cx, fresh_root.path());
    fresh.update(fresh_cx, |app, cx| {
        app.present_feature_guide_after_canvas_ready_for_e2e(cx)
    });
    fresh_cx.run_until_parked();
    let fresh_snapshot = fresh.update(fresh_cx, |app, _| app.feature_guide_e2e_snapshot());

    assert!(
        fresh_snapshot.visible,
        "无最近项目的新用户在首个 Canvas ready 后必须自动看到导览"
    );
    assert_eq!(
        fresh_snapshot.status, "in_progress",
        "首次自动展示必须写入 in_progress，便于未完成时恢复"
    );
    assert!(
        fresh_cx.debug_bounds("feature-guide-overlay").is_some(),
        "fresh 自动展示不能只改变状态，必须在真实 GPUI 树渲染 overlay"
    );
    assert_ne!(
        fresh_snapshot.status, "completed",
        "仅显示首屏不能误判为用户已完成整个导览"
    );
}

/// 导览设置持久化：Skip 和最后主题必须由后台任务写入 app SettingsStore。
///
/// 数据构造（含关键数值的推导过程）：
///   Settings keys = status(1) + topic(1) = 2
///   selected topic index = 5（agent_notifications）
///   user terminal decision = Skip(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用隔离 SettingsStore 构造 Stored 模式 App。
///   2. 从 Help 打开导览，点击第 6 个主题并点击 Skip。
///   3. 等待 GPUI 后台持久化任务停稳，直接重新读取两个稳定 key。
///
/// 预期结果：
///   - 正断言：status 精确写入 suppressed，topic 精确写入 agent_notifications。
///   - 负断言：Skip 不得写成 completed，主题也不能回退为 live_node_shortcuts。
#[test]
fn e2e_15_feature_guide_skip_persists_status_and_last_topic() {
    let mut cx = TestAppContext::single();
    let temp = tempfile::tempdir().expect("应创建导览设置持久化临时目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let (_app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), store.clone());

    cx.dispatch_action(OpenFeatureGuide);
    cx.run_until_parked();
    click_debug(cx, "feature-guide-topic-5");
    click_debug(cx, "feature-guide-skip");
    cx.run_until_parked();

    let saved_status = store
        .get_string("feature_guide.status.v1")
        .expect("应能从隔离 app Settings 读取导览状态");
    let saved_topic = store
        .get_string("feature_guide.topic.v1")
        .expect("应能从隔离 app Settings 读取导览主题");

    assert_eq!(
        saved_status.as_deref(),
        Some("suppressed"),
        "Skip 必须通过后台持久化精确写入 suppressed"
    );
    assert_eq!(
        saved_topic.as_deref(),
        Some("agent_notifications"),
        "持久化必须保留用户最后查看的第 6 个 Agent 通知主题"
    );
    assert_ne!(
        saved_status.as_deref(),
        Some("completed"),
        "Skip 路径不得错误写成 completed"
    );
    assert_ne!(
        saved_topic.as_deref(),
        Some("live_node_shortcuts"),
        "用户已切换主题后，持久化不能回退到默认 Live Node 快捷键主题"
    );
}

/// 导览键盘闭环：Tab 必须聚焦模态主按钮，Enter 只推进一个主题。
///
/// 数据构造（含关键数值的推导过程）：
///   initial focused element = app root（导览打开时恢复到 MultisoulApp focus handle）
///   focus traversal = Tab × 1 → tab_index(1) 的主操作“下一项”
///   focus traversal = Tab × 1 → primary_focused = true
///   primary activation = Enter × 1 → topic index 0 + 1 = 1（voice_group_control）
///   background actions = 0（模态层不能把按键送入 Canvas 或 PTY）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从 Help action 打开导览并确认状态为 Visible。
///   2. 模拟 `tab` 并读取主按钮 FocusHandle，证明焦点实际进入 tab_index(1) 控件。
///   3. 模拟 `enter`，确认焦点控件只推进一个主题并保留 suppressed 决策。
///
/// 预期结果：
///   - 正断言：Tab 聚焦主按钮，Enter 切换到 voice_group_control，导览仍可见。
///   - 正断言：键盘推进保留 suppressed，不误标为 completed。
///   - 负断言：主题不能仍是 agent_orchestration，也不能跳过一页到 agent_collaboration。
#[test]
fn e2e_15_feature_guide_tab_and_arrow_keys_stay_inside_modal() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());

    cx.dispatch_action(OpenFeatureGuide);
    cx.run_until_parked();
    let opened = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert!(
        opened.visible,
        "键盘闭环测试开始前导览必须可见，否则无法验证模态焦点"
    );

    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    let focused = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert!(
        focused.primary_focused,
        "一次 Tab 必须把真实 GPUI 焦点移入 tab_index(1) 的导览主按钮"
    );

    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    let advanced = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert_eq!(
        advanced.topic, "voice_group_control",
        "主按钮已聚焦时 Enter 必须激活模态主操作并推进到第 2 个语音群控主题"
    );
    assert_eq!(
        advanced.status, "suppressed",
        "既有用户从 Help 手动打开并键盘推进时，不能覆盖原有 suppressed 自动展示决策"
    );
    assert!(
        advanced.visible,
        "键盘推进后导览应继续 visible，供用户继续浏览剩余主题"
    );
    assert_ne!(
        advanced.topic, "agent_orchestration",
        "键盘操作后不得仍停在首页，否则 Enter 未激活已聚焦的模态主按钮"
    );
    assert_ne!(
        advanced.topic, "agent_collaboration",
        "一次 Enter 只能推进一个主题，不能重复激活并跳过第 2 页"
    );
}

fn click_debug(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("点击前应找到 {selector} 的真实 debug bounds"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}
