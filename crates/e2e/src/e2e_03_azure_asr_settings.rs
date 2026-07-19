#[path = "e2e_03_azure_asr_support.rs"]
mod support;

use gpui::TestAppContext;

use crate::harness::{echo_factory, enter_canvas, make_app, VoiceTranscriptEnvGuard};
use support::*;

/// E2E-ASR-AZ-1：真实 Settings 操作必须完成 Azure 验证、识别测试与显式供应商切换。
///
/// 数据构造（含关键数值的推导过程）：
///   built-in providers = 2（Local Sherpa + Azure Speech）
///   user-entered fields = 2（region + Key；locale 使用 UI 默认 zh-CN）
///   fake Azure requests = 8（Validate + Test + Single + Continuous + Limit + Failure + 2×Late）
///   validation WAV = 44 + 3,200 = 3,244 bytes
///   recognition WAV = 44 + 640 = 684 bytes（20 ms × 16,000 × 2）
///   Azure duration limit = 60 × 16,000 × 2 = 1,920,000 PCM bytes
///   explicit provider changes = 6（含往返切换与迟到结果失效切换）
///   async settle budget = 600 × 5 ms = 3,000 ms（仅等待本地 fake HTTP 后台任务）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开 Voice Settings，确认 fresh 用户仍以 Local Sherpa 为当前 Provider
///   2. 通过真实输入控件填写 eastus 与 fake Key，未验证时点击 Azure 不切换
///   3. 点击 Validate，后台经 fake Azure HTTP 验证认证并将相同 profile 标记 Ready
///   4. 保持 Local 激活时点击 Test Recognition，确定性 PCM 经公共 pipeline 得到 Final
///   5. 真实 ⌘E 驱动 Single 与 Continuous，确定性 PCM 分别进入同一 Azure factory/pipeline
///   6. 送入精确 60 秒 PCM，观察 Transcribing 上限提示及已录音频提交
///   7. 注入 401，确认错误可见且不回退 Local；再以 Provider 切换使迟到结果失效
///
/// 预期结果：
///   - 正断言：验证与识别测试走真实按钮、后台任务、Keychain seam 和 Azure transport
///   - 正断言：Ready 后可往返切换 Local / Azure，Azure profile 保持可用
///   - 正断言：Single、Continuous 与 60 秒边界均产生各自具体 Final
///   - 负断言：未 Ready 或 Test Recognition 时不得隐式替换 Local Sherpa
///   - 负断言：401 不得回退 Local，迟到 Final 不得覆盖新会话 transcript
#[test]
fn azure_asr_requires_real_validation_and_supports_test_and_round_trip_switching() {
    let _transcript = VoiceTranscriptEnvGuard::clear();
    let (endpoint, server) = spawn_fake_azure_server();
    let _endpoint_override = asr::install_azure_speech_endpoint_override_for_tests(endpoint);
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Azure ASR e2e workspace should create");
    enter_canvas(&app, cx, workspace.path());
    app.update(cx, |app, _cx| app.reset_voice_setup_for_e2e());

    cx.simulate_keystrokes("cmd-e");
    cx.run_until_parked();

    assert_eq!(
        app.read_with(cx, |app, _cx| app.active_asr_provider_for_e2e()),
        "local_sherpa",
        "fresh and upgraded users must default to Local Sherpa"
    );
    assert!(
        !app.read_with(cx, |app, _cx| app.azure_asr_ready_for_e2e()),
        "Azure profile must not start Ready before real validation"
    );

    input_text(cx, "settings-voice-field-azure-region", "eastus");
    input_secret(
        cx,
        "settings-voice-field-azure-api-key",
        "fake-e2e-azure-key",
    );
    click(cx, "settings-asr-provider-azure");

    assert_eq!(
        app.read_with(cx, |app, _cx| app.active_asr_provider_for_e2e()),
        "local_sherpa",
        "unvalidated Azure draft must not replace Local Sherpa"
    );

    click(cx, "settings-asr-azure-validate");
    for _ in 0..600 {
        cx.run_until_parked();
        if app.read_with(cx, |app, _cx| app.azure_asr_ready_for_e2e()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    assert!(
        app.read_with(cx, |app, _cx| app.azure_asr_ready_for_e2e()),
        "successful fake Azure authentication must mark the unchanged profile Ready; status: {:?}",
        app.read_with(cx, |app, _cx| app.voice_settings_status_for_e2e())
    );
    assert_eq!(
        app.read_with(cx, |app, _cx| app.active_asr_provider_for_e2e()),
        "local_sherpa",
        "validation alone must preserve the currently active Local provider"
    );
    let validation_status = app
        .read_with(cx, |app, _cx| app.voice_settings_status_for_e2e())
        .expect("validation should publish a visible Voice Settings status");
    assert!(
        validation_status.0.contains("ready to activate"),
        "validation success should tell the user Azure is ready to activate"
    );
    assert!(
        !validation_status.1,
        "successful validation must not be rendered as an error"
    );

    click(cx, "settings-asr-azure-test-recognition");
    for _ in 0..600 {
        cx.run_until_parked();
        let completed = app.read_with(cx, |app, _cx| {
            app.voice_settings_status_for_e2e()
                .is_some_and(|(message, _)| message.contains("Azure e2e transcript"))
        });
        if completed {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    let recognition_status = app
        .read_with(cx, |app, _cx| app.voice_settings_status_for_e2e())
        .expect("recognition test should publish a visible Voice Settings status");
    assert!(
        recognition_status.0.contains("Azure e2e transcript"),
        "Test Recognition must display the fake provider's concrete Final transcript"
    );
    assert!(
        !recognition_status.1,
        "successful recognition test must not be rendered as an error"
    );
    assert_eq!(
        app.read_with(cx, |app, _cx| app.active_asr_provider_for_e2e()),
        "local_sherpa",
        "Test Recognition must not implicitly activate Azure"
    );

    click(cx, "settings-asr-provider-azure");
    assert_eq!(
        app.read_with(cx, |app, _cx| app.active_asr_provider_for_e2e()),
        "cloud_azure",
        "Ready Azure profile must activate only after the explicit click"
    );

    app.update(cx, |app, _cx| app.assume_voice_ready_for_e2e());
    cx.simulate_keystrokes("cmd-,");
    app.update(cx, |app, _cx| {
        app.install_voice_pcm_for_e2e(vec![vec![0; 640]])
    });
    voice_key_down(cx);
    wait_for_active_speech_session(&app, cx);
    voice_key_up(cx);
    wait_for_pending_transcript(&app, cx, "Azure single transcript");
    assert_eq!(
        app.read_with(cx, |app, _cx| app.pending_voice_transcript_for_e2e()),
        Some("Azure single transcript".to_string()),
        "Azure Single Voice Command Final must enter the existing Agent dialogue turn"
    );

    app.update(cx, |app, _cx| app.reset_voice_runtime_for_e2e());
    cx.simulate_keystrokes("cmd-,");
    click(cx, "settings-voice-mode-continuous");
    cx.simulate_keystrokes("cmd-,");
    let loud = std::iter::repeat_n(i16::MAX.to_le_bytes(), 512)
        .flatten()
        .collect::<Vec<_>>();
    app.update(cx, |app, _cx| {
        app.install_voice_pcm_for_e2e(vec![loud, vec![0; 1_024]])
    });
    voice_key_down(cx);
    wait_for_pending_transcript(&app, cx, "Azure continuous transcript");
    assert_eq!(
        app.read_with(cx, |app, _cx| app.pending_voice_transcript_for_e2e()),
        Some("Azure continuous transcript".to_string()),
        "Azure Continuous Listening must use VAD SpeechEnd and the same public pipeline"
    );

    app.update(cx, |app, _cx| app.reset_voice_runtime_for_e2e());
    cx.simulate_keystrokes("cmd-,");
    click(cx, "settings-voice-mode-dictation");
    cx.simulate_keystrokes("cmd-,");
    app.update(cx, |app, _cx| {
        app.install_voice_pcm_for_e2e(vec![vec![0; 1_920_000]])
    });
    voice_key_down(cx);
    wait_for_pending_transcript(&app, cx, "Azure limit transcript");
    let limit_snapshot = app.read_with(cx, |app, _cx| app.voice_e2e_snapshot());
    assert!(
        limit_snapshot
            .duration_limit_feedback
            .as_deref()
            .is_some_and(|message| message.contains("Azure 60-second limit reached")),
        "exact 60-second input must show an explicit Azure limit reason: {limit_snapshot:?}"
    );
    assert_ne!(
        limit_snapshot.phase, "recording",
        "60-second limit must stop accepting more PCM before the request completes"
    );

    app.update(cx, |app, _cx| app.reset_voice_runtime_for_e2e());
    app.update(cx, |app, _cx| {
        app.install_voice_pcm_for_e2e(vec![vec![0; 640]])
    });
    voice_key_down(cx);
    wait_for_active_speech_session(&app, cx);
    voice_key_up(cx);
    wait_for_voice_phase(&app, cx, "error");
    let failure = app.read_with(cx, |app, _cx| app.voice_e2e_snapshot());
    assert!(
        failure.label.contains("401"),
        "Azure authentication failure must remain a concrete visible error: {failure:?}"
    );
    assert_eq!(
        app.read_with(cx, |app, _cx| app.active_asr_provider_for_e2e()),
        "cloud_azure",
        "Azure runtime failure must not silently fall back to Local Sherpa"
    );

    app.update(cx, |app, _cx| app.reset_voice_runtime_for_e2e());
    app.update(cx, |app, _cx| {
        app.set_voice_result_delay_for_e2e(Some(1_000));
        app.install_voice_pcm_for_e2e(vec![vec![0; 640]])
    });
    voice_key_down(cx);
    wait_for_active_speech_session(&app, cx);
    voice_key_up(cx);
    assert_eq!(
        app.read_with(cx, |app, _cx| app.voice_e2e_snapshot().phase),
        "transcribing",
        "old Azure request must enter Transcribing before its generation is invalidated"
    );
    open_voice_settings(&app, cx);
    click(cx, "settings-asr-provider-local");
    assert_eq!(
        app.read_with(cx, |app, _cx| app.active_asr_provider_for_e2e()),
        "local_sherpa",
        "user must be able to explicitly switch back to Local Sherpa"
    );
    click(cx, "settings-asr-provider-azure");
    assert_eq!(
        app.read_with(cx, |app, _cx| app.active_asr_provider_for_e2e()),
        "cloud_azure",
        "retained Ready profile must support switching back to Azure again"
    );
    cx.simulate_keystrokes("cmd-,");
    app.update(cx, |app, _cx| {
        app.set_voice_result_delay_for_e2e(None);
        app.install_voice_pcm_for_e2e(vec![vec![0; 640]])
    });
    voice_key_down(cx);
    wait_for_active_speech_session(&app, cx);
    voice_key_up(cx);
    wait_for_pending_transcript(&app, cx, "Azure fresh transcript");
    std::thread::sleep(std::time::Duration::from_millis(400));
    cx.run_until_parked();
    assert_eq!(
        app.read_with(cx, |app, _cx| app.pending_voice_transcript_for_e2e()),
        Some("Azure fresh transcript".to_string()),
        "late Final from the invalidated old generation must not overwrite the fresh session"
    );

    open_voice_settings(&app, cx);
    click(cx, "settings-asr-provider-local");
    assert_eq!(
        app.read_with(cx, |app, _cx| app.active_asr_provider_for_e2e()),
        "local_sherpa",
        "second round-trip must restore Local without losing the Azure profile"
    );

    let requests = server.join().expect("fake Azure server should finish");
    assert_eq!(
        requests.len(),
        8,
        "full Settings and App mode flow must issue exactly eight bounded Azure requests"
    );
    assert!(
        String::from_utf8_lossy(&requests[0]).contains("initialSilenceTimeoutMs=1000"),
        "first request must be the bounded authenticated-silence validation"
    );
    assert!(
        !String::from_utf8_lossy(&requests[1]).contains("initialSilenceTimeoutMs=1000"),
        "recognition request must not be confused with credential validation"
    );
}
