use super::*;
use crate::settings::voice::{VoiceRequirementState, VoiceSettingsState};
use audio::{PcmAudioChunk, PcmFormat};
use gpui::TestAppContext;

fn remote_voice_change_clears_old_audio(refresh_runtime: bool) {
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
    // PCM16 单声道 1 帧 = 1 × 2 字节，仅验证缓存生命周期，不实际播放。
    let pcm = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![1, 0]).expect("单帧 PCM 应有效");
    let old_generation = app.update(cx, |app, _| {
        app.voice_settings = VoiceSettingsState::load_from_store(store);
        app.voice_runtime_checked = !refresh_runtime;
        app.voice_settings
            .set_provider_requirement(VoiceRequirementState::Ready);
        app.workspace_state.current_root = Some(temp.path().to_owned());
        app.agent_announcement_audio_cache.insert(
            temp.path(),
            "Chloe",
            "Done",
            pcm.clone(),
            pcm.clone(),
            SystemTime::now(),
        );
        app.agent_announcement_playback.phase =
            AgentAnnouncementPlaybackPhase::Synthesizing(AgentAnnouncement::new(1, "Chloe, Done"));
        app.agent_announcement_playback.generation
    });
    let voice = tts::TtsVoiceId::parse("replacement-voice").expect("新音色 ID 应有效");
    writer.select_tts_model_and_voice(None, Some(voice.clone()));
    for write in writer.take_pending_writes() {
        write.execute().expect("新音色应成功落库");
    }
    writer.record_persistence_result(Ok(()));
    app.update(cx, |app, cx| app.resync_voice_settings(refresh_runtime, cx));
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        assert_eq!(
            app.voice_settings.values().tts_voice_id(),
            Some(&voice),
            "应采用远端新音色"
        );
        assert!(
            app.agent_announcement_playback.is_idle(),
            "旧音色合成必须取消"
        );
        assert_ne!(
            app.agent_announcement_playback.generation, old_generation,
            "旧合成回调必须失效"
        );
        assert!(
            app.agent_announcement_audio_cache
                .lookup(temp.path(), "Chloe", "Done", SystemTime::now())
                .is_none(),
            "旧音色 PCM 不能继续命中缓存"
        );
        // 模拟旧后台合成在配置同步后才结束，通过生产完成入口交付旧 generation。
        app.finish_agent_announcement_synthesis(
            old_generation,
            Some(temp.path().to_owned()),
            Ok(SynthesizedAgentTerminalAnnouncement {
                name: Arc::new(pcm.clone()),
                outcome: Arc::new(pcm.clone()),
                pcm: pcm.clone(),
            }),
            cx,
        );
        assert!(
            app.agent_announcement_playback.is_idle(),
            "迟到旧合成不能重新启动播放"
        );
        assert!(
            app.agent_announcement_audio_cache
                .lookup(temp.path(), "Chloe", "Done", SystemTime::now())
                .is_none(),
            "迟到旧合成不能污染新音色缓存"
        );
    });
}

/// 远端更换 TTS 音色：已扫描会话不能复用旧 PCM，迟到合成也不能回写。
///
/// 数据构造（含关键数值的推导过程）：
///   1 个隔离设置库、1 个接收会话；旧缓存 2 片段，每片段 1 帧 × 2 字节 = 2 字节。
///   1 个尚未完成的合成 generation；refresh_runtime=false，只同步配置。
///
/// 执行过程：
///   1. 写入新音色，通过生产 resync 读取并推进后台任务。
///   2. 检查旧合成已取消、旧 PCM 未命中；交付旧 generation 的迟到结果。
///
/// 预期结果：新音色生效；旧音频不能命中、播放或重新进入缓存。
#[test]
fn remote_tts_voice_change_invalidates_cached_and_inflight_announcements() {
    remote_voice_change_clears_old_audio(false);
}

/// 首次就绪扫描与远端换音色同时发生：完整扫描不能跳过 Agent 音频失效。
///
/// 数据构造（含关键数值的推导过程）：
///   旧缓存 = 2 片段 × 1 帧 × 2 字节 = 4 字节；接收会话尚未完成首次扫描。
///   refresh_runtime=true，同时触发首次扫描和新音色同步，不使用真实音频设备。
///
/// 执行过程：
///   1. 保存新音色、调用生产 resync 并推进首次扫描。
///   2. 交付旧 generation 的迟到合成，检查缓存与播放状态。
///
/// 预期结果：首次扫描仍取消旧合成并清缓存，迟到结果不能回写或播放。
#[test]
fn first_runtime_scan_also_invalidates_old_announcement_audio() {
    remote_voice_change_clears_old_audio(true);
}
