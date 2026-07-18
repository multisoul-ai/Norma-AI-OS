use gpui::{Modifiers, TestAppContext};

use crate::harness::{
    echo_factory, enter_canvas, make_app, VoiceModelInstallE2eGuard, VoiceTranscriptEnvGuard,
};

/// Voice 尚未配置时按下 Cmd+E：必须打开 Settings → Voice 设置页，且不得开始录音
///
/// 数据构造（含关键数值的推导过程）：
///   fresh Voice required gates = 4
///   configured gates = 0
///   shortcut presses = 1（cmd-e）
///   expected recording starts = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 fresh headless App 并进入 Canvas
///   2. 确认 Voice 入口状态为 set_up、录音 phase 为 idle
///   3. 通过真实键盘分发输入 cmd-e
///   4. 读取 Settings 与 Voice 快照，并检查渲染树中的 setup 页面
///
/// 预期结果：
///   - 正断言：Settings 打开且 active section 为 Voice，setup 页面可见
///   - 正断言：Voice entry 仍为 set_up
///   - 负断言：录音 phase 不得变为 recording
#[test]
fn cmd_e_opens_voice_setup_before_required_models_are_configured() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Voice setup workspace should create");
    enter_canvas(&app, cx, workspace.path());
    app.update(cx, |app, _cx| app.reset_voice_setup_for_e2e());

    let before = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(
        before.entry_state, "set_up",
        "fresh Voice entry must require setup before cmd-e"
    );
    assert_eq!(
        before.phase, "idle",
        "fresh Voice must be idle before cmd-e"
    );

    cx.simulate_keystrokes("cmd-e");
    cx.run_until_parked();

    let settings = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    let after = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    assert!(
        settings.settings_open,
        "cmd-e must open Settings when Voice has unmet required gates"
    );
    assert_eq!(
        settings.active_section, "Voice",
        "cmd-e setup route must land directly on the Voice section"
    );
    assert_eq!(
        after.entry_state, "set_up",
        "opening setup must not falsely mark Voice ready"
    );
    assert_eq!(
        after.phase, "idle",
        "cmd-e must not start recording while Voice setup is incomplete"
    );
    assert_ne!(
        after.phase, "recording",
        "incomplete Voice configuration must block recording"
    );
    assert!(
        cx.debug_bounds("voice-setup-page").is_some(),
        "cmd-e must render the one-page Voice setup surface"
    );
}

/// 底部 Voice 入口尚未配置时被点击：与 Cmd+E 共用同一 Setup 路由
///
/// 数据构造（含关键数值的推导过程）：
///   fresh required gates = 4
///   ready gates = 0
///   bottom entry clicks = 1
///   expected recording starts = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 fresh App 并进入 Canvas
///   2. 从真实渲染树取得 workspace-voice-control 中心点
///   3. 模拟一次鼠标点击
///   4. 读取 Settings 与 Voice 状态
///
/// 预期结果：
///   - 正断言：点击打开 Settings → Voice 和 setup 页面
///   - 负断言：点击不得开始 recording
#[test]
fn bottom_voice_entry_opens_the_same_setup_surface() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Voice setup workspace should create");
    enter_canvas(&app, cx, workspace.path());
    app.update(cx, |app, _cx| app.reset_voice_setup_for_e2e());

    let bounds = cx
        .debug_bounds("workspace-voice-control")
        .expect("fresh Canvas must render the bottom Voice entry");
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();

    let settings = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    let voice = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    assert!(
        settings.settings_open,
        "bottom Voice entry must open Settings when setup is incomplete"
    );
    assert_eq!(
        settings.active_section, "Voice",
        "bottom Voice entry must route directly to the Voice section"
    );
    assert_eq!(
        voice.phase, "idle",
        "bottom Voice entry must not start recording before setup completes"
    );
    assert_ne!(
        voice.phase, "recording",
        "incomplete Voice setup must block click-triggered recording"
    );
    assert!(
        cx.debug_bounds("voice-setup-page").is_some(),
        "bottom Voice entry must reveal the same one-page setup surface as cmd-e"
    );
}

