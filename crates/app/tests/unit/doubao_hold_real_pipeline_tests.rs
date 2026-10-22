use super::*;
use crate::app::doubao::input::SessionInput;
use crate::app::doubao::ContextRequest;
use crate::app::voice_window_inset::{
    TtsPlaybackEvent, TtsPlaybackSource, VoiceWindowInsetEventBus,
};
use audio::{PcmAudioChunk, PcmFormat};
use std::time::Duration;

/// L3：按住录音松开后，生产会话必须完成真实 ASR、AI 回复及原生音频播放。
///
/// 数据构造（含关键数值的推导过程）：NORMA_HOLD_PCM 为固定合成测试语音；
///   16,000 Hz × 2 B × 0.1 s = 3,200 B/帧；录音 id = 1；
///   画布和名册为空，不上传真实项目内容，也不提供项目操作工具。
/// 执行过程：启动生产会话 → 按住并上传 PCM → 关闭输入模拟松键
///   → 等待转写、思考、回复及播放完成 → 显式停止会话。
/// 预期结果：松键前不回复；松键后进入 Thinking 和 Speaking，产生并完成音频播放；
///   会话保持可接收下一段录音，且没有取消或中断播放。
#[tokio::test]
#[ignore = "需要本机豆包凭据、NORMA_HOLD_PCM、联网及音频输出，消耗少量套餐额度"]
async fn doubao_hold_real_pipeline_replies_after_release() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let database = storage::default_app_db_path().expect("应定位本机设置数据库");
    let store = storage::SettingsStore::new(database.clone());
    let account = store
        .get_string("voice.doubao.credential_ref")
        .expect("应读取凭据引用")
        .unwrap_or_else(|| "agent-provider.doubao-desktop".into());
    let pcm = std::fs::read(std::env::var("NORMA_HOLD_PCM").expect("应指定测试 PCM 文件"))
        .expect("测试 PCM 应可读取");
    let (context, requests): (ContextRequests, _) = async_channel::unbounded();
    let context_task = tokio::spawn(async move {
        while let Ok(request) = requests.recv().await {
            let ContextRequest::Model(response) = request;
            let _ = response.send((
                agent::CurrentCanvasContext {
                    project_name: "语音测试".into(),
                    workspace_root: "未打开项目".into(),
                    total_cards: 0,
                    cards: Vec::new(),
                    conversation_tails: Vec::new(),
                },
                Vec::new(),
                None,
            ));
        }
    });
    let cancellation = CancellationToken::new();
    let (events, statuses) = async_channel::unbounded();
    let (_reports, reports) = async_channel::unbounded();
    let (speech, speech_rx) = async_channel::unbounded();
    let (recordings, recording_rx) = async_channel::unbounded();
    let (pcm_tx, pcm_rx) = async_channel::unbounded();
    let (playback, _wake) = VoiceWindowInsetEventBus::new();
    let task = tokio::spawn(run_session(
        (account, ::tts::providers::doubao::default_voice()),
        context,
        None,
        database,
        None,
        cancellation.clone(),
        (
            events,
            reports,
            playback.sink(TtsPlaybackSource::Doubao),
            SessionInput {
                events: speech.clone(),
                speech: speech_rx,
                recordings: Some(recording_rx),
                listens: None,
            },
        ),
    ));
    speech
        .send(asr::SpeechEvent::HeldStarted(1))
        .await
        .expect("应开始录音");
    recordings.send((1, pcm_rx)).await.expect("应提交录音输入");
    for frame in pcm.chunks(3_200) {
        pcm_tx
            .send(Ok(PcmAudioChunk::new(
                PcmFormat::Pcm16,
                16_000,
                1,
                frame.to_vec(),
            )
            .expect("测试音频应为单声道 PCM16")))
            .await
            .expect("应上传音频");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(statuses.try_recv().is_err(), "松键前不能发起 AI 回复");
    assert!(playback.drain().is_empty(), "松键前不能播放回复");
    drop(pcm_tx);
    // 最终 ASR 10 s + Chat 30 s + TTS 30 s + 短回复播放 5 s = 75 s。
    let status_result = tokio::time::timeout(Duration::from_secs(75), async {
        let resolved = statuses.recv().await.expect("应收到转写完成状态");
        assert_eq!(
            resolved,
            DoubaoStatus::RecordingResolved(1),
            "当前录音必须完成转写"
        );
        assert!(
            matches!(statuses.recv().await.expect("应收到转写文本"), DoubaoStatus::UserTranscript(text) if !text.trim().is_empty()),
            "松键后应向左侧对话显示非空真实转写"
        );
        assert_eq!(
            statuses.recv().await.expect("应收到思考状态"),
            DoubaoStatus::Thinking,
            "转写必须进入真实 AI 回复流程"
        );
        assert!(
            matches!(statuses.recv().await.expect("应收到回复文本"), DoubaoStatus::AssistantReply(text) if !text.trim().is_empty()),
            "AI 回复必须先进入左侧对话"
        );
        assert_eq!(
            statuses.recv().await.expect("应收到回复状态"),
            DoubaoStatus::Speaking,
            "AI 必须生成非空回复并进入播放"
        );
        assert_eq!(
            statuses.recv().await.expect("应收到播放完成状态"),
            DoubaoStatus::Connected,
            "播放后应继续等待下一段录音"
        );
    })
    .await;
    let played = playback.drain();
    let was_cancelled = cancellation.is_cancelled();
    cancellation.cancel();
    // 会话退出最多等待 ASR 连接关闭的 2 s + 1 s 调度余量 = 3 s。
    let result = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .expect("停止后应释放会话")
        .expect("会话任务不应 panic");
    context_task.abort();
    assert!(status_result.is_ok(), "松键后完整回复流程不能超时");
    assert!(
        result.is_ok(),
        "生产会话不能因识别、模型或播放错误退出：{result:?}"
    );
    assert!(!was_cancelled, "松键不能取消完整会话");
    assert!(
        played
            .iter()
            .any(|(_, event)| matches!(event, TtsPlaybackEvent::PcmQueued(_))),
        "真实 TTS 必须生成可播放 PCM"
    );
    assert!(
        played
            .iter()
            .any(|(_, event)| matches!(event, TtsPlaybackEvent::Started)),
        "原生播放器必须开始播放"
    );
    assert!(
        played
            .iter()
            .any(|(_, event)| matches!(event, TtsPlaybackEvent::Finished)),
        "原生播放器必须完成回复"
    );
    assert!(
        !played
            .iter()
            .any(|(_, event)| matches!(event, TtsPlaybackEvent::Interrupted)),
        "松键提交的回复不能被无故中断"
    );
}
