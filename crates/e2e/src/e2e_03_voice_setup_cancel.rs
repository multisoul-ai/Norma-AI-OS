use gpui::{Modifiers, TestAppContext};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// 推荐语音包取消：一次点击必须同时停止仍活跃的本地模型任务，并回到可重试设置态
///
/// 数据构造（含关键数值的推导过程）：
///   推荐任务 = 2（ASR + TTS）
///   ASR fake = immediate completion
///   TTS fake = held installation（保证取消按钮可观察）
///   cancel clicks = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开 fresh Voice Setup，并注入麦克风与 Provider Ready
///   2. 点击 Install Recommended，等待入口进入 setting_up
///   3. 再点击同一稳定 selector，此时动作语义为 Cancel Download
///   4. 等待 TTS 取消收口并读取 Voice 与 Settings 快照
///
/// 预期结果：
///   - 正断言：取消后仍停留在可重试 Setup，TTS 回到 not_installed
///   - 负断言：入口不得保持 setting_up 或错误进入 Ready
#[test]
fn recommended_voice_package_can_be_cancelled_and_retried() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Voice cancel workspace should create");
    enter_canvas(&app, cx, workspace.path());
    app.update(cx, |app, _cx| app.reset_voice_setup_for_e2e());
    app.update(cx, |app, _cx| {
        app.set_voice_setup_install_fakes_for_e2e(true)
    });
    app.update(cx, |app, cx| {
        app.set_fake_tts_transport_for_e2e(true, true, cx)
    });
    app.update(cx, |app, _cx| {
        app.set_voice_setup_requirements_for_e2e(true, true)
    });
    cx.simulate_keystrokes("cmd-e");
    cx.run_until_parked();

    let install = cx
        .debug_bounds("voice-setup-install-recommended")
        .expect("Voice Setup must expose recommended install action");
    cx.simulate_click(install.center(), Modifiers::none());
    cx.run_until_parked();
    let installing = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(
        installing.entry_state, "setting_up",
        "held TTS task must expose aggregate setting_up state before cancellation"
    );

    cx.simulate_click(install.center(), Modifiers::none());
    cx.run_until_parked();
    let voice = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    let settings = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    app.update(cx, |app, _cx| {
        app.set_voice_setup_install_fakes_for_e2e(false)
    });

    let recommended_tts = settings
        .tts_models
        .iter()
        .find(|model| model.id == "matcha-icefall-zh-en")
        .expect("Settings snapshot must include recommended TTS model");
    assert_eq!(
        recommended_tts.installation, "not_installed",
        "cancelled recommended TTS must return to a retryable not_installed state"
    );
    assert_eq!(
        voice.entry_state, "set_up",
        "cancelled package must stay in Setup"
    );
    assert_ne!(
        voice.entry_state, "setting_up",
        "cancelled package must not leave stale download progress"
    );
    assert_ne!(
        voice.entry_state, "ready",
        "cancelled required TTS must block Ready"
    );
}