/// 已就绪的底部 Voice 入口：鼠标点击必须形成可停止的单次录音切换
///
/// 数据构造（含关键数值的推导过程）：
///   readiness = Ready（测试 harness 的完整 legacy-ready 配置）
///   pointer clicks = 2（第一次开始，第二次停止）
///   Settings opens = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入已就绪 Canvas 并定位底部 Voice 控件
///   2. 第一次点击，确认进入 recording
///   3. 第二次点击，复用 Voice 意图路由停止录音
///
/// 预期结果：
///   - 正断言：第一次点击启动录音，第二次点击退出录音
///   - 负断言：Ready 点击不得错误打开 Settings，也不得因 Hold 缺少 key-up 而卡住
#[test]
fn ready_bottom_voice_entry_clicks_toggle_one_shot_recording() {
    let _transcript = VoiceTranscriptEnvGuard::set("ready-bottom-voice-click");
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Voice click workspace should create");
    enter_canvas(&app, cx, workspace.path());
    let bounds = cx
        .debug_bounds("workspace-voice-control")
        .expect("ready Canvas must render the bottom Voice entry");

    cx.simulate_click(bounds.center(), Modifiers::none());
    let started = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    cx.simulate_click(bounds.center(), Modifiers::none());
    let stopped = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    let settings = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));

    assert_eq!(
        started.phase, "recording",
        "first ready click must start recording"
    );
    assert_ne!(
        stopped.phase, "recording",
        "second ready click must stop recording"
    );
    assert!(
        !settings.settings_open,
        "ready Voice click must not open Settings"
    );
}

/// 首次设置清单的非线性访问：三步内容同时可见，可直接配置后续 Provider
///
/// 数据构造（含关键数值的推导过程）：
///   必需项总数 = 4
///   初始 Ready 项数 = 0
///   visual steps = 3（Speech Recognition、AI Connection、Spoken Replies）
///   可直接访问的 Provider 配置 = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从 fresh Voice state 用 `⌘E` 打开 Setup
///   2. 确认 Speech Recognition 内的 Microphone 与推荐安装操作可见
///   3. 检查 AI Connection 的 Provider 连接控件同时可见
///   4. 检查入口仍保持 set_up 且没有启动录音
///
/// 预期结果：
///   - 正断言：AI Connection 无需展开即可显示连接按钮
///   - 负断言：用户不需要先授权麦克风或安装 ASR 才能配置 Provider
#[test]
fn setup_cards_allow_out_of_order_provider_configuration() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Voice setup workspace should create");
    enter_canvas(&app, cx, workspace.path());
    app.update(cx, |app, _cx| app.reset_voice_setup_for_e2e());

    cx.simulate_keystrokes("cmd-e");
    cx.run_until_parked();

    assert!(
        cx.debug_bounds("voice-setup-microphone-action").is_some(),
        "fresh Setup must show microphone permission inside Speech Recognition"
    );
    assert!(
        cx.debug_bounds("voice-setup-install-recommended").is_some(),
        "fresh Setup must show the recommended model package without expanding a card"
    );
    assert!(
        cx.debug_bounds("voice-setup-provider-connect").is_some(),
        "AI Connection must expose Provider configuration without prerequisite steps"
    );
    assert_eq!(
        app.update(cx, |app, _cx| app.voice_e2e_snapshot().phase),
        "idle",
        "opening Provider directly must not start microphone capture or any Voice business path"
    );
    assert_eq!(
        app.update(cx, |app, _cx| app.voice_e2e_snapshot().entry_state),
        "set_up",
        "out-of-order configuration must keep Voice in setup until all four hard gates are ready"
    );
    assert_ne!(
        app.update(cx, |app, _cx| app.voice_e2e_snapshot().entry_state),
        "ready",
        "expanding a later card must not bypass readiness validation"
    );
}

