use crate::app::MultisoulApp;
use gpui::{AppContext, TestAppContext};

/// 云端声音启用时，扫描本地模型不能错误地提示回复只能显示文字。
///
/// 数据构造（含关键数值的推导过程）：
///   隔离数据库；本地模型选择为 None；云端 Provider = MiniMax。
///   先测试云端(1)再测试本地(1)，共 2 种来源，不连接网络。
/// 执行过程（逐步说明系统如何处理）：
///   1. 启用云端并设置已知状态，后台扫描 → 本地未选模型不覆盖云端状态。
///   2. 启用本地并再次扫描 → 缺少本地模型应显示文字回复提示。
///
/// 预期结果：
///   - 正断言：云端 Provider 保留；本地模式下显示缺模型提示。
///   - 负断言：云端模式下没有错误的 text-only 提示。
#[test]
fn voice_redesign_cloud_voice_ignores_missing_local_model_status() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let mut cx = TestAppContext::single();
    let host = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
    cx.run_until_parked();
    let cloud = tts::TtsProviderId::parse(tts::MINIMAX_TTS_PROVIDER_ID).expect("云端 ID 合法");
    host.update(&mut cx, |app, cx| {
        app.voice_settings.select_tts_provider(Some(cloud.clone()));
        app.voice_settings.select_tts_model_and_voice(None, None);
        app.voice_settings
            .report_tts_status("Cloud voice configured", false);
        app.refresh_tts_model_states(cx);
    });
    cx.run_until_parked();
    let status = host.read_with(&cx, |app, _| {
        app.voice_settings.status().map(|(m, _)| m.to_string())
    });
    assert_eq!(
        status.as_deref(),
        Some("Cloud voice configured"),
        "本地扫描不能覆盖云端声音的状态"
    );
    assert_eq!(
        host.read_with(&cx, |app, _| app
            .voice_settings
            .values()
            .active_tts_provider_id()
            .cloned()),
        Some(cloud),
        "本地扫描不能替换云端 Provider"
    );
    let missing =
        ui::i18n::text("voice.no_local_tts_model_is_selected_replies_will_stay_text_only");
    assert_ne!(
        status.as_deref(),
        Some(missing),
        "云端声音不应显示缺少本地模型的提示"
    );
    host.update(&mut cx, |app, cx| {
        let local = tts::TtsProviderId::parse(tts::LOCAL_TTS_PROVIDER_ID).expect("本地 ID 合法");
        app.voice_settings.select_tts_provider(Some(local));
        app.refresh_tts_model_states(cx);
    });
    cx.run_until_parked();
    assert_eq!(
        host.read_with(&cx, |app, _| app
            .voice_settings
            .status()
            .map(|(m, _)| m.to_string()))
            .as_deref(),
        Some(missing),
        "本地来源缺少模型时必须继续提示"
    );
}
