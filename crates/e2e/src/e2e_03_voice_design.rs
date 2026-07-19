use gpui::{Modifiers, TestAppContext};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// 首次 Voice Setup：必须按批准稿呈现三步引导，麦克风作为识别步骤的前置状态
///
/// 数据构造（含关键数值的推导过程）：
///   hard gates = microphone(1) + ASR(1) + provider(1) + TTS(1) = 4
///   visual steps = recognition(1) + connection(1) + replies(1) = 3
///   ready gates = 0 / 4
///   setup entry = 1 次 `cmd-e`
///   ASR catalog disclosure = 1 次点击后展示识别模型目录
///   TTS catalog placement = Spoken Replies 内直接展示 1 个本地声音目录
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 fresh App、进入 Canvas，并清空四项 Voice readiness
///   2. 分发一次 `cmd-e` → 打开 Settings → Voice 内的 Setup 内容
///   3. 检查 Settings 侧栏、Voice 页头、页内进度、纵向步骤轨道和三张步骤卡
///   4. 检查麦克风操作位于 Speech Recognition 内，AI Connection 无需展开即可配置
///   5. 确认 TTS 目录归属 Spoken Replies，再点击“选择其他识别模型”展开 ASR 目录
///
/// 预期结果：
///   - 正断言：Voice Settings 外壳、三步、Spoken Replies TTS 目录均可见
///   - 负断言：Setup 不得脱离 Settings 外壳；644 MB 推荐包卡片不得存在；ASR 目录在用户主动展开前不得铺满页面
#[test]
fn fresh_voice_setup_matches_the_approved_three_step_visual_structure() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Voice design workspace should create");
    enter_canvas(&app, cx, workspace.path());
    app.update(cx, |app, _cx| app.reset_voice_setup_for_e2e());

    cx.simulate_keystrokes("cmd-e");
    cx.run_until_parked();

    assert!(
        cx.debug_bounds("voice-setup-title").is_some(),
        "fresh Setup must render the centered Voice Setup title from the approved design"
    );
    assert!(
        cx.debug_bounds("voice-setup-progress").is_some(),
        "fresh Setup must render the compact four-requirement progress capsule"
    );
    assert!(
        cx.debug_bounds("voice-setup-stepper").is_some(),
        "fresh Setup must render one continuous vertical stepper"
    );
    assert!(
        cx.debug_bounds("voice-setup-step-1").is_some(),
        "fresh Setup must render step 1 for Speech Recognition"
    );
    assert!(
        cx.debug_bounds("voice-setup-step-2").is_some(),
        "fresh Setup must render step 2 for AI Connection"
    );
    assert!(
        cx.debug_bounds("voice-setup-step-3").is_some(),
        "fresh Setup must render step 3 for Spoken Replies"
    );
    assert!(
        cx.debug_bounds("voice-setup-microphone-action").is_some(),
        "Speech Recognition step must expose its required microphone prerequisite"
    );
    assert!(
        cx.debug_bounds("voice-setup-install-recommended").is_none(),
        "Voice Setup must not render the removed 644 MB recommended package card"
    );
    assert!(
        cx.debug_bounds("voice-setup-alternative-models-toggle")
            .is_some(),
        "fresh Setup must expose an explicit path to alternative ASR models"
    );
    assert!(
        cx.debug_bounds("voice-setup-alternative-models").is_none(),
        "alternative ASR catalog must stay collapsed until the user explicitly requests it"
    );
    assert!(
        cx.debug_bounds("voice-setup-spoken-replies-models")
            .is_some(),
        "Local TTS Model selection must live inside the Spoken Replies setup step"
    );
    assert!(
        cx.debug_bounds("settings-tts-model-0").is_some(),
        "Spoken Replies must directly expose the first local TTS model without opening ASR options"
    );
    let alternative_models = cx
        .debug_bounds("voice-setup-alternative-models-toggle")
        .expect("fresh Setup must render the alternative model disclosure action");
    cx.simulate_mouse_move(alternative_models.center(), None, Modifiers::none());
    cx.simulate_click(alternative_models.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("voice-setup-alternative-models").is_some(),
        "clicking the disclosure must render the alternative ASR catalog inside Voice Setup"
    );
    assert!(
        cx.debug_bounds("settings-asr-model-0").is_some(),
        "expanded Setup must expose the ASR catalog so recognition models remain configurable"
    );
    assert!(
        cx.debug_bounds("voice-setup-provider-connect").is_some(),
        "AI Connection must be configurable immediately without expanding a card"
    );
    assert!(
        cx.debug_bounds("voice-setup-finish").is_some(),
        "the setup flow must keep the approved full-width Finish Setup affordance"
    );
    assert!(
        cx.debug_bounds("settings-sidebar-scroll").is_some(),
        "first-run Setup must remain inside the normal Settings sidebar shell"
    );
    assert!(
        cx.debug_bounds("settings-section-voice").is_some(),
        "first-run Setup must keep Voice selected in the Settings navigation"
    );
    assert!(
        cx.debug_bounds("voice-setup-requirement-microphone")
            .is_none(),
        "microphone permission must not remain a separate fourth setup card"
    );
}

