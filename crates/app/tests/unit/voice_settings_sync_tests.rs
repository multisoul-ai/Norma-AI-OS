use crate::app::{doubao::DoubaoStatus, MultisoulApp};
use crate::settings::voice::{DesktopVoiceMode, VoiceSettingsState};
use gpui::TestAppContext;
use tokio_util::sync::CancellationToken;

fn persist(state: &mut VoiceSettingsState) {
    for write in state.take_pending_writes() {
        write.execute().expect("隔离语音设置应成功落库");
    }
    state.record_persistence_result(Ok(()));
}

/// 远端切回 Legacy：同步设置必须取消原豆包通话，而不是只切换界面上的模式。
///
/// 数据构造（含关键数值的推导过程）：
///   同一隔离设置库上的写入者和接收者各 1 个；接收者持有 1 个真实取消令牌。
///   本地汇报通道代替网络边界，不创建麦克风或远端连接；模式由 Doubao 改为 Legacy。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入 Doubao 模式，接收者从库加载并进入已连接状态。
///   2. 写入者切回 Legacy 并落库，接收者通过生产 resync 接口读取新配置。
///   3. 推进异步执行器，检查通话取消令牌和输出通道。
///
/// 预期结果：
///   - 正断言：配置变为 Legacy，旧令牌已取消，通话状态为 Idle。
///   - 负断言：旧通话不再活跃，旧汇报通道不再被会话持有。
#[test]
fn remote_mode_switch_cancels_active_doubao_session() {
    let temp = tempfile::tempdir().expect("应创建隔离设置目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut writer = VoiceSettingsState::load_from_store(store.clone());
    writer.select_desktop_mode(DesktopVoiceMode::Doubao);
    persist(&mut writer);

    let mut test = TestAppContext::single();
    let (app, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    let cancellation = CancellationToken::new();
    let (sender, _reports) = async_channel::unbounded();
    app.update(cx, |app, _| {
        app.voice_settings = VoiceSettingsState::load_from_store(store);
        app.doubao.cancellation = Some(cancellation.clone());
        app.doubao.status = DoubaoStatus::Connected;
        app.doubao.reports = Some(sender);
    });

    writer.select_desktop_mode(DesktopVoiceMode::Legacy);
    persist(&mut writer);
    app.update(cx, |app, cx| app.resync_voice_settings(false, cx));
    cx.run_until_parked();

    app.read_with(cx, |app, _| {
        assert_eq!(
            app.voice_settings.values().desktop_mode(),
            DesktopVoiceMode::Legacy,
            "接收者应采用远端保存的 Legacy 模式"
        );
        assert!(
            cancellation.is_cancelled(),
            "旧通话令牌必须取消，以停止网络和麦克风任务"
        );
        assert_eq!(app.doubao.status, DoubaoStatus::Idle, "旧通话应回到 Idle");
        assert!(!app.doubao.is_active(), "切换模式后不能继续维持旧通话");
        assert!(app.doubao.reports.is_none(), "旧通话不能继续持有汇报通道");
    });
}

/// Legacy 切到豆包：旧录音和持续聆听的迟到启动均须失效。
///
/// 数据构造：同一隔离设置库的写入者与接收者各 1 个，接收者记录 1 次录音 generation
///   和 1 次聆听 startup attempt；只建立运行态，不启动麦克风或识别器。
///
/// 执行过程：
///   1. 接收者在 Legacy 下打开录音状态和持续聆听启动尝试。
///   2. 写入者保存 Doubao 模式，接收者通过生产 resync 同步并推进异步执行器。
///   3. 检查旧录音和旧启动令牌的接收条件。
///
/// 预期结果：
///   - 正断言：模式为 Doubao，录音状态为 Idle。
///   - 负断言：旧录音结果和旧聆听启动回调不能继续生效。
#[test]
fn remote_mode_switch_invalidates_legacy_recording_and_startup() {
    use crate::app::voice::control::{VoicePhase, VoiceTrigger};
    let temp = tempfile::tempdir().expect("应创建隔离设置目录");
    let store = storage::SettingsStore::new(temp.path().join("app.sqlite"));
    let mut writer = VoiceSettingsState::load_from_store(store.clone());
    let mut test = TestAppContext::single();
    let (app, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    let (recording, startup) = app.update(cx, |app, _| {
        app.voice_settings = VoiceSettingsState::load_from_store(store);
        assert!(
            app.voice.begin_recording(VoiceTrigger::Shortcut),
            "旧 Legacy 录音应已开始"
        );
        (
            app.voice.recording_generation(),
            app.dialogue.begin_startup_attempt(),
        )
    });
    writer.select_desktop_mode(DesktopVoiceMode::Doubao);
    persist(&mut writer);
    app.update(cx, |app, cx| app.resync_voice_settings(false, cx));
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert_eq!(
            app.voice_settings.values().desktop_mode(),
            DesktopVoiceMode::Doubao,
            "应采用远端豆包模式"
        );
        assert_eq!(app.voice.phase(), &VoicePhase::Idle, "旧录音必须停止");
        assert!(
            !app.voice.accepts_recording_result(recording),
            "旧录音结果必须失效"
        );
        assert!(
            !app.dialogue.should_accept_startup_attempt(startup),
            "旧聆听启动必须失效"
        );
    });
}

/// 跨窗口保存模式：同库宿主、桌面子项目和无窗口项目都同步，另一设置库不受影响。
///
/// 数据构造（含关键数值的推导过程）：
///   同库 = 2 个窗口宿主 + 1 个来源子项目 + 1 个接收子项目 + 1 个后台项目 = 5 个实体。
///   接收子项目同时登记到项目注册表，模拟真实双重登记；另一库有 1 个独立窗口。
///   接收子项目和后台项目各有 1 个通话取消令牌，不使用麦克风或网络。
///
/// 执行过程：
///   1. 所有实体加载 Doubao；登记两个宿主及后台项目，来源绑定第一个宿主。
///   2. 来源改为 Legacy、落库，再调用生产广播入口，推进异步执行器。
///   3. 逐一检查跨窗口和后台项目的模式与旧通话状态。
///
/// 预期结果：同库全部采用 Legacy 并停止旧通话；另一库保持 Doubao，不能跨库广播。
#[test]
fn broadcast_mode_change_reaches_other_windows_and_background_projects() {
    use gpui::AppContext;
    let temp = tempfile::tempdir().expect("应创建隔离设置目录");
    let store = storage::SettingsStore::new(temp.path().join("shared.sqlite"));
    let other_store = storage::SettingsStore::new(temp.path().join("other.sqlite"));
    let mut writer = VoiceSettingsState::load_from_store(store.clone());
    writer.select_desktop_mode(DesktopVoiceMode::Doubao);
    persist(&mut writer);
    let mut other_writer = VoiceSettingsState::load_from_store(other_store.clone());
    other_writer.select_desktop_mode(DesktopVoiceMode::Doubao);
    persist(&mut other_writer);
    let build = |cx: &mut gpui::Context<MultisoulApp>, store| {
        MultisoulApp::new_with_pty_factory_and_voice_settings_store(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            store,
        )
    };
    let mut test = TestAppContext::single();
    let first_window = test.add_window(|_, cx| build(cx, store.clone()));
    let second_window = test.add_window(|_, cx| build(cx, store.clone()));
    let other_window = test.add_window(|_, cx| build(cx, other_store.clone()));
    let first = first_window.entity(&test).expect("第一个宿主应存在");
    let second = second_window.entity(&test).expect("第二个宿主应存在");
    let other = other_window.entity(&test).expect("另一库宿主应存在");
    let source = test.new(|cx| build(cx, store.clone()));
    let target = test.new(|cx| build(cx, store.clone()));
    let background = test.new(|cx| build(cx, store.clone()));
    let target_cancel = CancellationToken::new();
    let background_cancel = CancellationToken::new();
    test.update(|cx| {
        crate::workspace::desktop::sessions::register(
            store.path().to_owned(),
            first_window.into(),
            first.clone(),
            cx,
        );
        crate::workspace::desktop::sessions::register(
            store.path().to_owned(),
            second_window.into(),
            second.clone(),
            cx,
        );
        crate::workspace::desktop::sessions::register(
            other_store.path().to_owned(),
            other_window.into(),
            other.clone(),
            cx,
        );
        first.update(cx, |app, _| {
            app.desktop_workspace
                .sessions
                .insert(("desktop-a".into(), "source".into()), source.clone());
        });
        second.update(cx, |app, _| {
            app.desktop_workspace
                .sessions
                .insert(("desktop-b".into(), "target".into()), target.clone());
        });
        source.update(cx, |app, _| app.desktop_host = Some(first.downgrade()));
        target.update(cx, |app, _| {
            app.doubao.cancellation = Some(target_cancel.clone());
            app.doubao.status = DoubaoStatus::Connected;
        });
        background.update(cx, |app, _| {
            app.doubao.cancellation = Some(background_cancel.clone());
            app.doubao.status = DoubaoStatus::Connected;
        });
        crate::workspace::sessions::register(
            store.path().to_owned(),
            "target".into(),
            target.clone(),
            cx,
        );
        crate::workspace::sessions::register(
            store.path().to_owned(),
            "background".into(),
            background.clone(),
            cx,
        );
        source.update(cx, |app, cx| {
            app.voice_settings
                .select_desktop_mode(DesktopVoiceMode::Legacy);
            persist(&mut app.voice_settings);
            app.broadcast_voice_settings_changed(false, cx);
        });
    });
    test.run_until_parked();
    test.read(|cx| {
        assert_eq!(
            first.read(cx).voice_settings.values().desktop_mode(),
            DesktopVoiceMode::Legacy,
            "来源宿主必须同步"
        );
        assert_eq!(
            second.read(cx).voice_settings.values().desktop_mode(),
            DesktopVoiceMode::Legacy,
            "另一窗口宿主必须同步"
        );
        assert_eq!(
            source.read(cx).voice_settings.values().desktop_mode(),
            DesktopVoiceMode::Legacy,
            "来源项目必须保持最新值"
        );
        assert_eq!(
            target.read(cx).voice_settings.values().desktop_mode(),
            DesktopVoiceMode::Legacy,
            "另一窗口子项目必须同步"
        );
        assert_eq!(
            background.read(cx).voice_settings.values().desktop_mode(),
            DesktopVoiceMode::Legacy,
            "无窗口项目必须同步"
        );
        assert!(target_cancel.is_cancelled(), "另一窗口旧通话必须取消");
        assert!(background_cancel.is_cancelled(), "后台项目旧通话必须取消");
        assert!(
            !target.read(cx).doubao.is_active(),
            "接收子项目不能保留旧通话"
        );
        assert!(
            !background.read(cx).doubao.is_active(),
            "后台项目不能保留旧通话"
        );
        assert_eq!(
            other.read(cx).voice_settings.values().desktop_mode(),
            DesktopVoiceMode::Doubao,
            "另一设置库不能受广播影响"
        );
    });
}
