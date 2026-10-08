use super::*;
use crate::app::MultisoulApp;
use crate::settings::voice::{AsrModelInstallState, VoiceRequirement, VoiceRequirementState};
use gpui::TestAppContext;

fn persist(state: &mut VoiceSettingsState) {
    for write in state.take_pending_writes() {
        write.execute().expect("隔离设置应成功落库");
    }
    state.record_persistence_result(Ok(()));
}

/// 远端选择 ASR：已经扫描过的会话重新校验模型和 VAD，不能保留旧缺失缓存。
///
/// 数据构造（含关键数值的推导过程）：
///   1 个隔离设置库和模型根目录；初次扫描 0 个模型目录、0 个 VAD 文件。
///   后来增加 1 个无清单的 Zipformer 目录和 1 字节损坏 VAD 文件，不伪造有效模型。
///   期望模型与 VAD 从 Missing/NotInstalled 变为 Failed，证明实际文件系统已重扫。
///
/// 执行过程：
///   1. 接收会话生产扫描空目录并完成首次检测。
///   2. 构造损坏模型和 VAD，写入者保存 Zipformer 选择。
///   3. 接收者调用生产 resync，推进后台任务，检查模型与 VAD 缓存。
///
/// 预期结果：新模型已选且安装状态 Failed；VAD 为 Failed，不能残留旧缺失状态或伪报 Ready。
#[test]
fn remote_asr_selection_refreshes_model_and_vad_readiness() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let model_root = temp.path().join("models");
    let mut writer = VoiceSettingsState::load_from_store(store.clone());
    let id = asr::AsrModelId::parse(asr::ZIPFORMER_STREAMING_ZH_MODEL_ID).expect("模型 ID 应有效");
    let mut test = TestAppContext::single();
    let (app, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    app.update(cx, |app, cx| {
        app.voice_settings = VoiceSettingsState::load_from_store(store);
        app.voice_settings.model_root = model_root.clone();
        app.voice_runtime_checked = true;
        app.refresh_asr_model_states(cx);
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert_eq!(
            app.voice_settings.model_manager().installation(&id),
            Some(&AsrModelInstallState::NotInstalled),
            "初次扫描应发现模型未安装"
        );
        assert_eq!(
            app.voice_settings.vad_requirement,
            VoiceRequirementState::Missing,
            "初次扫描应发现 VAD 缺失"
        );
    });
    std::fs::create_dir_all(model_root.join(id.as_str())).expect("应创建损坏模型目录");
    let vad_root = model_root.join(asr::SILERO_VAD_DIRECTORY);
    std::fs::create_dir_all(&vad_root).expect("应创建 VAD 目录");
    std::fs::write(vad_root.join(asr::SILERO_VAD_FILE), [1_u8]).expect("应写入损坏 VAD");
    writer.select_asr_model(Some(id.clone()));
    persist(&mut writer);
    app.update(cx, |app, cx| app.resync_voice_settings(false, cx));
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert_eq!(
            app.voice_settings.values().asr_model_id(),
            Some(&id),
            "应采用远端 ASR 模型选择"
        );
        assert!(
            matches!(
                app.voice_settings.model_manager().installation(&id),
                Some(AsrModelInstallState::Failed(_))
            ),
            "必须重扫并发现损坏模型，不能沿用未安装缓存"
        );
        assert_ne!(
            app.voice_settings.model_manager().installation(&id),
            Some(&AsrModelInstallState::NotInstalled),
            "不能保留扫描前的旧模型缓存"
        );
        assert_eq!(
            app.voice_settings.vad_requirement,
            VoiceRequirementState::Failed,
            "同步 ASR 后必须重验 VAD 完整性"
        );
        assert_eq!(
            app.voice_settings
                .requirement_state(VoiceRequirement::SpeechRecognition),
            VoiceRequirementState::Failed,
            "就绪判断必须使用新扫描结果"
        );
        assert_ne!(
            app.voice_settings.vad_requirement,
            VoiceRequirementState::Ready,
            "损坏 VAD 不能被伪报为 Ready"
        );
    });
}

