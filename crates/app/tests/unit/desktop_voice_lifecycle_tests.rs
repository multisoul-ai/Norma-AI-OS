use super::*;
use crate::app::AppScreen;
use crate::settings::voice::{
    DesktopVoiceMode, EnvVoiceSettings, SavedVoiceSettings, VoiceInteractionMode,
    VoiceSettingsState,
};
use gpui::TestAppContext;
use tokio_util::sync::CancellationToken;

/// 按住豆包录音时切项目：旧项目不会收到松键，切换入口必须主动停止旧采集和通话。
///
/// 数据构造（含关键数值的推导过程）：
///   1 个桌面工作空间含 2 张已加载画布 A/B，共享同一窗口；只使用隔离目录。
///   A 持有会话与录音取消令牌各 1 个；B 持有独立会话令牌 1 个，无麦克风和网络。
///   reduced_motion=true，直接检查切换结束状态，不依赖动画计时。
///
/// 执行过程：
///   1. 经生产加载入口创建 A/B，A 模拟按住录音，B 持有独立通话状态。
///   2. 通过生产 expand_desktop_project_area 从 A 切到 B，不向 A 发送松键事件。
///   3. 核对旧令牌、generation、通道和项目窗口绑定，确认 B 未被误停。
///
/// 预期结果：A 的会话及录音均取消、输出通道清空；B 展开且会话令牌仍有效。
#[test]
fn switching_project_cancels_old_doubao_hold_without_key_up() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化数据库");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Voice lifecycle").expect("应创建空间");
    let area_a = catalog
        .add_project(&workspace.id, &root)
        .expect("应创建画布 A");
    let area_b = catalog
        .add_project(&workspace.id, &root)
        .expect("应创建画布 B");
    let mut test = TestAppContext::single();
    let (host, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.areas = vec![area_a.clone(), area_b.clone()];
        app.ensure_desktop_area_session(area_a.id.clone(), window, cx);
        app.ensure_desktop_area_session(area_b.id.clone(), window, cx);
    });
    cx.run_until_parked();
    let (a, b) = host.read_with(cx, |app, _| {
        (
            app.desktop_workspace.sessions[&(workspace.id.clone(), area_a.id.clone())].clone(),
            app.desktop_workspace.sessions[&(workspace.id.clone(), area_b.id.clone())].clone(),
        )
    });
    let session = CancellationToken::new();
    let recording = CancellationToken::new();
    let other_session = CancellationToken::new();
    let (reports, _receiver) = async_channel::unbounded();
    let settings = || {
        VoiceSettingsState::from_sources(
            SavedVoiceSettings {
                desktop_mode: Some(DesktopVoiceMode::Doubao),
                doubao_interaction_mode: Some(VoiceInteractionMode::SingleVoiceCommand),
                ..Default::default()
            },
            EnvVoiceSettings::default(),
        )
    };
    let old_generation = a.update_in(cx, |app, window, _| {
        app.voice_settings = settings();
        app.project_window = Some(window.window_handle());
        app.background_project = false;
        app.doubao.cancellation = Some(session.clone());
        app.doubao.recording = Some(recording.clone());
        app.doubao.recording_from_shortcut = true;
        app.doubao.reports = Some(reports);
        app.doubao.generation
    });
    b.update(cx, |app, _| {
        app.voice_settings = settings();
        app.doubao.cancellation = Some(other_session.clone());
    });
    host.update_in(cx, |app, window, cx| {
        app.screen = AppScreen::DesktopWorkspace;
        app.desktop_workspace.expanded_area_id = Some(area_a.id.clone());
        app.expand_desktop_project_area(area_b.id.clone(), window, cx);
    });
    cx.run_until_parked();
    a.read_with(cx, |app, _| {
        assert!(
            session.is_cancelled(),
            "切项目必须停止旧通话，不能等旧项目收不到的松键事件"
        );
        assert!(recording.is_cancelled(), "旧麦克风采集必须取消");
        assert!(!app.doubao.is_active(), "后台旧项目不能继续持有通话");
        assert!(app.doubao.recording.is_none(), "旧项目不能保留录音令牌");
        assert!(app.doubao.reports.is_none(), "旧通话不能保留输出通道");
        assert_ne!(
            app.doubao.generation, old_generation,
            "旧通话迟到状态必须失效"
        );
        assert!(app.project_window.is_none(), "旧项目应解除窗口绑定");
    });
    b.read_with(cx, |app, _| {
        assert!(!other_session.is_cancelled(), "旧项目取消不能误停新项目");
        assert!(app.doubao.is_active(), "新项目会话应保持有效");
        assert!(app.project_window.is_some(), "新项目应绑定当前窗口");
        assert!(!app.background_project, "新项目不能仍标记为后台");
    });
    host.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_deref(),
            Some(area_b.id.as_str()),
            "最终展开项目应为 B"
        );
        assert_ne!(
            app.desktop_workspace.expanded_area_id.as_deref(),
            Some(area_a.id.as_str()),
            "旧项目不能继续接收语音键事件"
        );
    });
}
