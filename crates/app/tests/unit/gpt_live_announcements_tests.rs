use super::*;
use crate::settings::voice::DesktopVoiceMode;
use gpui::TestAppContext;

/// AC13：GPT-Live模式不能使用旧TTS播放任务播报，避免与Live语音叠加。
/// 数据构造：隔离App，旧e2e TTS明确Ready；先选Legacy再选GPT，未建立网络或设备。
/// 执行过程：查询两种模式的通知可用性，再尝试启动旧通知调度。
/// 预期结果：Legacy仍Ready；GPT不提供旧TTS可用性，旧播放器保持Idle。
#[test]
fn gpt_live_never_uses_legacy_announcement_tts() {
    let mut test = TestAppContext::single();
    let (app, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    app.update(cx, |app, cx| {
        app.agent_announcement_playback.e2e_enabled = true;
        app.agent_announcement_playback.e2e_tts_ready = true;
        app.voice_settings
            .select_desktop_mode(DesktopVoiceMode::Legacy);
        assert_eq!(
            app.agent_announcement_tts_availability(),
            AgentAnnouncementTtsAvailability::Ready,
            "旧模式仍可播报"
        );
        app.voice_settings
            .select_desktop_mode(DesktopVoiceMode::GptLive);
        assert_eq!(
            app.agent_announcement_tts_availability(),
            AgentAnnouncementTtsAvailability::Unavailable,
            "GPT模式不得回退旧TTS"
        );
        app.start_next_agent_announcement(cx);
        assert!(
            app.agent_announcement_playback.is_idle(),
            "旧播报播放器保持空闲"
        );
    });
}
