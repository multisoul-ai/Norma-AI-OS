//! Built-in TTS Provider Settings user-flow acceptance.

use crate::harness::{
    create_codex_agent, echo_factory, enter_canvas, make_app, make_app_with_settings_store,
    read_runtime, VoiceModelInstallE2eGuard,
};
use gpui::{
    point, px, Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext,
};

mod network_resilience;
mod voice_list;

/// E2E-TTS-PICKER-1：Spoken Replies 默认只显示 Active 摘要，Change 后才渐进披露统一选择器。
///
/// 数据构造（含关键数值的推导过程）：
///   default TTS surfaces = 1（Current Voice 摘要）
///   built-in Provider entries = 2（On-device + MiniMax）
///   unrelated controls = 3（Agent announcements + window inset + input device）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开已就绪的 Voice Settings，停留在日常配置页面。
///   2. 检查 Spoken Replies 当前声音摘要，并确认 Provider 选择器默认隐藏。
///   3. 点击 Change Voice，等待统一选择器绘制 On-device 与 MiniMax 两个入口。
///   4. 检查 Voice 其余配置仍在同一 Settings 页面可达。
///
/// 预期结果：
///   - 正断言：默认页显示 Current Voice 与 Change，打开后显示统一 Provider 选择器。
///   - 正断言：On-device 与 MiniMax 为同级入口，非 TTS 配置仍存在。
///   - 负断言：默认页不得铺开 API Key、Voice 目录或旧的 Provider 按钮墙。
#[test]
fn e2e_spoken_replies_progressively_discloses_unified_tts_picker() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::tempdir().expect("应创建 Spoken Replies 选择器 e2e workspace");
    app.update(cx, |app, cx| app.assume_minimax_connected_for_e2e(cx));
    enter_canvas(&app, cx, workspace.path());
    cx.simulate_keystrokes("cmd-,");
    click(cx, "settings-section-voice");

    scroll_and_assert_visible(cx, "settings-tts-current-voice");
    scroll_and_assert_visible(cx, "settings-tts-change-voice");
    assert!(
        cx.debug_bounds("settings-tts-picker").is_none(),
        "默认 Spoken Replies 页面不得提前展开统一 TTS 选择器"
    );
    assert!(
        cx.debug_bounds("settings-voice-field-tts-minimax-api-key")
            .is_none(),
        "默认 Spoken Replies 页面不得暴露 MiniMax API Key"
    );
    assert!(
        cx.debug_bounds("settings-tts-provider-local").is_none(),
        "默认页面不得保留旧的 On-device Provider 按钮墙"
    );

    click(cx, "settings-tts-change-voice");

    scroll_and_assert_visible(cx, "settings-tts-picker");
    scroll_and_assert_visible(cx, "settings-tts-picker-provider-local");
    scroll_and_assert_visible(cx, "settings-tts-picker-provider-minimax");
    assert!(
        cx.debug_bounds("settings-tts-picker-current-notice")
            .is_some(),
        "统一选择器必须明确说明当前声音在 Apply 前继续生效"
    );
    assert!(
        cx.debug_bounds("settings-agent-task-announcements-toggle")
            .is_some(),
        "重构 Spoken Replies 不得移除 Agent task announcements"
    );
    assert!(
        cx.debug_bounds("settings-voice-window-inset-toggle")
            .is_some(),
        "重构 Spoken Replies 不得移除 Voice-driven window inset"
    );
    assert!(
        cx.debug_bounds("settings-voice-input-device-picker")
            .is_some(),
        "重构 Spoken Replies 不得影响 Input device 配置入口"
    );
}