/// Voice 已就绪后的回看入口：从简化主页面进入完整四项清单，并可返回主页面
///
/// 数据构造（含关键数值的推导过程）：
///   e2e legacy-ready gates = 4 / 4
///   Review Setup clicks = 1
///   Done Reviewing clicks = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas，真实点击工具栏 Settings 和 Voice section
///   2. 从 Ready banner 点击 Review Setup
///   3. 确认同一 Settings 页面切为四项 Setup 清单
///   4. 点击 Done Reviewing 回到简化 Ready 页面
///
/// 预期结果：
///   - 正断言：Ready 页面与 Setup 清单之间可双向切换
///   - 负断言：回看 Setup 不得降低 readiness 或启动录音
#[test]
fn ready_voice_page_can_review_setup_and_return() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Voice review workspace should create");
    enter_canvas(&app, cx, workspace.path());

    for selector in ["toolbar-settings", "settings-section-voice"] {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("review setup test must render {selector}"));
        cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
        cx.simulate_click(bounds.center(), Modifiers::none());
        cx.run_until_parked();
    }
    assert!(
        cx.debug_bounds("voice-ready-summary").is_some(),
        "ready Voice page must expose the approved setup summary card"
    );
    let review = cx
        .debug_bounds("voice-review-setup")
        .expect("ready Voice page must expose Review Setup");
    cx.simulate_mouse_move(review.center(), None, Modifiers::none());
    cx.simulate_click(review.center(), Modifiers::none());
    cx.run_until_parked();

    assert!(
        cx.debug_bounds("voice-review-setup-done").is_some(),
        "Review Setup must reveal a clear return action on the shared setup page"
    );
    assert_eq!(
        app.update(cx, |app, _cx| app.voice_e2e_snapshot().entry_state),
        "ready",
        "reviewing completed requirements must preserve Voice readiness"
    );
    assert_eq!(
        app.update(cx, |app, _cx| app.voice_e2e_snapshot().phase),
        "idle",
        "reviewing setup must not start recording"
    );

    let done = cx
        .debug_bounds("voice-review-setup-done")
        .expect("review setup page must keep the Done Reviewing action clickable");
    cx.simulate_mouse_move(done.center(), None, Modifiers::none());
    cx.simulate_click(done.center(), Modifiers::none());
    cx.run_until_parked();

    assert!(
        cx.debug_bounds("voice-ready-banner").is_some(),
        "Done Reviewing must restore the simplified Voice ready page"
    );
    assert_ne!(
        app.update(cx, |app, _cx| app.voice_e2e_snapshot().phase),
        "recording",
        "returning from setup review must not capture microphone input"
    );
}

