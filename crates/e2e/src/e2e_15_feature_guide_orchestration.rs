//! E2E-15：Feature Guide 编排安装教育、Settings 往返和 Help 后续入口。

use app::OpenFeatureGuide;
use gpui::{Modifiers, TestAppContext};
use std::time::Duration;

use crate::harness::{echo_factory, enter_canvas, make_app};

/// E2E-FEATURE-GUIDE-ORCH-01：编排首屏只读展示共享状态，并通过唯一 CTA 往返 Settings。
///
/// 数据构造（含关键数值的推导过程）：
///   guide topics       = orchestration(1) + website demos(5) = 6
///   guided providers  = Codex(1) + Claude(1) + Cursor(1) + OpenCode(1) = 4
///   canonical statuses = Ready(2) + Not installed(1) + Needs repair(1) = 4
///   settings roundtrips = open(1) + close(1) = 2 transitions
///   direct install actions inside guide = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 注入共享编排健康快照，从 Help 打开 Feature Guide。
///   2. 验证编排是第 1 / 6 项、四个 Provider 状态只读可见，且没有媒体或安装按钮。
///   3. 展开再收起组件说明，确认辅助教育入口有真实交互。
///   4. 点击 Open Orchestration Settings，验证导览暂停且 Settings 精确打开编排页。
///   5. 模拟 Settings 刷新共享状态并关闭面板，验证导览恢复同一主题和最新状态。
///   6. 点击 Next 进入语音群控，再回到编排页 Skip；最后通过 Help 重新进入。
///
/// 预期结果：
///   - 正断言：编排首屏、四个状态、说明展开、Settings 安装入口、暂停恢复和 Next 均可操作。
///   - 正断言：关闭 Settings 后仍在 agent_orchestration，Help 在 Skip 后仍可再次打开。
///   - 负断言：编排首屏不得展示 Grok、伪造 Demo、播放控件或第二套 Install 行为。
///   - 负断言：Settings 往返不得关闭、重置或完成 Feature Guide。
#[test]
fn e2e_15_feature_guide_orchestration_roundtrips_settings_and_reopens_from_help() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::tempdir().expect("应创建编排导览 Settings 往返临时 Canvas 项目");
    enter_canvas(&app, cx, workspace.path());
    cx.update(|_, cx| app::mark_orchestration_health_ready_for_e2e(cx));
    app.update(cx, |app, cx| {
        app.set_feature_guide_reduce_motion_for_e2e(true, cx)
    });

    cx.dispatch_action(OpenFeatureGuide);
    cx.run_until_parked();

    let opened = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert_eq!(
        opened.topic, "agent_orchestration",
        "Help 首次打开必须进入第 1 项 Agent Orchestration"
    );
    assert!(
        opened.visible,
        "Agent Orchestration 首屏必须进入真实可见渲染状态"
    );
    assert_eq!(
        (opened.topic_position, opened.topic_count),
        (1, 6),
        "编排首屏必须向用户显示稳定的 1 / 6 进度"
    );
    assert!(
        cx.debug_bounds("feature-guide-progress").is_some(),
        "六步进度必须进入真实 GPUI 渲染树供辅助技术读取"
    );
    assert!(
        cx.debug_bounds("feature-guide-reduced-motion-crossfade")
            .is_some(),
        "Reduced Motion 下 Feature Guide 必须走短淡入淡出分支而非空间位移动画"
    );
    assert!(
        cx.debug_bounds("feature-guide-orchestration-badge-set-up")
            .is_some(),
        "四个 Provider 均未安装时导航 badge 必须显示 Set up"
    );
    assert!(
        !opened.media_playing,
        "编排说明没有产品 Demo，不得在后台启动视频解码"
    );
    assert!(
        !opened.poster_available,
        "编排说明不得伪造或复用其他主题的海报"
    );
    assert!(
        cx.debug_bounds("feature-guide-topic-5").is_some(),
        "加入两段新 Demo 后第 6 项 Agent Notifications 必须仍然存在"
    );
    assert!(
        cx.debug_bounds("feature-guide-topic-6").is_none(),
        "六步导览不得渲染不存在的第 7 项"
    );
    assert!(
        cx.debug_bounds("feature-guide-orchestration-status-codex-not-installed")
            .is_some(),
        "刷新前 Codex 的共享 Not installed 状态必须出现在只读 Provider 列表"
    );
    assert!(
        cx.debug_bounds("feature-guide-orchestration-status-claude-not-installed")
            .is_some(),
        "Claude Code 的共享 Not installed 状态必须出现在只读 Provider 列表"
    );
    assert!(
        cx.debug_bounds("feature-guide-orchestration-status-cursor-not-installed")
            .is_some(),
        "刷新前 Cursor Agent CLI 的 Not installed 状态必须出现在只读 Provider 列表"
    );
    assert!(
        cx.debug_bounds("feature-guide-orchestration-status-opencode-not-installed")
            .is_some(),
        "刷新前 OpenCode 的共享 Not installed 状态必须出现在只读 Provider 列表"
    );
    assert!(
        cx.debug_bounds("feature-guide-orchestration-provider-grok")
            .is_none(),
        "首版不支持安装的 Grok 不得进入 Feature Guide Provider 列表"
    );
    assert!(
        cx.debug_bounds("feature-guide-demo-media").is_none(),
        "编排首屏不得占用伪造的 Demo 媒体区域"
    );
    assert!(
        cx.debug_bounds("feature-guide-playback").is_none(),
        "编排首屏没有媒体时不得显示无效播放控件"
    );
    assert!(
        cx.debug_bounds("settings-orchestration-codex-action")
            .is_none(),
        "Feature Guide 内不得复制 Settings 的 Install 或 Uninstall 行为"
    );

    click_debug(cx, "feature-guide-orchestration-details");
    cx.refresh()
        .expect("展开编排组件说明后应能请求 Feature Guide 重绘");
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("feature-guide-orchestration-components-explanation")
            .is_some(),
        "点击组件说明入口后必须渲染 Skill、CLI、Hook 与授权的解释"
    );
    assert!(
        cx.debug_bounds("feature-guide-orchestration-details-expanded")
            .is_some(),
        "说明展开后辅助按钮必须提供明确的收起状态"
    );
    click_debug(cx, "feature-guide-orchestration-details");
    cx.refresh()
        .expect("收起编排组件说明后应能请求 Feature Guide 重绘");
    cx.run_until_parked();
    let collapsed = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert!(
        !collapsed.orchestration_details_expanded,
        "第二次点击组件说明入口必须把 Feature Guide 状态切回收起"
    );
    assert!(
        cx.debug_bounds("feature-guide-orchestration-details-collapsed")
            .is_some(),
        "再次点击组件说明入口后必须恢复可展开状态"
    );

    click_debug(cx, "feature-guide-orchestration-open-settings");
    cx.refresh()
        .expect("打开 Orchestration Settings 后应能请求窗口重绘");
    cx.run_until_parked();
    let suspended = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    let settings = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        suspended.presentation, "suspended",
        "Settings 打开时 Feature Guide 必须进入 Suspended，而不是被关闭"
    );
    assert!(
        !suspended.visible,
        "Settings 与 Feature Guide 不得同时争抢最上层输入"
    );
    assert_eq!(
        suspended.topic, "agent_orchestration",
        "打开 Settings 时必须保留当前编排主题进度"
    );
    assert!(
        settings.settings_open,
        "Open Orchestration Settings 必须真实打开 Settings 面板"
    );
    assert!(
        cx.debug_bounds("settings-reduced-motion-crossfade")
            .is_some(),
        "Reduced Motion 下 Settings 必须使用短淡入淡出，不能弹跳或大范围位移"
    );
    assert_eq!(
        settings.active_section, "Orchestration",
        "CTA 必须精确选择 Settings 的 Orchestration 页面"
    );
    assert!(
        cx.debug_bounds("settings-orchestration-codex-action")
            .is_some(),
        "真实 Install/Repair/Uninstall 操作必须只在 Settings 页面出现"
    );
    // reduced_fade = MotionTokens::default().reduced_fade = 160ms；推进完整时长，
    // Opening crossfade 才能结束并进入稳定的 Settings-only 阶段。
    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(160));
    cx.run_until_parked();

    cx.update(|_, cx| app::publish_canonical_orchestration_health_for_e2e(cx));
    cx.run_until_parked();
    click_debug(cx, "settings-close");

    let closing = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    let closing_settings = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        closing.presentation, "suspended",
        "Closing crossfade 完成前导览状态仍应保持 Suspended"
    );
    assert!(
        closing_settings.settings_open,
        "Closing crossfade 的 160ms 内必须保留 Settings 旧层用于淡出"
    );
    assert!(
        cx.debug_bounds("feature-guide-reduced-motion-crossfade")
            .is_some()
            && cx
                .debug_bounds("settings-reduced-motion-crossfade")
                .is_some(),
        "Closing crossfade 期间 Guide 与 Settings 两层必须同时进入渲染树"
    );
    // Closing 使用同一 reduced_fade = 160ms；完整推进后才关闭 Settings、清 blocker。
    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(160));
    cx.run_until_parked();

    let resumed = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    let refreshed = app.update(cx, |app, _| app.orchestration_settings_e2e_snapshot());
    assert_eq!(
        resumed.presentation, "visible",
        "Settings 关闭且 blocker 清空后 Feature Guide 必须自动恢复"
    );
    assert_eq!(
        resumed.topic, "agent_orchestration",
        "Settings 往返后必须恢复到原来的 Agent Orchestration 主题"
    );
    assert!(
        cx.debug_bounds("feature-guide-reduced-motion-crossfade")
            .is_some(),
        "关闭 Settings 后恢复的 Feature Guide 必须再次使用 Reduced Motion 淡入"
    );
    assert!(
        cx.debug_bounds("feature-guide-orchestration-status-cursor-needs-repair")
            .is_some(),
        "恢复后的 Provider 列表必须读取 Settings 刷新的最新共享状态"
    );
    assert_eq!(
        refreshed.providers[0].status, "Ready",
        "Settings 刷新后 Codex 必须从 Not installed 变为 Ready"
    );
    assert_eq!(
        refreshed.providers[2].status, "Needs repair",
        "Settings 刷新后 Cursor 必须变为 Needs repair，不能保留旧快照"
    );
    assert_ne!(
        resumed.status, "completed",
        "仅完成 Settings 往返不得把整个六步导览误标为 completed"
    );

    click_debug(cx, "feature-guide-primary");
    let advanced = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert_eq!(
        advanced.topic, "voice_group_control",
        "编排首屏的 Next 必须只推进到第 2 项 Voice Group Control"
    );
    assert_ne!(
        advanced.topic, "workspace_layouts",
        "一次 Next 不得跨过 Live Node Shortcuts 跳到第 3 项"
    );

    click_debug(cx, "feature-guide-topic-0");
    click_debug(cx, "feature-guide-skip");
    let skipped = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert_eq!(
        skipped.status, "suppressed",
        "编排首屏的 Skip Guide 必须沿用完整导览的 suppressed 语义"
    );
    assert!(
        !skipped.visible,
        "Skip Guide 后 Feature Guide 必须退出渲染层"
    );

    cx.dispatch_action(OpenFeatureGuide);
    cx.run_until_parked();
    let reopened = app.update(cx, |app, _| app.feature_guide_e2e_snapshot());
    assert!(
        reopened.visible,
        "Skip 只关闭自动引导，Help 的永久后续入口必须仍可重新打开"
    );
    assert_eq!(
        reopened.topic, "agent_orchestration",
        "Help 再次进入必须恢复用户最后停留的编排主题"
    );
}

