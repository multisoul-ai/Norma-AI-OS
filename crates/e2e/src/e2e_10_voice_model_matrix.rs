//! E2E-10 Voice 模型矩阵：真实受管模型加载、切换、共享选择与持久化。

use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{
    echo_factory, enter_canvas, make_app_with_settings_store, scroll_settings_target_into_view,
    VoiceModelInstallE2eGuard,
};

/// 旧双语 Zipformer 安装缺少热词 BPE 词表时，Voice Settings 必须提供可恢复 Retry。
///
/// 数据构造（含关键数值的推导过程）：
///   managed models       = 4（内置 ASR catalog 固定四项）；
///   bilingual card index = 1（Paraformer 后的第二张卡）；
///   required files       = 5，其中旧安装保留 4 个并精确缺少 `bpe.vocab` 1 个；
///   network downloads    = 0（点击 Retry 时使用现有 e2e 假安装传输）。
///   wait budget          = 3s = 600 × 5ms 轮询，覆盖后台安装事件回传但避免无界等待。
///   scroll attempts      = 8，每次按目标与视口差值移动，24px 边距避免点击裁剪边缘。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在隔离 model root 写入双语 Zipformer 的合法 installation manifest 与旧四文件；
///   2. 启动 headless App，将 Voice Settings 指向隔离目录；
///   3. 打开 Voice Settings，后台扫描识别 `UpgradeRequired`；
///   4. 检查失败状态与 Retry 后，启用假安装、真实点击 Retry 并等待状态迁移。
///
/// 预期结果：
///   - 正断言：双语模型先显示 failed/retry，点击后迁移到 ready/selected；
///   - 负断言：升级前不得显示 Install/Select，升级后不得残留 Failed/Retry。
#[test]
fn e2e_10_legacy_bilingual_installation_renders_retry_upgrade_action() {
    let _install_guard = VoiceModelInstallE2eGuard::acquire();
    let temp = tempfile::TempDir::new().expect("应创建隔离的旧 ASR 模型目录");
    let model_root = temp.path().join("models");
    let catalog = asr::builtin_model_catalog();
    let model_id = asr::AsrModelId::parse(asr::ZIPFORMER_BILINGUAL_MODEL_ID)
        .expect("内置双语 Zipformer ID 应有效");
    let descriptor = catalog
        .find(&model_id)
        .expect("内置 catalog 应包含双语 Zipformer");
    let installation_root = model_root.join(model_id.as_str());
    std::fs::create_dir_all(&installation_root).expect("应创建隔离的模型安装目录");
    for required_file in descriptor
        .artifact()
        .required_files()
        .iter()
        .filter(|file| file.as_str() != "bpe.vocab")
    {
        std::fs::write(installation_root.join(required_file), b"legacy")
            .expect("应写入旧安装已有的受控文件占位");
    }
    asr::InstallationManifest::for_descriptor(descriptor)
        .write_to(&installation_root)
        .expect("应写入与 catalog 匹配的 installation manifest");

    let store = storage::SettingsStore::new(temp.path().join("voice-upgrade.sqlite"));
    let workspace = tempfile::TempDir::new().expect("应创建隔离的 Voice Settings workspace");
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), store);
    app.update(cx, |app, cx| {
        app.set_voice_model_root_for_e2e(model_root, cx)
    });
    enter_canvas(&app, cx, workspace.path());
    cx.simulate_keystrokes("cmd-,");
    click_debug(cx, "settings-section-voice");

    let snapshot = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        snapshot.asr_models.len(),
        4,
        "Voice Settings 扫描后仍应展示完整四模型 catalog"
    );
    assert_eq!(
        snapshot.asr_models[1].id,
        asr::ZIPFORMER_BILINGUAL_MODEL_ID,
        "第二张模型卡必须是待升级的双语 Zipformer"
    );
    assert_eq!(
        snapshot.asr_models[1].installation, "failed",
        "缺少 bpe.vocab 的旧安装必须进入失败/可恢复状态"
    );
    assert_eq!(
        snapshot.asr_models[1].action, "retry",
        "旧安装必须向用户暴露 Retry，而不是继续选择"
    );
    assert!(
        cx.debug_bounds("settings-asr-retry-1").is_some(),
        "双语模型卡必须在真实渲染树中提供可点击 Retry"
    );
    assert!(
        cx.debug_bounds("settings-asr-install-1").is_none(),
        "已存在但需升级的旧安装不得退化成普通 Install"
    );
    assert!(
        cx.debug_bounds("settings-asr-select-1").is_none(),
        "缺少 bpe.vocab 的旧安装不得暴露 Select"
    );
    assert_ne!(
        snapshot.asr_models[1].installation, "ready",
        "缺少热词 BPE 工件时不得错误报告 Ready"
    );

    app.update(cx, |app, _cx| {
        app.set_voice_setup_install_fakes_for_e2e(true)
    });
    scroll_settings_target_into_view(cx, "settings-asr-retry-1");
    click_debug(cx, "settings-asr-retry-1");
    let after_click = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_ne!(
        after_click.asr_models[1].action, "retry",
        "真实点击 Retry 后必须同步离开旧 Retry 状态，实际模型状态: {:?}",
        after_click.asr_models[1]
    );
    // deadline = 3s；poll interval = 5ms；最多 3,000 / 5 = 600 次。
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let upgraded = loop {
        cx.run_until_parked();
        let snapshot = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
        if snapshot.asr_models[1].action == "selected" || std::time::Instant::now() >= deadline {
            break snapshot;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    app.update(cx, |app, _cx| {
        app.set_voice_setup_install_fakes_for_e2e(false)
    });

    assert_eq!(
        upgraded.selected_asr_model_id.as_deref(),
        Some(asr::ZIPFORMER_BILINGUAL_MODEL_ID),
        "点击 Retry 并完成假安装后必须自动选择双语 Zipformer"
    );
    assert_eq!(
        upgraded.asr_models[1].installation, "ready",
        "Retry 完成后双语模型必须从 Failed 迁移到 Ready"
    );
    assert_eq!(
        upgraded.asr_models[1].action, "selected",
        "Retry 完成并自动选择后模型动作必须显示 Selected"
    );
    assert_ne!(
        upgraded.asr_models[1].installation, "failed",
        "成功重试后不得继续投影旧 Failed 状态"
    );
}

/// 受控 ASR 模型必须经 UI 与真实运行时加载切换，并由两种模式共享同一选择。
///
/// 数据构造（含关键数值的推导过程）：
///   managed models = 4（Paraformer + Zipformer Bilingual + 两个中文 Zipformer）
///   L2 installed    = 2（本用例真实加载 Paraformer 与 Zipformer Transducer）
///   runtime loads  = 2（每次点击 Select 都真实构造对应 Sherpa recognizer）
///   mode switches  = 2（Dictation → Continuous → Dictation）
///   app reloads    = 1（同一 SQLite store 创建第二个 App）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开 Voice Settings，扫描 Application Support 安装清单、必要文件与共享 VAD
///   2. 点击 Paraformer Select → 后台校验 Online Paraformer 加载 → 保存选择
///   3. 切到 Continuous Listening，确认选择仍为 Paraformer
///   4. 点击 Zipformer Select → 后台校验 Online Transducer/Joiner 加载 → 替换选择
///   5. 切回 Dictation，关闭 App；使用同一 SQLite store 重载并重新扫描模型
///
/// 预期结果：
///   - 正断言：已安装的两个模型均为 Ready，且分别能成为唯一 Selected 模型
///   - 正断言：模式切换和 App 重载后都保留最后选择的 Zipformer 不可变 ID
///   - 负断言：切换后旧 Paraformer 不得继续报告 Selected
///   - 负断言：当前 Zipformer 不得暴露 Delete 动作
#[test]
#[ignore = "L2 model UI smoke: requires managed Paraformer, Zipformer and Silero VAD installations"]
fn e2e_10_real_paraformer_and_zipformer_selection_is_shared_and_persistent() {
    let model_root = asr::default_model_installation_root()
        .expect("L2 模型测试需要可用的 Application Support 模型根");
    asr::validate_silero_vad_installation(&model_root)
        .expect("L2 模型测试需要校验通过的共享 Silero VAD");
    for model_id in [
        asr::PARAFORMER_MODEL_ID,
        asr::ZIPFORMER_STREAMING_ZH_MODEL_ID,
    ] {
        let id = asr::AsrModelId::parse(model_id).expect("内置模型 ID 应有效");
        let catalog = asr::builtin_model_catalog();
        let descriptor = catalog.find(&id).expect("受控模型库应包含 L2 测试模型");
        asr::validate_installation(&model_root.join(model_id), descriptor)
            .unwrap_or_else(|error| panic!("L2 模型 `{model_id}` 安装应有效: {error}"));
    }

    let temp = tempfile::TempDir::new().expect("应创建隔离的模型选择数据库目录");
    let store = storage::SettingsStore::new(temp.path().join("voice-model-matrix.sqlite"));
    let workspace = tempfile::TempDir::new().expect("应创建隔离的模型 e2e workspace");

    {
        let mut cx = TestAppContext::single();
        let (app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), store.clone());
        enter_canvas(&app, cx, workspace.path());
        cx.simulate_keystrokes("cmd-,");
        click_debug(cx, "settings-section-voice");

        let initial = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
        assert_eq!(initial.asr_models.len(), 4, "模型库应精确包含四个受控模型");
        assert_eq!(
            initial.asr_models[0].installation, "ready",
            "Paraformer 应为 Ready"
        );
        assert_eq!(
            initial.asr_models[2].installation, "ready",
            "Zipformer Streaming ZH 应为 Ready"
        );
        assert_eq!(
            initial.asr_models[1].display_name, "Zipformer Bilingual",
            "第二张模型卡应为 Zipformer Bilingual"
        );
        assert_eq!(
            initial.asr_models[3].display_name, "Zipformer Streaming CTC ZH",
            "第四张模型卡应为 Zipformer Streaming CTC ZH"
        );
        assert_eq!(
            initial.selected_asr_model_id, None,
            "fresh store 不应预选模型"
        );

        click_debug(cx, "settings-asr-select-0");
        let paraformer = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
        assert_eq!(
            paraformer.selected_asr_model_id.as_deref(),
            Some(asr::PARAFORMER_MODEL_ID),
            "点击并通过运行时加载后应选择 Paraformer"
        );
        assert_eq!(
            paraformer.asr_models[0].action, "selected",
            "Paraformer 应显示 Selected"
        );
        assert_ne!(
            paraformer.asr_models[2].action, "selected",
            "Zipformer 此时不得被误选"
        );

        click_debug(cx, "settings-voice-mode-continuous");
        let continuous = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
        assert_eq!(
            continuous.selected_asr_model_id.as_deref(),
            Some(asr::PARAFORMER_MODEL_ID),
            "切到 Continuous Listening 后应共享 Paraformer 选择"
        );

        click_debug(cx, "settings-asr-select-2");
        let zipformer = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
        assert_eq!(
            zipformer.selected_asr_model_id.as_deref(),
            Some(asr::ZIPFORMER_STREAMING_ZH_MODEL_ID),
            "点击并通过 Transducer 加载后应选择 Zipformer"
        );
        assert_eq!(
            zipformer.asr_models[2].action, "selected",
            "Zipformer 应显示 Selected"
        );
        assert_ne!(
            zipformer.asr_models[0].action, "selected",
            "切换后 Paraformer 不得仍为 Selected"
        );
        assert!(
            !zipformer.asr_models[2].deletable,
            "当前 Zipformer 不得暴露 Delete 动作"
        );

        click_debug(cx, "settings-voice-mode-dictation");
        let dictation = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
        assert_eq!(
            dictation.selected_asr_model_id.as_deref(),
            Some(asr::ZIPFORMER_STREAMING_ZH_MODEL_ID),
            "切回 Dictation 后仍应共享 Zipformer 选择"
        );
        cx.run_until_parked();
    }

    let mut reload_cx = TestAppContext::single();
    let (reloaded_app, reload_cx) =
        make_app_with_settings_store(&mut reload_cx, echo_factory(), store);
    enter_canvas(&reloaded_app, reload_cx, workspace.path());
    reload_cx.simulate_keystrokes("cmd-,");
    click_debug(reload_cx, "settings-section-voice");
    let reloaded = reloaded_app.update(reload_cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        reloaded.selected_asr_model_id.as_deref(),
        Some(asr::ZIPFORMER_STREAMING_ZH_MODEL_ID),
        "App 重载后应从 SQLite 恢复 Zipformer 不可变 ID"
    );
    assert_eq!(
        reloaded.asr_models[2].installation, "ready",
        "重载扫描后 Zipformer 仍应 Ready"
    );
    assert_eq!(
        reloaded.asr_models[2].action, "selected",
        "重载后 Zipformer 应恢复 Selected 动作"
    );
    assert_ne!(
        reloaded.asr_models[0].action, "selected",
        "重载后 Paraformer 不得恢复成旧选择"
    );
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("selector `{selector}` 应有可点击 bounds"));
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}