/// Voice Ready 页面：必须按批准稿用横幅和配置摘要分层呈现
///
/// 数据构造（含关键数值的推导过程）：
///   harness readiness = Ready（4 / 4 hard gates）
///   Settings clicks = toolbar(1) + Voice navigation(1) = 2
///   summary rows = recognition(1) + connection(1) + spoken replies(1) = 3
///   collapsed microphone picker = 1
///   direct configuration actions = ASR Change(1) + Provider Change(1) + TTS Change(1) = 3
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建默认 Ready App 并进入 Canvas
///   2. 依次点击 Settings 与 Voice 导航
///   3. 检查 ready banner、三行摘要、两个模型更换入口和独立 Review Setup 按钮
///   4. 检查 Ready 页面保留 Settings 导航与紧凑麦克风选择器
///   5. 确认 Ready Advanced 不再重复 TTS 目录
///   6. 点击 ASR Change，确认进入同一 Setup 并自动展开识别模型目录
///
/// 预期结果：
///   - 正断言：Ready 横幅、配置摘要、模型更换入口、Review Setup 与侧栏均可见
///   - 负断言：横幅无额外 Try Voice；Advanced 无重复 TTS；更换模型不启动录音；麦克风列表默认不展开
#[test]
fn ready_voice_page_matches_the_approved_summary_structure() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Voice ready design workspace should create");
    enter_canvas(&app, cx, workspace.path());

    for selector in ["toolbar-settings", "settings-section-voice"] {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("Voice ready design test must render {selector}"));
        cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
        cx.simulate_click(bounds.center(), Modifiers::none());
        cx.run_until_parked();
    }

    assert!(
        cx.debug_bounds("voice-ready-banner").is_some(),
        "ready Voice page must render the approved success banner"
    );
    assert!(
        cx.debug_bounds("voice-ready-summary").is_some(),
        "ready Voice page must render a separate setup summary card"
    );
    assert!(
        cx.debug_bounds("voice-ready-summary-recognition").is_some(),
        "ready summary must show Speech Recognition status"
    );
    assert!(
        cx.debug_bounds("voice-ready-summary-provider").is_some(),
        "ready summary must show AI Connection status"
    );
    assert!(
        cx.debug_bounds("voice-ready-summary-replies").is_some(),
        "ready summary must show required Spoken Replies status"
    );
    assert!(
        cx.debug_bounds("voice-ready-change-asr").is_some(),
        "ready summary must expose a direct Change action for the selected ASR model"
    );
    assert!(
        cx.debug_bounds("voice-ready-change-tts").is_some(),
        "ready summary must expose a direct Change action for the selected TTS model and voice"
    );
    assert!(
        cx.debug_bounds("voice-ready-change-provider").is_some(),
        "ready summary must align AI Connection with a direct Change action"
    );
    assert!(
        cx.debug_bounds("voice-review-setup").is_some(),
        "ready summary must expose the full-width Review Setup action"
    );
    assert!(
        cx.debug_bounds("settings-sidebar-scroll").is_some(),
        "ready Voice configuration must remain inside normal Settings navigation"
    );
    assert!(
        cx.debug_bounds("settings-voice-input-device-picker")
            .is_some(),
        "ready Voice Input card must show one compact microphone picker"
    );
    assert!(
        cx.debug_bounds("voice-try-now").is_none(),
        "the approved ready banner must not contain an extra Try Voice action"
    );
    assert!(
        cx.debug_bounds("settings-voice-input-device-default")
            .is_none(),
        "ready Voice Input card must keep the full device list collapsed by default"
    );
    assert!(
        cx.debug_bounds("settings-tts-model-0").is_none(),
        "Ready Advanced must not duplicate Local TTS Model outside Spoken Replies"
    );
    let change_asr = cx
        .debug_bounds("voice-ready-change-asr")
        .expect("ready summary must keep the ASR Change action clickable");
    cx.simulate_mouse_move(change_asr.center(), None, Modifiers::none());
    cx.simulate_click(change_asr.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("voice-setup-alternative-models").is_some(),
        "ASR Change must open the shared Voice Setup with recognition models expanded"
    );
    assert!(
        cx.debug_bounds("settings-asr-model-0").is_some(),
        "ASR Change must expose the ASR model catalog at its destination"
    );
    assert!(
        cx.debug_bounds("settings-tts-model-0").is_some(),
        "Spoken Replies must keep TTS choices reachable while reviewing ASR models"
    );
    assert_ne!(
        app.update(cx, |app, _cx| app.voice_e2e_snapshot().phase),
        "recording",
        "opening model management from Ready must not capture microphone input"
    );
}