/// E2E-TTS-PROVIDER-1：MiniMax Draft 必须支持目录搜索、手填 Voice ID、Preview 校验与显式激活。
///
/// 数据构造（含关键数值的推导过程）：
///   built-in providers = 2（On-device + MiniMax）
///   catalog groups = 3（System + Cloned + Generated）
///   catalog Voices = 1 + 1 + 1 = 3
///   manual Voice edits = 1（external_voice_e2e）
///   explicit input commits = 2（搜索 + 手填 Voice ID）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开 Voice → Spoken Replies，注入确定性的已连接 MiniMax 账户与三组目录。
///   2. 在搜索框输入 cloned，确认只剩 Cloned 目录 Voice，再清除搜索。
///   3. 在真实 Voice ID 输入框写入目录外 ID，确认状态要求 Preview 校验。
///   4. 通过 e2e 假 PCM 执行 Verify & Preview，等待后台完成，再点击 Use This Voice。
///   5. 激活后重新打开选择器选择目录 System Voice，确认 Apply 前 runtime 不变、Apply 后切换。
///
/// 预期结果：
///   - 正断言：两种 Provider、两个模型、搜索、三组 Voice 和手填 Voice ID 控件均可见。
///   - 正断言：Preview 后手填 Voice 变为 Ready，并可显式激活 MiniMax。
///   - 正断言：active MiniMax 的 Voice 变化会重配实际 runtime。
///   - 负断言：搜索 Cloned 时 System Voice 不可见；Preview 前 MiniMax 不得处于 Ready。
#[test]
fn e2e_minimax_manual_voice_validates_before_explicit_activation() {
    let _tts_guard = VoiceModelInstallE2eGuard::acquire();
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::tempdir().expect("应创建 TTS Provider e2e workspace");
    app.update(cx, |app, cx| {
        app.set_fake_tts_transport_for_e2e(true, false, cx);
        app.assume_minimax_connected_for_e2e(cx);
    });
    enter_canvas(&app, cx, workspace.path());
    cx.simulate_keystrokes("cmd-,");
    click(cx, "settings-section-voice");
    click(cx, "voice-ready-change-tts");
    click(cx, "settings-tts-picker-provider-minimax");

    for selector in [
        "settings-tts-picker-provider-local",
        "settings-tts-picker-provider-minimax",
        "settings-tts-minimax-model-turbo",
        "settings-tts-minimax-model-hd",
        "settings-voice-field-tts-minimax-voice-id",
        "settings-voice-field-tts-minimax-voice-search",
        "settings-tts-minimax-voice-0-0",
        "settings-tts-minimax-voice-1-0",
        "settings-tts-minimax-voice-2-0",
    ] {
        scroll_and_assert_visible(cx, selector);
    }

    click(cx, "settings-voice-field-tts-minimax-voice-search");
    cx.simulate_input("cloned");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let filtered = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        filtered.tts_provider.minimax_visible_voice_ids.as_slice(),
        ["cloned_voice_e2e"],
        "Cloned 搜索必须只保留精确匹配的 Cloned Voice"
    );
    assert!(
        cx.debug_bounds("settings-tts-minimax-voice-1-0").is_some(),
        "Cloned 搜索必须保留精确匹配的 Cloned Voice"
    );
    assert!(
        !filtered
            .tts_provider
            .minimax_visible_voice_ids
            .iter()
            .any(|voice| voice == "system_voice_e2e"),
        "Cloned 搜索不得继续显示不匹配的 System Voice"
    );

    click(cx, "settings-voice-field-tts-minimax-voice-search");
    cx.simulate_keystrokes("cmd-a backspace");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    click(cx, "settings-voice-field-tts-minimax-voice-id");
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("external_voice_e2e");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let manual = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        manual.tts_provider.picker_minimax_voice_id.as_deref(),
        Some("external_voice_e2e"),
        "真实 Voice ID 输入必须进入 MiniMax Profile"
    );
    assert_eq!(
        manual.tts_provider.picker_minimax_voice_provenance, "manual",
        "目录外 Voice ID 必须标记为 manual Draft"
    );
    assert_eq!(
        manual.tts_provider.picker_minimax_readiness, "voice_validation_required",
        "手填 Voice 在 Preview 前不得被误判为 Ready"
    );
    assert_ne!(
        manual.tts_provider.active_provider_id.as_deref(),
        Some(tts::MINIMAX_TTS_PROVIDER_ID),
        "Preview 前不得隐式激活 MiniMax"
    );

    click(cx, "settings-tts-minimax-preview");
    settle(cx);
    let validated = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        validated.tts_provider.picker_minimax_readiness, "ready",
        "成功 Preview 必须校验当前 credential/model/Voice 三元组"
    );
    click(cx, "settings-tts-picker-apply");
    settle(cx);
    let active = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        active.tts_provider.active_provider_id.as_deref(),
        Some(tts::MINIMAX_TTS_PROVIDER_ID),
        "只有点击 Use MiniMax 后 active Provider 才能切换为 MiniMax"
    );
    assert_ne!(
        active.tts_provider.active_provider_id.as_deref(),
        Some(tts::LOCAL_TTS_PROVIDER_ID),
        "显式激活 MiniMax 后不得静默回退 On-device"
    );
    assert_eq!(
        active.runtime_tts_voice_id.as_deref(),
        Some("external_voice_e2e"),
        "激活后实际 runtime 必须使用已验证的手动 Voice"
    );

    click(cx, "settings-tts-change-voice");
    click(cx, "settings-tts-picker-provider-minimax");
    scroll_and_assert_visible(cx, "settings-tts-minimax-voice-0-0");
    click(cx, "settings-tts-minimax-voice-0-0");
    settle(cx);
    let pending_switch = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        pending_switch.runtime_tts_voice_id.as_deref(),
        Some("external_voice_e2e"),
        "只选择目录 Voice 时 active runtime 必须继续使用旧手动 Voice"
    );
    assert_ne!(
        pending_switch.runtime_tts_voice_id.as_deref(),
        Some("system_voice_e2e"),
        "目录 Voice Draft 在 Apply 前不得提前重配 runtime"
    );
    click(cx, "settings-tts-picker-apply");
    settle(cx);
    let switched = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        switched.runtime_tts_provider_id.as_deref(),
        Some(tts::MINIMAX_TTS_PROVIDER_ID),
        "目录 Voice 切换后实际 runtime 必须继续使用 MiniMax Provider"
    );
    assert_eq!(
        switched.runtime_tts_voice_id.as_deref(),
        Some("system_voice_e2e"),
        "active MiniMax 选择目录 Voice 后必须重配实际 runtime"
    );
    assert_ne!(
        switched.runtime_tts_voice_id.as_deref(),
        Some("external_voice_e2e"),
        "active runtime 不得继续使用切换前的旧手动 Voice"
    );
}

