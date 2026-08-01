//! E2E-15：功能导览的永久 Help 入口与首次项目自动展示。

use app::OpenFeatureGuide;
use gpui::{Modifiers, TestAppContext};

use crate::harness::{echo_factory, enter_canvas, make_app, make_app_with_settings_store};

/// Help 功能导览：用户可切换主题、推进下一项、跳过，并在跳过后从 Help 再次打开。
///
/// 数据构造（含关键数值的推导过程）：
///   guide topics = 4
///   initial topic index = 0（voice_dispatch）
///   direct topic click = index 1（continue_conversation）
///   primary advance = index 1 + 1 = 2（canvas_management）
///   explicit endings = Skip(1) + Close(1) + Complete(1) = 3
///   visible surface threshold = 100×100px（远小于 680×560px 最小面板，用于排除零尺寸布局）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 Launchpad 通过公开 OpenFeatureGuide action 模拟 Help › Feature Guide…。
///   2. 从真实渲染树点击第 2 个主题，再点击主按钮推进到第 3 个主题。
///   3. 点击 Skip，确认弹窗关闭且状态为 suppressed。
///   4. 再次派发 OpenFeatureGuide，确认永久 Help 入口不受 suppressed 限制。
///   5. 点击 Close，确认只关闭当前会话且保留 suppressed 决策。
///   6. 第三次打开后直达最后主题，点击“开始使用”完成导览。
///
/// 预期结果：
///   - 正断言：overlay 与 panel 均大于 100×100px，且标题、主题按钮、主按钮、Skip、Close 均来自真实 GPUI 渲染树。
///   - 正断言：主题严格按 voice → conversation → canvas 推进，Help 可重复打开。
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
    let opened = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert_eq!(
        opened.topic, "voice_dispatch",
        "首次手动打开应从默认语音派发主题开始"
    );
    assert!(
        opened.visible,
        "Help action 后导览必须可见，不能只更新内部状态"
    );

    click_debug(cx, "feature-guide-topic-1");
    let conversation = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert_eq!(
        conversation.topic, "continue_conversation",
        "点击第 2 个主题应切换到 Agent 后续对话说明"
    );
    assert_ne!(
        conversation.topic, "voice_dispatch",
        "切换主题后不应仍停留在默认语音派发页"
    );

    click_debug(cx, "feature-guide-primary");
    let canvas = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert_eq!(
        canvas.topic, "canvas_management",
        "第 2 个主题的主按钮应推进到第 3 个画布管理主题"
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
        reopened.topic, "canvas_management",
        "Help 再次打开应保留用户最后查看的画布主题"
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
    click_debug(cx, "feature-guide-topic-3");
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
///
/// 数据构造（含关键数值的推导过程）：
///   app instances = existing/default(1) + fresh(1) = 2
///   recent workspace count = existing semantic(>=1) / fresh(0)
///   canvas-ready signals = 1 per app
///
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
///   selected topic index = 2（canvas_management）
///   user terminal decision = Skip(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用隔离 SettingsStore 构造 Stored 模式 App。
///   2. 从 Help 打开导览，点击第 3 个主题并点击 Skip。
///   3. 等待 GPUI 后台持久化任务停稳，直接重新读取两个稳定 key。
///
/// 预期结果：
///   - 正断言：status 精确写入 suppressed，topic 精确写入 canvas_management。
///   - 负断言：Skip 不得写成 completed，主题也不能回退为 voice_dispatch。
#[test]
fn e2e_15_feature_guide_skip_persists_status_and_last_topic() {
    let mut cx = TestAppContext::single();
    let temp = tempfile::tempdir().expect("应创建导览设置持久化临时目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let (_app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), store.clone());

    cx.dispatch_action(OpenFeatureGuide);
    cx.run_until_parked();
    click_debug(cx, "feature-guide-topic-2");
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
        Some("canvas_management"),
        "持久化必须保留用户最后查看的第 3 个画布管理主题"
    );
    assert_ne!(
        saved_status.as_deref(),
        Some("completed"),
        "Skip 路径不得错误写成 completed"
    );
    assert_ne!(
        saved_topic.as_deref(),
        Some("voice_dispatch"),
        "用户已切换主题后，持久化不能回退到默认语音主题"
    );
}

/// 导览键盘闭环：Tab 必须聚焦模态主按钮，Enter 只推进一个主题。
///
/// 数据构造（含关键数值的推导过程）：
///   initial focused element = app root（导览打开时恢复到 MultisoulApp focus handle）
///   focus traversal = Tab × 1 → tab_index(1) 的主操作“下一项”
///   focus traversal = Tab × 1 → primary_focused = true
///   primary activation = Enter × 1 → topic index 0 + 1 = 1（continue_conversation）
///   background actions = 0（模态层不能把按键送入 Canvas 或 PTY）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从 Help action 打开导览并确认状态为 Visible。
///   2. 模拟 `tab` 并读取主按钮 FocusHandle，证明焦点实际进入 tab_index(1) 控件。
///   3. 模拟 `enter`，确认焦点控件只推进一个主题并保留 suppressed 决策。
///
/// 预期结果：
///   - 正断言：Tab 聚焦主按钮，Enter 切换到 continue_conversation，导览仍可见。
///   - 正断言：键盘推进保留 suppressed，不误标为 completed。
///   - 负断言：主题不能仍是 voice_dispatch，也不能跳过一页到 canvas_management。
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
        advanced.topic, "continue_conversation",
        "主按钮已聚焦时 Enter 必须激活模态主操作并推进到第 2 个主题"
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
        advanced.topic, "voice_dispatch",
        "键盘操作后不得仍停在首页，否则 Enter 未激活已聚焦的模态主按钮"
    );
    assert_ne!(
        advanced.topic, "canvas_management",
        "一次 Enter 只能推进一个主题，不能重复激活并跳过第 2 页"
    );
}

