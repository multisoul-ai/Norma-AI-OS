//! Built-in TTS Provider Settings user-flow acceptance.

use crate::harness::{echo_factory, enter_canvas, make_app, make_app_with_settings_store};
use gpui::{
    point, px, Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext,
};

/// E2E-TTS-PROVIDER-1：MiniMax 必须支持目录搜索、手填 Voice ID、Preview 校验与显式激活。
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
///   4. 通过 e2e 假 PCM 执行 Preview，等待后台完成，再点击 Use MiniMax。
///   5. 激活后选择目录 System Voice，确认实际 runtime 同步切换而非继续使用旧手动 Voice。
///
/// 预期结果：
///   - 正断言：两种 Provider、两个模型、搜索、三组 Voice 和手填 Voice ID 控件均可见。
///   - 正断言：Preview 后手填 Voice 变为 Ready，并可显式激活 MiniMax。
///   - 正断言：active MiniMax 的 Voice 变化会重配实际 runtime。
///   - 负断言：搜索 Cloned 时 System Voice 不可见；Preview 前 MiniMax 不得处于 Ready。
#[test]
fn e2e_minimax_manual_voice_validates_before_explicit_activation() {
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

    for selector in [
        "settings-tts-provider-local",
        "settings-tts-provider-minimax",
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
        manual.tts_provider.minimax_voice_id.as_deref(),
        Some("external_voice_e2e"),
        "真实 Voice ID 输入必须进入 MiniMax Profile"
    );
    assert_eq!(
        manual.tts_provider.minimax_voice_provenance, "manual",
        "目录外 Voice ID 必须标记为 manual"
    );
    assert_eq!(
        manual.tts_provider.minimax_readiness, "voice_validation_required",
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
        validated.tts_provider.minimax_readiness, "ready",
        "成功 Preview 必须校验当前 credential/model/Voice 三元组"
    );
    click(cx, "settings-tts-minimax-activate");
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

    scroll_and_assert_visible(cx, "settings-tts-minimax-voice-0-0");
    click(cx, "settings-tts-minimax-voice-0-0");
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

/// E2E-TTS-PROVIDER-2：新 API Key 草稿点击 Preview / Validate 必须自动连接并产生试听。
///
/// 数据构造（含关键数值的推导过程）：
///   credential drafts = 1（synthetic-preview-draft-key）
///   manual Voice edits = 1（manual_preview_voice_e2e）
///   provider validation clicks = 1（Preview / Validate）
///   expected PCM queue delta = 1（一次成功试听至少排入一个 PCM chunk）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开 Voice → Spoken Replies，启用确定性的 e2e MiniMax 与 PCM transport。
///   2. 选择 Turbo、输入手动 Voice ID，再提交一个全新 API Key 草稿。
///   3. 确认本地状态为 Configured，随后只点击一次 Preview / Validate。
///   4. 等待连接、凭据保存、合成与播放任务收束，再读取 readiness 与 PCM 计数。
///
/// 预期结果：
///   - 正断言：单次点击必须完成远端验证并把当前手动 Voice 证明推进到 Ready。
///   - 正断言：试听必须实际排入 PCM，而不只是更新状态文本。
///   - 负断言：点击后不得仍停留在 Configured，也不得隐式激活 MiniMax。
#[test]
fn e2e_minimax_preview_connects_new_credential_draft_before_synthesis() {
    let mut cx = TestAppContext::single();
    let workspace = tempfile::tempdir().expect("应创建 MiniMax 一步 Preview e2e workspace");
    let settings = tempfile::tempdir().expect("应创建 MiniMax Preview 持久化 e2e 目录");
    let store = storage::SettingsStore::new(settings.path().join("app.sqlite"));
    let (app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), store);
    app.update(cx, |app, cx| {
        app.set_fake_tts_transport_for_e2e(true, false, cx);
    });
    enter_canvas(&app, cx, workspace.path());
    cx.simulate_keystrokes("cmd-,");
    click(cx, "settings-section-voice");
    click(cx, "voice-ready-change-tts");
    click(cx, "settings-tts-minimax-model-turbo");

    click(cx, "settings-voice-field-tts-minimax-voice-id");
    cx.simulate_input("manual_preview_voice_e2e");
    cx.simulate_keystrokes("enter");
    click(cx, "settings-voice-field-tts-minimax-api-key");
    cx.simulate_input("synthetic-preview-draft-key");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    let configured = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        configured.tts_provider.minimax_readiness, "configured",
        "提交 API Key 草稿与完整 Profile 后必须显示 Configured"
    );
    assert_ne!(
        configured.tts_provider.minimax_readiness, "unconfigured",
        "非空 API Key 草稿不得保存后立刻退回 Not Configured"
    );
    let pcm_before = app.update(cx, |app, _| app.tts_preview_pcm_queue_count_for_e2e());

    click(cx, "settings-tts-minimax-preview");
    settle(cx);

    let validated = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    let pcm_after = app.update(cx, |app, _| app.tts_preview_pcm_queue_count_for_e2e());
    assert_eq!(
        validated.tts_provider.minimax_readiness, "ready",
        "Preview / Validate 必须先连接新凭据，再校验当前手动 Voice"
    );
    assert_ne!(
        validated.tts_provider.minimax_readiness, "configured",
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
        "Preview / Validate 只验证配置，不得隐式激活 MiniMax"
    );
    click(cx, "settings-tts-minimax-activate");
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
        cx.simulate_event(ScrollWheelEvent {
            position: content.center(),
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
