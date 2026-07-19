use super::*;

/// E2E-TTS-1：Voice Settings 必须展示官方 TTS 下载、选择、删除与试听入口。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog entries = 3（Matcha zh-en + Kokoro + ZipVoice-Distill INT8）
///   initial selected = 0（新用户没有自动下载模型）
///   first entry actions = 1（Install）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. headless 打开 Canvas 与 Voice Settings，从 Ready 摘要点击 TTS Change；
///   2. 点击 Manage Models 渐进展开本地目录，再查询第一个模型的安装按钮；
///   3. 保持未安装状态，不发起真实网络下载。
///
/// 预期结果：
///   - 正断言：目录精确暴露三个官方模型与 Install 控件；
///   - 负断言：新用户不得已有选择或自动下载。
#[test]
fn e2e_tts_catalog_exposes_explicit_install_without_auto_selection() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::tempdir().expect("应创建 e2e workspace");
    app.update(cx, |app, cx| {
        app.set_voice_model_root_for_e2e(workspace.path().join("models"), cx)
    });
    enter_canvas(&app, cx, workspace.path());
    cx.simulate_keystrokes("cmd-,");
    click(&app, cx, "settings-section-voice");
    let snapshot = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        snapshot.active_section, "Voice",
        "点击 Voice 导航后必须显示 Voice 内容，snapshot: {:?}",
        snapshot
    );
    click(&app, cx, "voice-ready-change-tts");
    settle(cx);
    click(&app, cx, "settings-tts-picker-manage-models");
    settle(cx);
    let snapshot = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        snapshot.tts_models.len(),
        3,
        "TTS 目录应包含两个固定音色模型和一个实验性克隆模型"
    );
    assert_eq!(
        snapshot.selected_tts_model_id, None,
        "新用户不应自动选择或下载 TTS 模型"
    );
    assert_eq!(
        snapshot.tts_models[0].id, "matcha-icefall-zh-en",
        "第一个推荐目录项必须是不可变 ID 的 Matcha zh-en，而不是旧 Baker 模型"
    );
    assert_ne!(
        snapshot.tts_models[0].id, "matcha-icefall-zh-baker",
        "Voice Settings 不得继续向用户暴露旧 Matcha Baker 模型"
    );
    assert!(
        cx.debug_bounds("settings-tts-install-0").is_some(),
        "第一个未安装模型应提供 Install 按钮"
    );
    assert!(
        cx.debug_bounds("settings-tts-model-1").is_some(),
        "第二个精选模型应出现在 Voice Settings"
    );
    assert!(
        cx.debug_bounds("settings-tts-model-2").is_some(),
        "第三个实验性 ZipVoice 模型应出现在 Voice Settings"
    );
    assert_eq!(
        snapshot.tts_models[0].voices,
        vec!["default".to_string()],
        "单 Voice 的 Matcha 条目必须公开默认 Voice，而不能隐藏为模型文件细节"
    );
    assert_eq!(
        snapshot.tts_models[1].voices,
        vec!["af_heart".to_string(), "af_bella".to_string()],
        "多 Voice 的 Kokoro 条目必须公开两个可选择的目录 Voice"
    );
    assert_eq!(
        snapshot.tts_models[0].action, "install",
        "未安装模型必须只提供显式 Install，而不能误显示 Select、Delete 或 Cancel"
    );
    assert!(
        !snapshot.tts_models[0].deletable,
        "未安装模型不得暴露删除动作，因为没有可回收的受控目录"
    );
}