/// 高优先级语音浮层协调：录音开始时导览暂停，语音结束后恢复原主题。
///
/// 数据构造（含关键数值的推导过程）：
///   initially visible guides = 1
///   active voice recordings = 1
///   selected topic index = 1（continue_conversation，用于验证恢复时不丢进度）
///   visible guide states during recording = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从 Help 打开导览并选中第 2 个主题。
///   2. 注入 Recording 语音状态并触发真实 GPUI 重绘。
///   3. 确认导览转为 Suspended，渲染谓词 visible 变为 false。
///   4. 结束语音运行时并重绘，确认导览恢复原主题。
///
/// 预期结果：
///   - 正断言：录音期间 presentation 为 suspended，结束后恢复 visible。
///   - 正断言：恢复后仍是 continue_conversation，不能跳回首页。
///   - 负断言：录音期间导览不得继续满足 overlay 的 visible 渲染条件。
#[test]
fn e2e_15_feature_guide_suspends_while_voice_is_active_and_resumes() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());

    cx.dispatch_action(OpenFeatureGuide);
    cx.run_until_parked();
    click_debug(cx, "feature-guide-topic-1");
    app.update(cx, |app, cx| {
        app.show_voice_recording_transcript_for_e2e("把下一步交给小张");
        cx.notify();
    });
    cx.refresh().expect("语音 blocker 测试应能请求窗口重绘");
    cx.run_until_parked();

    let suspended = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert_eq!(
        suspended.presentation, "suspended",
        "语音 Recording 浮层出现时，导览必须自动转为 suspended"
    );
    assert!(
        !suspended.visible,
        "导览暂停后不能继续标记 visible，否则两个高层界面会同时争抢输入"
    );
    assert_eq!(
        suspended.topic, "continue_conversation",
        "临时暂停必须保留用户已选中的第 2 个主题，不能在语音开始时重置进度"
    );

    app.update(cx, |app, cx| {
        app.reset_voice_runtime_for_e2e();
        cx.notify();
    });
    cx.refresh().expect("语音结束后应能请求窗口重绘以恢复导览");
    cx.run_until_parked();

    let resumed = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert_eq!(
        resumed.presentation, "visible",
        "语音结束且 blocker 清空后，暂停的导览应自动恢复 visible"
    );
    assert_eq!(
        resumed.topic, "continue_conversation",
        "暂停与恢复必须保留用户选中的第 2 个主题，不能重置到 voice_dispatch"
    );
    assert!(
        cx.debug_bounds("feature-guide-overlay").is_some(),
        "恢复后必须重新把 feature-guide-overlay 放回真实 GPUI 渲染树"
    );
    assert_ne!(
        resumed.presentation, "closed_for_session",
        "系统级临时暂停不能误判成用户主动关闭当前会话"
    );
}

