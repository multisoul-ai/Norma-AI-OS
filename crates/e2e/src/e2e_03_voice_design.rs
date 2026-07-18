use gpui::{Modifiers, TestAppContext};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// 首次 Voice Setup：必须按批准稿呈现三步引导，麦克风作为识别步骤的前置状态
///
/// 数据构造（含关键数值的推导过程）：
///   hard gates = microphone(1) + ASR(1) + provider(1) + TTS(1) = 4
///   visual steps = recognition(1) + connection(1) + replies(1) = 3
///   ready gates = 0 / 4
///   setup entry = 1 次 `cmd-e`
///   model catalog disclosure = 1 次点击后同时展示 ASR + TTS 两个目录
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 fresh App、进入 Canvas，并清空四项 Voice readiness
///   2. 分发一次 `cmd-e` → 打开 Settings → Voice 内的 Setup 内容
///   3. 检查 Settings 侧栏、Voice 页头、页内进度、纵向步骤轨道和三张步骤卡
///   4. 检查麦克风操作位于 Speech Recognition 内，AI Connection 无需展开即可配置
///   5. 点击“选择其他模型”，确认 ASR 与 TTS 目录在同一 Setup 页面内可达
///
/// 预期结果：
///   - 正断言：Voice Settings 外壳、三步、四项进度、推荐包和可选模型入口均可见
///   - 负断言：Setup 不得脱离 Settings 外壳，模型目录在用户主动展开前不得铺满页面
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
        cx.debug_bounds("voice-setup-install-recommended").is_some(),
        "Speech Recognition step must expose the recommended local model package"
    );
    assert!(
        cx.debug_bounds("voice-setup-alternative-models-toggle")
            .is_some(),
        "fresh Setup must expose an explicit path to alternative ASR and TTS models"
    );
    assert!(
        cx.debug_bounds("voice-setup-alternative-models").is_none(),
        "alternative model catalogs must stay collapsed until the user explicitly requests them"
    );
    let alternative_models = cx
        .debug_bounds("voice-setup-alternative-models-toggle")
        .expect("fresh Setup must render the alternative model disclosure action");
    cx.simulate_mouse_move(alternative_models.center(), None, Modifiers::none());
    cx.simulate_click(alternative_models.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("voice-setup-alternative-models").is_some(),
        "clicking the disclosure must render the alternative model catalogs inside Voice Setup"
    );
    assert!(
        cx.debug_bounds("settings-asr-model-0").is_some(),
        "expanded Setup must expose the ASR catalog so recognition models remain configurable"
    );
    assert!(
        cx.debug_bounds("settings-tts-model-0").is_some(),
        "expanded Setup must expose the TTS catalog so spoken-reply models remain configurable"
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
///   direct model actions = ASR Change(1) + TTS Change(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建默认 Ready App 并进入 Canvas
///   2. 依次点击 Settings 与 Voice 导航
///   3. 检查 ready banner、三行摘要、两个模型更换入口和独立 Review Setup 按钮
///   4. 检查 Ready 页面保留 Settings 导航与紧凑麦克风选择器
///   5. 点击 ASR Change，确认进入同一 Setup 并自动展开模型目录
///
/// 预期结果：
///   - 正断言：Ready 横幅、配置摘要、模型更换入口、Review Setup 与侧栏均可见
///   - 负断言：横幅无额外 Try Voice；更换模型不启动录音；麦克风列表默认不展开
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
    let change_asr = cx
        .debug_bounds("voice-ready-change-asr")
        .expect("ready summary must keep the ASR Change action clickable");
    cx.simulate_mouse_move(change_asr.center(), None, Modifiers::none());
    cx.simulate_click(change_asr.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("voice-setup-alternative-models").is_some(),
        "ASR Change must open the shared Voice Setup with alternative models expanded"
    );
    assert!(
        cx.debug_bounds("settings-asr-model-0").is_some(),
        "ASR Change must expose the ASR model catalog at its destination"
    );
    assert!(
        cx.debug_bounds("settings-tts-model-0").is_some(),
        "the shared model manager must keep TTS choices reachable alongside ASR choices"
    );
    assert_ne!(
        app.update(cx, |app, _cx| app.voice_e2e_snapshot().phase),
        "recording",
        "opening model management from Ready must not capture microphone input"
    );
}