/// Voice Ready 的 TTS Change：必须进入 Spoken Replies，而不是复用 ASR 高级目录。
///
/// 数据构造（含关键数值的推导过程）：
///   harness readiness = Ready（4 / 4 hard gates）
///   Settings clicks = toolbar(1) + Voice navigation(1) = 2
///   TTS Change clicks = 1
///   expected expanded ASR catalogs = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建独立 Ready App 并进入 Settings → Voice
///   2. 点击 Spoken Replies 行的 TTS Change
///   3. 检查 Setup 中的 Provider 选择、Local TTS 首项和 ASR 折叠状态
///   4. 对比 On-device Provider 与 Settings 滚动视口边界，确认目标已被定位到可见范围
///
/// 预期结果：
///   - 正断言：Provider 与 Local TTS Model 都位于 Spoken Replies，且 Provider 在视口内
///   - 负断言：TTS Change 不得展开无关的 ASR 目录，也不得开始录音
#[test]
fn ready_tts_change_routes_to_spoken_replies_without_expanding_asr() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("TTS Change workspace should create");
    enter_canvas(&app, cx, workspace.path());

    for selector in ["toolbar-settings", "settings-section-voice"] {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("TTS Change test must render {selector}"));
        cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
        cx.simulate_click(bounds.center(), Modifiers::none());
        cx.run_until_parked();
    }
    let change_tts = cx
        .debug_bounds("voice-ready-change-tts")
        .expect("ready summary must keep the TTS Change action clickable");
    cx.simulate_mouse_move(change_tts.center(), None, Modifiers::none());
    cx.simulate_click(change_tts.center(), Modifiers::none());
    // frame 1 切换 Setup；frame 2 应用 Spoken Replies 锚点偏移；frame 3 绘制最终视口。
    for _ in 0..3 {
        cx.run_until_parked();
    }
    app.update(cx, |_app, cx| cx.notify());
    cx.run_until_parked();

    assert!(
        cx.debug_bounds("voice-setup-spoken-replies-models")
            .is_some(),
        "TTS Change must route to Local TTS Model selection inside Spoken Replies"
    );
    assert!(
        cx.debug_bounds("settings-tts-model-0").is_some(),
        "TTS Change must expose the first local TTS model"
    );
    assert!(
        cx.debug_bounds("settings-tts-provider-minimax").is_some(),
        "TTS Change must expose the built-in MiniMax Provider alongside On-device"
    );
    let viewport = cx
        .debug_bounds("settings-content-scroll")
        .expect("TTS Change must keep the shared Settings scroll viewport");
    let on_device_provider = cx
        .debug_bounds("settings-tts-provider-local")
        .expect("TTS Change must render On-device Provider before checking visibility");
    assert!(
        on_device_provider.center().y >= viewport.top()
            && on_device_provider.center().y <= viewport.bottom(),
        "TTS Change must scroll the Provider selector into the Settings viewport; viewport={viewport:?}, provider={on_device_provider:?}"
    );
    assert!(
        cx.debug_bounds("voice-setup-alternative-models").is_none(),
        "TTS Change must not expand the unrelated ASR model catalog"
    );
    assert_ne!(
        app.update(cx, |app, _cx| app.voice_e2e_snapshot().phase),
        "recording",
        "TTS Change must not capture microphone input"
    );
}

