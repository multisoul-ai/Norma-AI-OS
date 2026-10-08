use super::*;
use crate::settings::voice::{EnvVoiceSettings, SavedVoiceSettings};
use gpui::TestAppContext;

/// ASR 安装先失败、旧扫描后返回：扫描不能把终态错误覆盖为未安装。
///
/// 数据构造（含关键数值的推导过程）：
///   1 个接收会话和空模型目录；模拟 1 个 Zipformer 下载，不发网络请求。
///   安装进度 = 0 / 目录提供的 archive_bytes；1 个已排队扫描只会发现目录缺失。
///   安装失败原因 = "download failed"，必须在后台回调后仍保留该具体错误。
///
/// 执行过程：
///   1. 标记模型 Installing，调用生产重扫入口排队旧扫描。
///   2. 通过生产安装失败入口先处理 Failed，移除安装令牌并记录错误。
///   3. 推进执行器让旧扫描完成并回调，核对终态错误未被覆盖。
///
/// 预期结果：失败原因保留且下载令牌释放；模型不能退回 NotInstalled 或伪报 Ready。
#[test]
fn failed_install_is_not_overwritten_by_older_model_scan() {
    let temp = tempfile::tempdir().expect("应创建隔离模型目录");
    let id = AsrModelId::parse(asr::ZIPFORMER_STREAMING_ZH_MODEL_ID).expect("模型 ID 应有效");
    let catalog = asr::builtin_model_catalog();
    let total = catalog
        .find(&id)
        .expect("模型应在目录中")
        .artifact()
        .archive_bytes();
    let mut test = TestAppContext::single();
    let (app, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    app.update(cx, |app, cx| {
        app.voice_settings = VoiceSettingsState::from_sources_with_model_root(
            SavedVoiceSettings {
                asr_model_id: Some(id.clone()),
                ..Default::default()
            },
            EnvVoiceSettings::default(),
            temp.path().to_owned(),
        );
        app.voice_settings
            .model_manager_mut()
            .begin_install(&id, total);
        app.asr_download_cancellations
            .insert(id.clone(), Arc::new(AtomicBool::new(false)));
        app.refresh_asr_model_states(cx);
        app.fail_asr_model_install(&id, "download failed".into());
        assert_eq!(
            app.voice_settings.model_manager().installation(&id),
            Some(&AsrModelInstallState::Failed("download failed".into())),
            "安装失败必须先进入具体错误终态"
        );
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert_eq!(
            app.voice_settings.model_manager().installation(&id),
            Some(&AsrModelInstallState::Failed("download failed".into())),
            "迟到扫描不能覆盖安装失败原因"
        );
        assert_ne!(
            app.voice_settings.model_manager().installation(&id),
            Some(&AsrModelInstallState::NotInstalled),
            "旧扫描不能把失败回退为未安装"
        );
        assert_ne!(
            app.voice_settings.model_manager().installation(&id),
            Some(&AsrModelInstallState::Ready),
            "失败安装不能伪报 Ready"
        );
        assert!(
            !app.asr_download_cancellations.contains_key(&id),
            "失败下载必须释放活跃安装令牌"
        );
    });
}