/// 同一模型重装完成：即使设置没有变化，也通知其它会话重扫，并保留本地安装进度。
///
/// 数据构造（含关键数值的推导过程）：
///   同库来源和接收者各 1 个，已选 Zipformer ID 始终不变；磁盘由缺失变为损坏目录。
///   接收者另有 Paraformer 安装，下载进度 = 0 / 目录提供的 archive_bytes，无真实下载。
///   VAD 由缺失变为 1 字节损坏文件，生产哈希校验应得 Failed。
///
/// 执行过程：
///   1. 选定 Zipformer，接收者扫描缺失目录；来源完成同 ID 安装，不产生设置写入。
///   2. 接收者标记另一个本地安装，来源调用生产广播，推进后台任务。
///   3. 核对接收者模型/VAD的新磁盘状态及另一个安装的进度。
///
/// 预期结果：重扫发现 Zipformer/VAD损坏；不会保留旧缺失缓存，也不能覆盖 Paraformer 安装态。
#[test]
fn unchanged_asr_selection_broadcast_refreshes_resources_and_preserves_install() {
    use gpui::AppContext;
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let model_root = temp.path().join("models");
    let id = asr::AsrModelId::parse(asr::ZIPFORMER_STREAMING_ZH_MODEL_ID).expect("模型 ID 应有效");
    let installing_id =
        asr::AsrModelId::parse(asr::PARAFORMER_MODEL_ID).expect("安装模型 ID 应有效");
    let catalog = asr::builtin_model_catalog();
    let total = catalog
        .find(&installing_id)
        .expect("模型应在目录中")
        .artifact()
        .archive_bytes();
    let mut writer = VoiceSettingsState::load_from_store(store.clone());
    writer.select_asr_model(Some(id.clone()));
    persist(&mut writer);
    let build = |cx: &mut gpui::Context<MultisoulApp>| {
        MultisoulApp::new_with_pty_factory_and_voice_settings_store(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            store.clone(),
        )
    };
    let mut test = TestAppContext::single();
    let (receiver, cx) = test.add_window_view(|_, cx| build(cx));
    let source = cx.new(build);
    receiver.update(cx, |app, cx| {
        app.voice_settings.model_root = model_root.clone();
        app.voice_runtime_checked = true;
        app.refresh_asr_model_states(cx);
        crate::workspace::sessions::register(
            store.path().to_owned(),
            "receiver".into(),
            cx.entity(),
            cx,
        );
    });
    cx.run_until_parked();
    receiver.read_with(cx, |app, _| {
        assert_eq!(
            app.voice_settings.model_manager().installation(&id),
            Some(&AsrModelInstallState::NotInstalled),
            "初始模型缓存应为缺失"
        );
    });
    std::fs::create_dir_all(model_root.join(id.as_str())).expect("应创建损坏模型目录");
    let vad_root = model_root.join(asr::SILERO_VAD_DIRECTORY);
    std::fs::create_dir_all(&vad_root).expect("应创建 VAD 目录");
    std::fs::write(vad_root.join(asr::SILERO_VAD_FILE), [1_u8]).expect("应写入损坏 VAD");
    receiver.update(cx, |app, _| {
        app.voice_settings
            .model_manager_mut()
            .begin_install(&installing_id, total);
        app.asr_download_cancellations.insert(
            installing_id.clone(),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
    });
    source.update(cx, |app, cx| {
        app.voice_settings.model_root = model_root.clone();
        assert!(
            !app.voice_settings.complete_asr_model_install(id.clone()),
            "重装已选模型不能产生另一选择"
        );
        assert!(
            app.voice_settings.take_pending_writes().is_empty(),
            "同 ID 重装不依赖设置落库通知"
        );
        app.broadcast_voice_settings_changed(true, cx);
    });
    cx.run_until_parked();
    receiver.read_with(cx, |app, _| {
        assert_eq!(
            app.voice_settings.values().asr_model_id(),
            Some(&id),
            "模型选择必须保持不变"
        );
        assert!(
            matches!(
                app.voice_settings.model_manager().installation(&id),
                Some(AsrModelInstallState::Failed(_))
            ),
            "资源广播必须重新扫描模型，不能沿用未安装缓存"
        );
        assert_eq!(
            app.voice_settings.vad_requirement,
            VoiceRequirementState::Failed,
            "资源广播也必须刷新共享 VAD"
        );
        assert_eq!(
            app.voice_settings
                .model_manager()
                .installation(&installing_id),
            Some(&AsrModelInstallState::Installing {
                downloaded: 0,
                total
            }),
            "重扫不能覆盖仍在下载的本地进度"
        );
        assert_ne!(
            app.voice_settings
                .model_manager()
                .installation(&installing_id),
            Some(&AsrModelInstallState::NotInstalled),
            "下载中的模型不能退回未安装"
        );
    });
}