/// 推荐语音包一键安装：同时安装并选择 Zipformer、Matcha 与默认双语音色
///
/// 数据构造（含关键数值的推导过程）：
///   ASR archive = 511,274,346 bytes ≈ 511 MB
///   TTS package = 79,033,838 + 53,882,848 = 132,916,686 bytes ≈ 133 MB
///   推荐包总量 = 511,274,346 + 132,916,686 = 644,191,032 bytes ≈ 644 MB
///   麦克风和 Provider 通过稳定测试缝预置为 Ready
///   model root = workspace/models（与用户模型和其他测试隔离）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启用 ASR/TTS 假传输，避免测试依赖公网与真实模型文件
///   2. 通过 cmd-e 打开 Voice Setup，并注入麦克风、Provider 已就绪
///   3. 真实点击 Install Recommended
///   4. 等待两个后台安装任务完成和 TTS 默认音色提交
///
/// 预期结果：
///   - 正断言：选中推荐 ASR、推荐 TTS 和 default 音色，Voice 进入 ready
///   - 负断言：最终入口不得仍为 set_up 或 setting_up
#[test]
fn recommended_package_installs_and_selects_all_required_local_models() {
    let _install_guard = VoiceModelInstallE2eGuard::acquire();
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Voice setup workspace should create");
    enter_canvas(&app, cx, workspace.path());
    app.update(cx, |app, _cx| app.reset_voice_setup_for_e2e());
    app.update(cx, |app, cx| {
        app.set_voice_model_root_for_e2e(workspace.path().join("models"), cx)
    });
    cx.run_until_parked();
    app.update(cx, |app, _cx| {
        app.set_voice_setup_install_fakes_for_e2e(true)
    });

    cx.simulate_keystrokes("cmd-e");
    cx.run_until_parked();
    app.update(cx, |app, _cx| {
        app.set_voice_setup_requirements_for_e2e(true, true)
    });
    cx.run_until_parked();
    let install = cx
        .debug_bounds("voice-setup-install-recommended")
        .expect("ASR setup step must expose the recommended package action");
    cx.simulate_mouse_move(install.center(), None, Modifiers::none());
    cx.simulate_click(install.center(), Modifiers::none());

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let final_snapshot = loop {
        cx.run_until_parked();
        let voice = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
        if voice.entry_state == "ready" || std::time::Instant::now() >= deadline {
            break app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    let final_voice = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    app.update(cx, |app, _cx| {
        app.set_voice_setup_install_fakes_for_e2e(false)
    });

    assert_eq!(
        final_snapshot.selected_asr_model_id.as_deref(),
        Some(asr::ZIPFORMER_BILINGUAL_MODEL_ID),
        "recommended package must select Zipformer Bilingual"
    );
    assert_eq!(
        final_snapshot.selected_tts_model_id.as_deref(),
        Some("matcha-icefall-zh-en"),
        "recommended package must select Matcha Chinese-English"
    );
    assert_eq!(
        final_snapshot.selected_tts_voice_id.as_deref(),
        Some("default"),
        "recommended package must select Default Bilingual Voice"
    );
    assert_eq!(
        final_voice.entry_state, "ready",
        "all four completed requirements must transition Voice to ready"
    );
    assert_ne!(
        final_voice.entry_state, "setting_up",
        "completed recommended installs must not leave progress stuck"
    );
}

/// Settings 关闭后推荐包完成：后台继续安装并显示一次不抢焦点、不发声的 Ready 通知
///
/// 数据构造（含关键数值的推导过程）：
///   ASR fake completion = immediate
///   TTS fake completion = held until Settings closes
///   notification timeout = 5s，测试观察窗口 = 3s < 5s
///   expected audio starts = 0（Voice phase 保持 idle）
///   model root = workspace/models（与用户模型和其他测试隔离）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开 fresh Setup，注入麦克风与 Provider Ready
///   2. 启动推荐包，并让 TTS 假传输停在后台安装中
///   3. 确认底部入口进入 setting_up，而不是继续显示普通 set_up
///   4. 真实点击 Settings Close，确认面板关闭
///   5. 释放 TTS 安装，等待 setup completion 与通知渲染
///
/// 预期结果：
///   - 正断言：Settings 关闭后 Voice 仍完成为 ready，Ready 通知可见
///   - 负断言：通知不得开始录音或重新打开 Settings
#[test]
fn background_setup_completion_shows_a_nonblocking_ready_notification() {
    let _install_guard = VoiceModelInstallE2eGuard::acquire();
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("Voice setup workspace should create");
    enter_canvas(&app, cx, workspace.path());
    app.update(cx, |app, _cx| app.reset_voice_setup_for_e2e());
    app.update(cx, |app, cx| {
        app.set_voice_model_root_for_e2e(workspace.path().join("models"), cx)
    });
    cx.run_until_parked();
    app.update(cx, |app, _cx| {
        app.set_voice_setup_install_fakes_for_e2e(true)
    });
    app.update(cx, |app, cx| {
        app.set_fake_tts_transport_for_e2e(true, true, cx)
    });
    cx.simulate_keystrokes("cmd-e");
    cx.run_until_parked();
    app.update(cx, |app, _cx| {
        app.set_voice_setup_requirements_for_e2e(true, true)
    });
    cx.run_until_parked();

    let install = cx
        .debug_bounds("voice-setup-install-recommended")
        .expect("background setup test must expose recommended install");
    cx.simulate_mouse_move(install.center(), None, Modifiers::none());
    cx.simulate_click(install.center(), Modifiers::none());
    cx.run_until_parked();
    let installing = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(
        installing.entry_state, "setting_up",
        "recommended package must project active model installation into the bottom Voice entry"
    );
    assert_ne!(
        installing.entry_state, "set_up",
        "active recommended download must not keep showing the generic Set up Voice label"
    );
    let close = cx
        .debug_bounds("settings-close")
        .expect("open Voice Setup must expose Settings close");
    cx.simulate_mouse_move(close.center(), None, Modifiers::none());
    cx.simulate_click(close.center(), Modifiers::none());
    cx.run_until_parked();
    app.update(cx, |app, cx| app.release_fake_tts_transport_for_e2e(cx));

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let final_voice = loop {
        cx.run_until_parked();
        let snapshot = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
        if snapshot.ready_notification_visible || std::time::Instant::now() >= deadline {
            break snapshot;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    let final_settings = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    app.update(cx, |app, _cx| {
        app.set_voice_setup_install_fakes_for_e2e(false)
    });

    assert_eq!(
        final_voice.entry_state, "ready",
        "background model completion must finish Voice readiness"
    );
    assert!(
        final_voice.ready_notification_visible,
        "background completion must show the one-time Voice ready notification"
    );
    assert!(
        cx.debug_bounds("voice-ready-notification").is_some(),
        "ready notification must be present in the rendered workspace chrome"
    );
    assert!(
        !final_settings.settings_open,
        "background completion notification must not reopen Settings"
    );
    assert_eq!(
        final_voice.phase, "idle",
        "ready notification must not start recording or speech"
    );
    assert_ne!(
        final_voice.phase, "recording",
        "nonblocking readiness notification must not capture microphone input"
    );
}