/// Voice Ready 的 AI Connection Change：必须进入同一 Setup 并定位到 Provider。
///
/// 数据构造（含关键数值的推导过程）：
///   harness readiness = Ready（4 / 4 hard gates）
///   Settings clicks = toolbar(1) + Voice navigation(1) = 2
///   Provider Change clicks = 1
///   expected expanded ASR catalogs = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建独立 Ready App 并进入 Settings → Voice
///   2. 点击 AI Connection 行的 Change
///   3. 强制绘制滚动后视口，检查 Provider Connect 操作的绘制边界
///   4. 检查 ASR 目录折叠且语音仍为 idle
///
/// 预期结果：
///   - 正断言：AI Connection 的 Connect 操作位于 Settings 视口内
///   - 负断言：Provider Change 不得展开 ASR 目录，也不得开始录音
#[test]
fn ready_provider_change_routes_to_visible_ai_connection() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Provider Change workspace should create");
    enter_canvas(&app, cx, workspace.path());

    for selector in ["toolbar-settings", "settings-section-voice"] {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("Provider Change test must render {selector}"));
        cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
        cx.simulate_click(bounds.center(), Modifiers::none());
        cx.run_until_parked();
    }
    let change_provider = cx
        .debug_bounds("voice-ready-change-provider")
        .expect("ready summary must keep the Provider Change action clickable");
    cx.simulate_mouse_move(change_provider.center(), None, Modifiers::none());
    cx.simulate_click(change_provider.center(), Modifiers::none());
    for _ in 0..3 {
        cx.run_until_parked();
    }
    app.update(cx, |_app, cx| cx.notify());
    cx.run_until_parked();

    let viewport = cx
        .debug_bounds("settings-content-scroll")
        .expect("Provider Change must keep the shared Settings scroll viewport");
    let connect = cx
        .debug_bounds("voice-setup-provider-connect")
        .expect("Provider Change must render the AI Connection action");
    assert!(
        connect.center().y >= viewport.top() && connect.center().y <= viewport.bottom(),
        "Provider Change must scroll AI Connection into the Settings viewport; viewport={viewport:?}, connect={connect:?}"
    );
    assert!(
        cx.debug_bounds("voice-setup-alternative-models").is_none(),
        "Provider Change must not expand the unrelated ASR model catalog"
    );
    assert_eq!(
        app.update(cx, |app, _cx| app.voice_e2e_snapshot().phase),
        "idle",
        "Provider Change must keep Voice idle instead of capturing microphone input"
    );
}
