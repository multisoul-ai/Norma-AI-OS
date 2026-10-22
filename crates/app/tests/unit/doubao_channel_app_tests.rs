use crate::app::doubao::DoubaoStatus;
use crate::app::MultisoulApp;
use crate::settings::voice::{
    DesktopVoiceMode, EnvVoiceSettings, SavedVoiceSettings, VoiceInteractionMode,
    VoiceSettingsState,
};
use gpui::{AppContext, TestAppContext};
use tokio_util::sync::CancellationToken;

/// 持续聆听模式下 Cmd+E 只开关收音，播报通道保持常驻。
///
/// 数据构造：豆包模式 + 持续聆听 + 不存在的凭据引用；通道以本地 channel 注入：
///   会话令牌 1 个、报告通道 1 个、收音请求通道 1 个；不使用麦克风或网络。
/// 执行过程：
///   1. 第 1 次 Cmd+E（activate_ready_voice）→ 下发一次收音请求。
///   2. 第 2 次 Cmd+E → 停止本次收音。
/// 预期结果：第 1 次收到 id 1 的收音令牌，listening 存在；第 2 次令牌被取消、listening 清空；
///   两次操作后会话令牌未取消、报告通道仍在，通道 is_active 为真。
#[test]
fn doubao_shortcut_toggles_listening_without_closing_channel() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主");
    let session = CancellationToken::new();
    let (reports, _report_rx) = async_channel::unbounded();
    let (listens, listen_rx) = async_channel::unbounded();
    host.update_in(cx, |app, window, cx| {
        app.voice_settings = VoiceSettingsState::from_sources(
            SavedVoiceSettings {
                desktop_mode: Some(DesktopVoiceMode::Doubao),
                doubao_credential_ref: Some("not-saved-test-account".into()),
                doubao_interaction_mode: Some(VoiceInteractionMode::ContinuousListening),
                ..Default::default()
            },
            EnvVoiceSettings::default(),
        );
        app.doubao.cancellation = Some(session.clone());
        app.doubao.reports = Some(reports);
        app.doubao.listens = Some(listens);
        app.doubao.status = DoubaoStatus::Connected;
        app.activate_ready_voice(false, window, cx);
    });
    let (id, token) = listen_rx.try_recv().expect("Cmd+E 应下发一次收音");
    assert_eq!(id, 1, "第一次收音 id 应为 1");
    host.read_with(cx, |app, _| {
        assert!(app.doubao.listening.is_some(), "收音中应持有收音令牌");
        assert!(!app.doubao.waiting_for_input(), "收音中不能显示待命");
    });
    host.update_in(cx, |app, window, cx| {
        app.activate_ready_voice(false, window, cx)
    });
    host.read_with(cx, |app, _| {
        assert!(token.is_cancelled(), "第二次 Cmd+E 应停止收音，释放麦克风");
        assert!(app.doubao.listening.is_none(), "停止后不应保留收音令牌");
        assert!(!session.is_cancelled(), "停止收音不能关闭播报通道");
        assert!(app.doubao.is_active(), "播报通道应保持常驻");
        assert!(app.doubao.reports.is_some(), "报告通道应保留，通知照常播报");
    });
    assert!(listen_rx.try_recv().is_err(), "停止收音不能再下发新的收音");
}

/// 已排队的通道启动在执行前若不再需要（切到 GPT Live），不能再启动豆包会话。
///
/// 数据构造：豆包模式 + 不存在的凭据引用 + 已打开的项目 + 当前窗口作为项目窗口。
/// 执行过程：reconcile 排队启动 → 执行前切到 GPT Live 并停止豆包 → 运行到空闲。
/// 预期结果：豆包通道未启动，状态仍为 Idle，没有多余的错误状态。
#[test]
fn doubao_scheduled_start_rechecks_settings() {
    let workspace = tempfile::tempdir().expect("应创建隔离工作区");
    let mut test = TestAppContext::single();
    let (host, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    host.update_in(cx, |app, window, cx| {
        app.enter_canvas_for_test(workspace.path(), cx);
        app.project_window = Some(window.window_handle());
        app.voice_settings = settings(DesktopVoiceMode::Doubao);
        app.reconcile_doubao_channel(cx);
        assert!(app.doubao.start_pending, "项目就绪应排队启动通道");
        app.voice_settings = settings(DesktopVoiceMode::GptLive);
        app.doubao.stop();
    });
    cx.run_until_parked();
    host.read_with(cx, |app, _| {
        assert!(!app.doubao.is_active(), "已不需要的排队启动不能再建立通道");
        assert_eq!(app.doubao.status, DoubaoStatus::Idle, "不能留下多余状态");
    });
}

fn settings(mode: DesktopVoiceMode) -> VoiceSettingsState {
    VoiceSettingsState::from_sources(
        SavedVoiceSettings {
            desktop_mode: Some(mode),
            doubao_credential_ref: Some("not-saved-test-account".into()),
            ..Default::default()
        },
        EnvVoiceSettings::default(),
    )
}