/// E2E-TTS-2：取消 Voice 选择器后，后台完成的模型安装不得创建或激活声音 Draft。
///
/// 数据构造（含关键数值的推导过程）：
///   initial active TTS = 0（新用户未配置）
///   held installs = 1（Matcha 假传输停在后台）
///   picker cancellations = 1（下载完成前点击 Cancel）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开统一选择器与本地模型目录，启动 Matcha 安装并暂停完成事件。
///   2. 点击选择器 Cancel，确认 Draft 生命周期已结束。
///   3. 释放假传输并等待模型进入 Ready，再读取 active 与 picker 快照。
///
/// 预期结果：
///   - 正断言：模型安装状态可以独立变为 Ready，选择器保持关闭。
///   - 负断言：安装完成不得重开 Draft、写入 active Model 或配置运行时。
#[test]
fn e2e_cancelled_picker_ignores_late_local_install_selection() {
    let _transport_guard = VoiceModelInstallE2eGuard::acquire();
    let mut cx = TestAppContext::single();
    let temp = tempfile::tempdir().expect("应创建隔离 TTS 安装目录");
    let (app, cx) = make_app(&mut cx, echo_factory());
    app.update(cx, |app, cx| {
        app.set_voice_model_root_for_e2e(temp.path().join("models"), cx);
        app.set_fake_tts_transport_for_e2e(true, true, cx);
    });
    enter_canvas(&app, cx, temp.path());
    cx.simulate_keystrokes("cmd-,");
    click(&app, cx, "settings-section-voice");
    click(&app, cx, "voice-ready-change-tts");
    click(&app, cx, "settings-tts-picker-manage-models");
    click(&app, cx, "settings-tts-install-0");

    let installing = snapshot(&app, cx);
    assert_eq!(
        installing.tts_models[0].installation, "installing",
        "释放前 Matcha 假安装必须保持 installing"
    );
    click(&app, cx, "settings-tts-picker-cancel");
    app.update(cx, |app, cx| app.release_fake_tts_transport_for_e2e(cx));
    settle(cx);

    let completed = snapshot(&app, cx);
    assert_eq!(
        completed.tts_models[0].installation, "ready",
        "Cancel 只丢弃 Voice Draft，不应取消独立模型安装"
    );
    assert!(
        !completed.tts_provider.picker_open,
        "后台安装完成不得重新打开已取消的 Voice 选择器"
    );
    assert_eq!(
        completed.selected_tts_model_id, None,
        "已取消选择器的后台安装不得写入 active TTS model"
    );
    assert_eq!(
        completed.tts_provider.picker_local_model_id, None,
        "已取消选择器的后台安装不得重建本地 Draft"
    );
    assert_eq!(
        completed.runtime_tts_model_id, None,
        "已取消选择器的后台安装不得配置 TTS runtime"
    );
}

/// E2E-TTS-4：Use This Voice 持久化失败时必须回滚内存并保留当前运行时。
///
/// 数据构造（含关键数值的推导过程）：
///   installed Drafts = 1（Matcha/default）
///   active TTS before Apply = 0（未配置）
///   valid SQLite paths = 0（Apply 前把数据库文件路径替换成目录，强制打开失败）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用假传输安装 Matcha，形成可应用的本地 Draft。
///   2. 删除仅属于本测试的 settings 子目录，注入确定性的持久化失败。
///   3. 点击 Use This Voice，等待后台事务失败并读取内存、Picker 与 runtime。
///
/// 预期结果：
///   - 正断言：Picker 继续打开并保留 Matcha Draft，允许用户修复后重试。
///   - 负断言：失败不得写入 active Model、关闭 Picker 或切换 TTS runtime。
#[test]
fn e2e_local_activation_rolls_back_when_atomic_persistence_fails() {
    let _transport_guard = VoiceModelInstallE2eGuard::acquire();
    let mut cx = TestAppContext::single();
    let temp = tempfile::tempdir().expect("应创建隔离 TTS 激活目录");
    let settings_dir = temp.path().join("settings");
    std::fs::create_dir_all(&settings_dir).expect("应创建隔离 settings 子目录");
    let store = storage::SettingsStore::new(settings_dir.join("app.sqlite"));
    let (app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), store);
    app.update(cx, |app, cx| {
        app.set_voice_model_root_for_e2e(temp.path().join("models"), cx);
        app.set_fake_tts_transport_for_e2e(true, false, cx);
    });
    enter_canvas(&app, cx, temp.path());
    cx.simulate_keystrokes("cmd-,");
    click(&app, cx, "settings-section-voice");
    click(&app, cx, "voice-ready-change-tts");
    click(&app, cx, "settings-tts-picker-manage-models");
    click(&app, cx, "settings-tts-install-0");
    settle(cx);
    let drafted = snapshot(&app, cx);
    assert_eq!(
        drafted.tts_provider.picker_local_model_id.as_deref(),
        Some("matcha-icefall-zh-en"),
        "安装成功后必须先形成 Matcha Draft"
    );

    std::fs::remove_dir_all(&settings_dir).expect("应删除隔离 settings 子目录以注入持久化失败");
    std::fs::create_dir_all(settings_dir.join("app.sqlite"))
        .expect("应把数据库路径替换为目录以稳定注入 SQLite 打开失败");
    click(&app, cx, "settings-tts-picker-apply");
    settle(cx);

    let failed = snapshot(&app, cx);
    assert!(
        failed.tts_provider.picker_open,
        "原子持久化失败后 Picker 必须保持打开以便重试"
    );
    assert_eq!(
        failed.tts_provider.picker_local_model_id.as_deref(),
        Some("matcha-icefall-zh-en"),
        "原子持久化失败后 Matcha Draft 必须保留"
    );
    assert_eq!(
        failed.selected_tts_model_id, None,
        "原子持久化失败不得把 Matcha 写入 active Model"
    );
    assert_eq!(
        failed.runtime_tts_model_id, None,
        "原子持久化失败不得切换实际 TTS runtime"
    );
    assert_ne!(
        failed.tts_provider.active_provider_id.as_deref(),
        Some(tts::LOCAL_TTS_PROVIDER_ID),
        "原子持久化失败不得把 On-device 标记为 active Provider"
    );
}
