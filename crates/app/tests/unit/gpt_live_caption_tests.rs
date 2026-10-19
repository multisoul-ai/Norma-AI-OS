use super::*;
use crate::{app::MultisoulApp, settings::voice::DesktopVoiceMode};
use gpui::TestAppContext;
use live_voice::{Speaker, TranscriptFragment};

/// 场景：用户移除GPT-Live悬浮转写框，通话和内部双向转写仍保留，旧模式不受影响。
/// 数据构造：隔离App与无网络通话状态；用户/助手各一条转写，另放一条旧模式字幕。
/// 执行过程：接收连接及转写→检查渲染/功能引导→切换旧模式→注入通话错误。
/// 预期结果：GPT无浮层但保留原文/通话并阻止引导；旧字幕不变；错误仍可见。
#[test]
fn gpt_live_caption_is_hidden_without_losing_transcripts() {
    let temp = tempfile::tempdir().expect("创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths).expect("初始化隔离数据库");
    let mut test = TestAppContext::single();
    let (app, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    app.update(cx, |app, cx| {
        app.voice_settings
            .select_desktop_mode(DesktopVoiceMode::GptLive);
        app.voice.set_transcript_only("旧模式字幕", "fixture");
        let (generation, media) = app.gpt_live.begin();
        app.gpt_live.apply(generation, CallEvent::Connected);
        for (id, speaker, text) in [
            ("user", Speaker::User, "创建浏览器"),
            ("assistant", Speaker::Assistant, "已收到请求"),
        ] {
            app.gpt_live.apply(
                generation,
                CallEvent::Transcript(TranscriptFragment {
                    event_id: id.into(),
                    speaker,
                    delta: text.into(),
                    start_ms: 0.0,
                    end_ms: 100.0,
                }),
            );
        }
        assert!(!app.voice_caption_is_visible(), "可见性不得保留旧浮层状态");
        assert!(
            app.render_voice_caption(cx).is_none(),
            "实际渲染入口不生成浮层"
        );
        let transcript = app.voice_caption_text().expect("内部转写仍可读取");
        assert!(transcript.contains("创建浏览器"), "内部用户原文仍保留");
        assert!(transcript.contains("已收到请求"), "内部助手原文仍保留");
        assert!(
            !transcript.contains("旧模式字幕"),
            "Live原文不能回退到旧模式内容"
        );
        assert!(app.gpt_live.is_active(), "移除浮层不挂断通话");
        assert!(!media.is_cancelled(), "媒体取消令牌仍有效");
        app.feature_guide = crate::app::feature_guide::FeatureGuideState::load(None, None, false);
        app.feature_guide.on_workspace_opened();
        app.settings_panel.open = false;
        app.window_active = true;
        app.present_feature_guide_if_ready(cx);
        assert!(app.feature_guide.is_pending(), "通话仍阻止自动功能引导");
        assert!(
            !app.feature_guide.is_visible(),
            "隐藏字幕不能让引导遮挡正在进行的通话"
        );
        for mode in [DesktopVoiceMode::Legacy, DesktopVoiceMode::Doubao] {
            app.voice_settings.select_desktop_mode(mode);
            assert_eq!(
                app.voice_caption_text().as_deref(),
                Some("旧模式字幕"),
                "旧模式字幕语义不变"
            );
            assert!(app.voice_caption_is_visible(), "旧模式可见性不变");
            assert!(
                app.render_voice_caption(cx).is_some(),
                "旧模式仍可渲染原浮层"
            );
        }
        app.voice_settings
            .select_desktop_mode(DesktopVoiceMode::GptLive);
        app.gpt_live.fail("fixture connection failed".into());
        assert!(
            app.render_voice_caption(cx).is_none(),
            "错误不能恢复已移除的浮层"
        );
        assert_eq!(
            app.effective_voice_visual(),
            crate::app::voice::control::VoiceVisual::Error,
            "Dock仍显示错误状态"
        );
        assert!(
            app.gpt_live
                .view()
                .status
                .contains("fixture connection failed"),
            "设置页状态仍包含具体错误"
        );
        assert!(!app.gpt_live.is_active(), "真实错误后通话结束");
        app.present_feature_guide_if_ready(cx);
        assert!(app.feature_guide.is_pending(), "失败态仍保留原有引导阻断");
        assert!(!app.feature_guide.is_visible(), "功能引导不能遮挡通话错误");
    });
}