/// E2E-TTS-PROVIDER-2：新 API Key 必须先显式连接，再以 Verify & Preview 校验手动 Voice。
///
/// 数据构造（含关键数值的推导过程）：
///   credential drafts = 1（synthetic-preview-draft-key）
///   manual Voice edits = 1（manual_preview_voice_e2e）
///   provider actions = 3（连接前 Preview 被拒绝 + Connect + Verify & Preview）
///   expected PCM queue delta = 1（一次成功试听至少排入一个 PCM chunk）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开 Voice → Spoken Replies，启用确定性的 e2e MiniMax 与 PCM transport。
///   2. 输入全新 API Key、Turbo 与手动 Voice ID，先点击 Preview，确认它不会代替 Connect。
///   3. 显式点击 Connect，等待账户验证与 Keychain 保存完成。
///   4. 点击 Verify & Preview，等待合成与播放任务收束，再读取 readiness 与 PCM 计数。
///
/// 预期结果：
///   - 正断言：Connect 后账户可配置；Verify & Preview 把当前手动 Voice 推进到 Ready。
///   - 正断言：试听必须实际排入 PCM，而不只是更新状态文本。
///   - 负断言：Connect 前 Preview 不得保存凭据；试听不得隐式激活 MiniMax。
#[test]
fn e2e_minimax_connects_account_before_manual_voice_preview() {
    let _tts_guard = VoiceModelInstallE2eGuard::acquire();
    let mut cx = TestAppContext::single();
    let workspace = tempfile::tempdir().expect("应创建 MiniMax 一步 Preview e2e workspace");
    let settings = tempfile::tempdir().expect("应创建 MiniMax Preview 持久化 e2e 目录");
    let store = storage::SettingsStore::new(settings.path().join("app.sqlite"));
    let inspection_store = store.clone();
    let (app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), store);
    app.update(cx, |app, cx| {
        app.set_fake_tts_transport_for_e2e(true, false, cx);
    });
    enter_canvas(&app, cx, workspace.path());
    cx.simulate_keystrokes("cmd-,");
    click(cx, "settings-section-voice");
    click(cx, "voice-ready-change-tts");
    click(cx, "settings-tts-picker-provider-minimax");
    click(cx, "settings-voice-field-tts-minimax-api-key");
    cx.simulate_input("synthetic-preview-draft-key");
    cx.simulate_keystrokes("enter");
    click(cx, "settings-tts-minimax-model-turbo");
    click(cx, "settings-voice-field-tts-minimax-voice-id");
    cx.simulate_input("manual_preview_voice_e2e");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    let pcm_before_rejected_preview =
        app.update(cx, |app, _| app.tts_preview_pcm_queue_count_for_e2e());
    click(cx, "settings-tts-minimax-preview");
    settle(cx);
    let credential_before_connect = inspection_store
        .get_string("voice.tts.minimax.credential_ref")
        .expect("Connect 前应可查询 MiniMax credential reference");
    let pcm_after_rejected_preview =
        app.update(cx, |app, _| app.tts_preview_pcm_queue_count_for_e2e());
    assert!(
        credential_before_connect.is_none(),
        "Preview 不得代替 Connect 保存未验证的 API Key 候选"
    );
    assert_eq!(
        pcm_after_rejected_preview, pcm_before_rejected_preview,
        "账户未 Connect 时 Preview 不得排入假 PCM"
    );

    click(cx, "settings-tts-minimax-connect");
    settle(cx);

    let configured = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        configured.tts_provider.picker_minimax_readiness, "voice_validation_required",
        "连接账户并填写手动 Voice 后必须明确要求 Verify & Preview"
    );
    assert_ne!(
        configured.tts_provider.picker_minimax_readiness, "unconfigured",
        "成功 Connect 后不得退回 Not Configured"
    );
    let pcm_before = app.update(cx, |app, _| app.tts_preview_pcm_queue_count_for_e2e());

    click(cx, "settings-tts-minimax-preview");
    settle(cx);

    let validated = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    let pcm_after = app.update(cx, |app, _| app.tts_preview_pcm_queue_count_for_e2e());
    assert_eq!(
        validated.tts_provider.picker_minimax_readiness, "ready",
        "Verify & Preview 必须校验已连接账户下的当前手动 Voice"
    );
    assert_ne!(
        validated.tts_provider.picker_minimax_readiness, "configured",
        "点击后不得无反馈地停留在 Configured"
    );
    assert_eq!(
        pcm_after,
        pcm_before + 1,
        "一次成功 Preview 必须恰好新增一个 e2e PCM 排队记录"
    );
    assert_ne!(
        validated.tts_provider.active_provider_id.as_deref(),
        Some(tts::MINIMAX_TTS_PROVIDER_ID),
        "Verify & Preview 只验证配置，不得隐式激活 MiniMax"
    );
    click(cx, "settings-tts-picker-apply");
    settle(cx);
    let active = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        active.tts_provider.active_provider_id.as_deref(),
        Some(tts::MINIMAX_TTS_PROVIDER_ID),
        "手动 Voice 验证证明持久化后，用户必须能显式激活 MiniMax"
    );
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    scroll_and_assert_visible(cx, selector);
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} 应可点击"));
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

