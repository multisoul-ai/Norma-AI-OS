use super::*;
use crate::settings::voice::{DesktopVoiceMode, VoiceInteractionMode};
use gpui::{px, size, AppContext, TestAppContext};
use tokio_util::sync::CancellationToken;

/// 窗口失活导致松键事件丢失时，结束当前录音并保留识别及回复会话。
///
/// 数据构造（含关键数值的推导过程）：一个隔离 App、一个会话 token 和一个录音 token；
///   只保存不存在的凭据引用，不使用真实麦克风、密钥或网络连接。
/// 执行过程：通过生产入口安装窗口失活观察器 → 标记按住录音 → 模拟窗口失活
///   → 检查录音结束、会话保留和 Thinking 状态 → 显式停止。
/// 预期结果：录音 token 取消但会话 token 不取消；界面不能直接退回 Idle 丢弃结果。
#[test]
fn test_doubao_hold_window_deactivation_preserves_pending_reply() {
    use crate::settings::voice::{EnvVoiceSettings, SavedVoiceSettings, VoiceSettingsState};
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
    let recording = CancellationToken::new();
    let session = host.update_in(cx, |app, window, cx| {
        app.voice_settings = VoiceSettingsState::from_sources(
            SavedVoiceSettings {
                doubao_credential_ref: Some("not-saved-test-account".into()),
                doubao_interaction_mode: Some(VoiceInteractionMode::SingleVoiceCommand),
                ..Default::default()
            },
            EnvVoiceSettings::default(),
        );
        app.voice_settings
            .select_desktop_mode(DesktopVoiceMode::Doubao);
        app.toggle_doubao_voice(window, cx);
        let session = app.doubao.cancellation.clone().expect("应创建会话");
        app.doubao.recording = Some(recording.clone());
        app.doubao.recording_from_shortcut = true;
        window.activate_window();
        session
    });
    cx.run_until_parked();
    cx.deactivate_window();
    cx.run_until_parked();
    host.update(cx, |app, cx| {
        assert!(
            recording.is_cancelled(),
            "失活后必须结束录音，不能等待收不到的松键事件"
        );
        assert!(
            !session.is_cancelled(),
            "窗口失活不能取消等待识别和 AI 回复的会话"
        );
        assert!(app.doubao.is_active(), "失活后应保留本段录音的会话");
        assert_eq!(
            app.doubao.status,
            crate::app::doubao::DoubaoStatus::Thinking,
            "失活后必须等待转写和回复"
        );
        assert_ne!(
            app.doubao.status,
            crate::app::doubao::DoubaoStatus::Idle,
            "失活不能直接恢复语音按钮并丢弃内容"
        );
        app.stop_doubao_voice(cx);
    });
}

