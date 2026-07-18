//! E2E-10 Voice 模型矩阵：真实受管模型加载、切换、共享选择与持久化。

use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app_with_settings_store};

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
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}