fn scroll_and_assert_visible(cx: &mut VisualTestContext, selector: &'static str) {
    for _ in 0..6 {
        cx.run_until_parked();
        let Some(content) = cx.debug_bounds("settings-content-scroll") else {
            break;
        };
        let Some(target) = cx.debug_bounds(selector) else {
            break;
        };
        let visible_top = f32::from(content.origin.y) + 24.0;
        let visible_bottom = f32::from(content.bottom()) - 24.0;
        let target_top = f32::from(target.origin.y);
        let target_bottom = f32::from(target.bottom());
        if target_top >= visible_top && target_bottom <= visible_bottom {
            return;
        }
        let delta = if target_top < visible_top {
            visible_top - target_top
        } else {
            visible_bottom - target_bottom
        };
        // Settings content has 28px horizontal padding; x + 8px remains in its empty gutter.
        // Scrolling there explicitly targets the outer page instead of a nested voice list.
        let outer_scroll_position = point(content.left() + px(8.0), content.center().y);
        cx.simulate_event(ScrollWheelEvent {
            position: outer_scroll_position,
            delta: ScrollDelta::Pixels(point(px(0.0), px(delta))),
            ..Default::default()
        });
    }
    let content = cx
        .debug_bounds("settings-content-scroll")
        .expect("Settings content scroll 必须存在");
    let target = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("Settings 滚动后必须渲染 {selector}"));
    assert!(
        target.top() >= content.top() && target.bottom() <= content.bottom(),
        "Settings 滚动后 {selector} 必须位于可点击 viewport 内"
    );
}

fn settle(cx: &mut VisualTestContext) {
    // Preview 路径最多经过 4 层任务（点击→后台 PCM→前台完成→播放收束）；
    // 8 次停驻轮次 = 4 × 2，为每层保留一轮调度余量。
    for _ in 0..8 {
        cx.run_until_parked();
    }
}