/// 豆包设置的模式按钮可操作；松 E 或先松 Command 只提交快捷键录音一次。
///
/// 数据构造（含关键数值的推导过程）：隔离数据库；1280×800 桌面窗口；
///   三个录音 token 分别模拟松 E、先松 Command、按钮录音；无真实音频或网络请求。
/// 执行过程：打开豆包设置 → 点击按住说话 → 采集中注入旧 Speaking 状态
///   → 模拟按键释放与重复释放
///   → 模拟修饰键释放 → 验证按钮录音不被键盘释放打断 → 点击实时对话。
/// 预期结果：模式实际切换；旧 Speaking 不覆盖采集；快捷键录音结束而会话保留；
///   按钮录音未取消；实时模式可恢复。
#[test]
fn test_doubao_hold_keyboard_release_and_mode_controls() {
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
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    let host = host.expect("应创建宿主");
    host.update_in(cx, |app, window, cx| {
        app.voice_settings
            .select_desktop_mode(DesktopVoiceMode::Doubao);
        app.open_voice_settings(window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let hold = cx
        .debug_bounds("settings-doubao-mode-hold")
        .expect("豆包设置应显示按住说话");
    assert!(
        cx.debug_bounds("settings-doubao-mode-realtime").is_some(),
        "豆包设置应同时提供实时对话选项"
    );
    cx.simulate_click(hold.center(), Modifiers::default());
    cx.run_until_parked();
    let session = CancellationToken::new();
    let key_recording = CancellationToken::new();
    host.update_in(cx, |app, window, cx| {
        assert_eq!(
            app.voice_settings.values().doubao_interaction_mode(),
            VoiceInteractionMode::SingleVoiceCommand,
            "点击必须真正切换到按住模式"
        );
        app.doubao.cancellation = Some(session.clone());
        app.doubao.recording = Some(key_recording.clone());
        app.doubao.recording_from_shortcut = true;
        app.doubao
            .apply_status(crate::app::doubao::DoubaoStatus::Speaking);
        assert_eq!(
            app.doubao.status,
            crate::app::doubao::DoubaoStatus::Connected,
            "旧回复的 Speaking 状态不能覆盖正在采集的录音"
        );
        assert_ne!(
            app.doubao.status,
            crate::app::doubao::DoubaoStatus::Speaking,
            "录音期间不能显示 AI 正在回复"
        );
        let key = Keystroke::parse("cmd-e").expect("快捷键应有效");
        assert!(
            app.handle_configured_voice_hold_release(&key, window, cx),
            "松 E 应处理豆包录音"
        );
        assert!(
            !app.handle_configured_voice_hold_release(&key, window, cx),
            "重复松键不能再次提交"
        );
        assert!(key_recording.is_cancelled(), "松键必须停止录音");
        app.doubao
            .apply_status(crate::app::doubao::DoubaoStatus::Speaking);
        assert_eq!(
            app.doubao.status,
            crate::app::doubao::DoubaoStatus::Thinking,
            "松键后等待当前转写时不能被旧 Speaking 覆盖"
        );
        assert_ne!(
            app.doubao.status,
            crate::app::doubao::DoubaoStatus::Speaking,
            "转写完成前不能显示旧回复"
        );
        app.doubao
            .apply_status(crate::app::doubao::DoubaoStatus::RecordingResolved(1));
        app.doubao
            .apply_status(crate::app::doubao::DoubaoStatus::Speaking);
        assert_eq!(
            app.doubao.status,
            crate::app::doubao::DoubaoStatus::Thinking,
            "过期录音 id 不能解除当前录音的转写等待"
        );
        // 本测试未启动真实采集，当前 recording_id 保持默认值 0；id 1 是过期事件。
        app.doubao
            .apply_status(crate::app::doubao::DoubaoStatus::RecordingResolved(0));
        app.doubao
            .apply_status(crate::app::doubao::DoubaoStatus::Speaking);
        assert_eq!(
            app.doubao.status,
            crate::app::doubao::DoubaoStatus::Speaking,
            "当前转写完成后应恢复真实回复状态"
        );
        assert!(
            !session.is_cancelled(),
            "松键不能取消等待识别和 AI 回复的会话"
        );
        let modifier_recording = CancellationToken::new();
        app.doubao.recording = Some(modifier_recording.clone());
        app.doubao.recording_from_shortcut = true;
        app.handle_configured_voice_modifier_release(&Modifiers::default(), window, cx);
        assert!(
            modifier_recording.is_cancelled(),
            "先松 Command 也必须停止录音"
        );
        let pointer_recording = CancellationToken::new();
        app.doubao.recording = Some(pointer_recording.clone());
        app.doubao.recording_from_shortcut = false;
        app.handle_configured_voice_modifier_release(&Modifiers::default(), window, cx);
        assert!(
            !app.handle_configured_voice_hold_release(&key, window, cx),
            "键盘释放不能提交按钮录音"
        );
        assert!(
            !pointer_recording.is_cancelled(),
            "按钮录音应等待第二次点击"
        );
        app.stop_doubao_voice(cx);
        assert!(
            pointer_recording.is_cancelled(),
            "停止会话必须取消仍在采集的按钮录音"
        );
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let realtime = cx
        .debug_bounds("settings-doubao-mode-realtime")
        .expect("应保留实时选项");
    cx.simulate_click(realtime.center(), Modifiers::default());
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert_eq!(
            app.voice_settings.values().doubao_interaction_mode(),
            VoiceInteractionMode::ContinuousListening,
            "点击实时选项应恢复连续对话"
        );
        assert_ne!(
            app.voice_settings.values().doubao_interaction_mode(),
            VoiceInteractionMode::SingleVoiceCommand,
            "切换不能仍停留在按住模式"
        );
    });
}