/// E2E-FEATURE-GUIDE-ORCH-02：编排 CTA 与组件说明必须能由 Tab 配合 Enter/Space 激活。
///
/// 数据构造（含关键数值的推导过程）：
///   CTA tab position = primary(1) + topic buttons(6) + CTA(1) = 8
///   details tab position = primary(1) + topic buttons(6) + CTA(1) + details(1) = 9
///   activation keys = Enter(1) + Space(1) = 2
///   direct mouse activations for target controls = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在第一份真实 GPUI 窗口打开编排首屏，连续按 8 次 Tab，再按 Enter。
///   2. 验证 CTA 经键盘打开 Settings → Orchestration，并让导览进入 Suspended。
///   3. 在第二份隔离窗口打开编排首屏，连续按 9 次 Tab，再按 Space。
///   4. 验证组件说明经键盘展开，且导览仍停留在编排首屏。
///
/// 预期结果：
///   - 正断言：Enter 打开准确 Settings 页面，Space 展开组件说明。
///   - 正断言：键盘路径保留 Settings 暂停和主题进度语义。
///   - 负断言：键盘激活不得推进到 Live Node，也不得直接触发 Provider 安装。
#[test]
fn e2e_15_feature_guide_orchestration_controls_support_keyboard_activation() {
    let mut settings_cx = TestAppContext::single();
    let (settings_app, settings_cx) = make_app(&mut settings_cx, echo_factory());
    let settings_workspace = tempfile::tempdir().expect("应创建编排 CTA 键盘激活临时 Canvas 项目");
    enter_canvas(&settings_app, settings_cx, settings_workspace.path());
    settings_cx.dispatch_action(OpenFeatureGuide);
    settings_cx.run_until_parked();

    settings_cx.simulate_keystrokes("tab");
    settings_cx.simulate_keystrokes("tab");
    settings_cx.simulate_keystrokes("tab");
    settings_cx.simulate_keystrokes("tab");
    settings_cx.simulate_keystrokes("tab");
    settings_cx.simulate_keystrokes("tab");
    settings_cx.simulate_keystrokes("tab");
    settings_cx.simulate_keystrokes("tab");
    settings_cx.simulate_keystrokes("enter");
    settings_cx.run_until_parked();

    let suspended = settings_app.update(settings_cx, |app, _| app.feature_guide_e2e_snapshot());
    let settings = settings_app.update(settings_cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(
        settings.settings_open,
        "第 8 个 Tab stop 上按 Enter 必须打开 Orchestration Settings"
    );
    assert_eq!(
        settings.active_section, "Orchestration",
        "编排 CTA 的键盘路径必须精确选择 Orchestration 页面"
    );
    assert_eq!(
        suspended.presentation, "suspended",
        "键盘打开 Settings 时导览必须与鼠标路径一致进入 Suspended"
    );
    assert_ne!(
        suspended.topic, "voice_group_control",
        "Enter 激活编排 CTA 不得误触底部 Next 推进主题"
    );

    let mut details_cx = TestAppContext::single();
    let (details_app, details_cx) = make_app(&mut details_cx, echo_factory());
    let details_workspace =
        tempfile::tempdir().expect("应创建编排组件说明键盘激活临时 Canvas 项目");
    enter_canvas(&details_app, details_cx, details_workspace.path());
    details_cx.dispatch_action(OpenFeatureGuide);
    details_cx.run_until_parked();

    details_cx.simulate_keystrokes("tab");
    details_cx.simulate_keystrokes("tab");
    details_cx.simulate_keystrokes("tab");
    details_cx.simulate_keystrokes("tab");
    details_cx.simulate_keystrokes("tab");
    details_cx.simulate_keystrokes("tab");
    details_cx.simulate_keystrokes("tab");
    details_cx.simulate_keystrokes("tab");
    details_cx.simulate_keystrokes("tab");
    details_cx.simulate_keystrokes("space");
    details_cx.run_until_parked();

    let expanded = details_app.update(details_cx, |app, _| app.feature_guide_e2e_snapshot());
    assert!(
        expanded.orchestration_details_expanded,
        "第 9 个 Tab stop 上按 Space 必须展开组件说明"
    );
    assert_eq!(
        expanded.topic, "agent_orchestration",
        "键盘展开组件说明后必须保留在 Agent Orchestration 首屏"
    );
    assert!(
        details_cx
            .debug_bounds("feature-guide-orchestration-components-explanation")
            .is_some(),
        "Space 激活后必须真实渲染组件说明内容"
    );
    assert!(
        details_cx
            .debug_bounds("settings-orchestration-codex-action")
            .is_none(),
        "组件说明键盘入口不得在 Feature Guide 内生成直接安装操作"
    );
}

fn click_debug(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("点击前应找到 {selector} 的真实 debug bounds"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}