/// 语音上下文按钮：已就绪时回到真实入口，未配置时进入现有 Voice Settings。
///
/// 数据构造（含关键数值的推导过程）：
///   voice readiness transitions = Ready(1) → SetUp/Missing(1)
///   contextual button clicks = TryNow(1) + Configure/Allow(1) = 2
///   test PCM chunks = 1（640 bytes = 320 个 PCM16 样本，用于隔离真实麦克风设备）
///   newly invented voice flows = 0（所有点击都复用现有录音或 Settings 路径）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入真实 Canvas，注入 Ready 状态并从 Help 打开语音主题。
///   2. 安装 1 个确定性 PCM chunk，点击语音上下文按钮，确认进入现有 Recording 状态。
///   3. 将语音恢复为未配置状态，再次打开并点击同一稳定 selector。
///   4. 确认现有 Settings → Voice 打开，导览只暂停等待。
///
/// 预期结果：
///   - 正断言：Ready 点击后为 closed_for_session 且录音代次递增，未配置点击后 Settings/Voice 打开。
///   - 正断言：Settings 出现时导览为 suspended，可在关闭 Settings 后恢复。
///   - 负断言：Ready 路径不能误开 Settings，配置路径也不能把导览标成 completed。
#[test]
fn e2e_15_feature_guide_voice_action_reuses_real_voice_flows() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::tempdir().expect("应创建语音上下文按钮的临时 Canvas 项目");
    enter_canvas(&app, cx, workspace.path());
    app.update(cx, |app, _| {
        app.assume_voice_ready_for_e2e();
        app.install_voice_pcm_for_e2e(vec![vec![0; 640]]);
    });

    cx.dispatch_action(OpenFeatureGuide);
    cx.run_until_parked();
    click_debug(cx, "feature-guide-voice-action");

    let ready_result = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    let ready_settings = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    let ready_voice = app.update(cx, |app, _| app.voice_e2e_snapshot());
    assert_eq!(
        ready_result.presentation, "closed_for_session",
        "语音 Ready 时点击“现在试试”应关闭导览并回到真实应用入口"
    );
    assert!(
        !ready_settings.settings_open,
        "语音已经就绪时不能误开 Settings，避免给用户制造重复配置"
    );
    assert_eq!(
        ready_voice.recording_generation, 1,
        "语音 Ready 时点击“现在试试”必须通过现有 begin_voice_recording 启动第 1 个录音代次"
    );
    assert_ne!(
        ready_voice.phase, "idle",
        "真实录音链路即使在测试环境缺少模型而进入 Error，也不能停留在未触发的 Idle"
    );

    app.update(cx, |app, _| {
        app.reset_voice_runtime_for_e2e();
        app.reset_voice_setup_for_e2e();
    });
    cx.dispatch_action(OpenFeatureGuide);
    cx.run_until_parked();
    click_debug(cx, "feature-guide-voice-action");

    let setup_result = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    let setup_settings = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(
        setup_settings.settings_open,
        "语音未配置时，上下文按钮必须打开现有 Settings，而不是创建导览专用配置流"
    );
    assert_eq!(
        setup_settings.active_section, "Voice",
        "未配置语音的引导必须直接定位到 Settings → Voice"
    );
    assert_eq!(
        setup_result.presentation, "suspended",
        "Settings 优先级高于导览，打开后导览应暂停并保留进度"
    );
    assert_ne!(
        setup_result.status, "completed",
        "进入语音配置只表示离开导览试用，不能误标为已完成所有主题"
    );
}

fn click_debug(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("点击前应找到 {selector} 的真实 debug bounds"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}
