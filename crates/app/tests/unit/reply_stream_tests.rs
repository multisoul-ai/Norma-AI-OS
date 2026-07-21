use super::reply_stream::capture::write_pcm_chunks_to_wav;
use super::reply_stream::{pcm_trace_fingerprint, QueuedPlaybackChunk, StreamPlaybackBuffer};
use super::{DialogueControlState, SharedTtsRuntime, TtsPreviewControl};
use audio::{PcmAudioChunk, PcmFormat};
use dialogue::DialoguePhase;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// MEM-TTS-1：Dialogue 与 Agent announcement 顺序使用 TTS 后只能保留一个模型实例。
///
/// 数据构造（含关键数值的推导过程）：
///   initial clients     = 1 个 FakeTtsClient（代表已加载的 Kokoro/ONNX runtime）
///   sequential users    = 2 个（Dialogue、Agent announcement）
///   expected residents  = 1 个，而不是 1 × 2 = 2 个
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Dialogue 通过共享 runtime 合成一次；factory 创建第 1 个 FakeTtsClient。
///   2. Agent announcement 再通过同一 runtime 合成；应复用 resident client。
///   3. 检查两次业务调用的 PCM 和最终 resident/factory 计数。
///
/// 预期结果：
///   - 正断言：两条业务路径都能得到非空 PCM。
///   - 正断言：两次顺序使用后共享 runtime 仍恰好保留 1 个 client。
///   - 负断言：factory 不得执行第 2 次，否则同配置被重复加载。
#[test]
fn test_dialogue_and_agent_announcement_share_one_resident_tts_client() {
    let runtime = SharedTtsRuntime::default();
    let creations = Arc::new(AtomicUsize::new(0));
    let synthesize = |text: &str| {
        let creations_for_factory = Arc::clone(&creations);
        runtime
            .with_client(
                None,
                move || {
                    creations_for_factory.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, String>(super::fake_tts_client_for_tests())
                },
                |tts| super::reply::synthesize_reply(tts, text).map_err(|error| error.to_string()),
            )
            .expect("Fake TTS 合成不应失败")
    };

    let dialogue_pcm = synthesize("Dialogue reply");
    let announcement_pcm = synthesize("Agent done");

    assert!(!dialogue_pcm.bytes.is_empty(), "Dialogue 必须得到非空 PCM");
    assert!(
        !announcement_pcm.bytes.is_empty(),
        "Agent announcement 必须复用 runtime 并得到非空 PCM"
    );

    assert_eq!(
        runtime.resident_client_count_for_tests(),
        1,
        "两条业务路径顺序完成后必须只保留一个 resident TTS client"
    );
    assert_eq!(
        creations.load(Ordering::SeqCst),
        1,
        "同一配置下 Dialogue 与 Agent announcement 不得创建第二个 TTS client"
    );
}

fn pcm16_mono_chunk(sample_rate_hz: u32, duration: Duration) -> PcmAudioChunk {
    let frame_count = (duration.as_secs_f64() * f64::from(sample_rate_hz)) as usize;
    PcmAudioChunk::new(
        PcmFormat::Pcm16,
        sample_rate_hz,
        1,
        vec![0; frame_count * 2],
    )
    .expect("测试 PCM 应满足 PCM16 mono 格式约束")
}

/// STR-SILENT-1：TaskQueued 的静默临时回复必须完整结束生命周期且不启动音频工作。
///
/// 数据构造（含关键数值的推导过程）：
///   initial phase          = Thinking：Listening 经 1 次 utterance final 后得到
///   display replies        = 1：“任务处理中：检查当前项目”
///   expected speech items  = 0
///   expected PCM chunks    = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 把 DialogueSession 从 Idle 切到 Listening，再以 utterance final 进入 Thinking
///   2. 调用 finish_silent_reply 保存临时文字并结束当前 reply stream
///   3. 读取 phase、显示文本、TTS task、speech queue 与 PCM pending 数
///
/// 预期结果：
///   - 正断言：phase 回到 Listening，处理中临时文字仍可显示
///   - 负断言：不得创建 speech task、speech segment 或 PCM 播放块
#[test]
fn test_silent_queued_reply_returns_to_listening_without_tts_or_pcm() {
    let mut dialogue = DialogueControlState::default();
    dialogue.session.on_listening_started();
    dialogue
        .session
        .on_utterance_final()
        .expect("Listening 收到 final 后必须进入 Thinking");

    dialogue.finish_silent_reply("任务处理中：检查当前项目".to_string());

    assert_eq!(
        dialogue.phase(),
        DialoguePhase::Listening,
        "静默完成 TaskQueued 临时回复后必须回到 Listening，不能卡在 Thinking"
    );
    assert_eq!(
        dialogue.last_reply_text.as_deref(),
        Some("任务处理中：检查当前项目"),
        "静默只抑制播报，不得丢失界面需要显示的处理中状态"
    );
    assert!(
        dialogue.speech_task.is_none(),
        "静默 TaskQueued 不得创建 TTS synthesis task"
    );
    assert!(
        dialogue.speech_backlog.is_empty(),
        "静默 TaskQueued 不得排入任何 speech segment"
    );
    assert_eq!(
        dialogue.playback_chunks_pending, 0,
        "静默 TaskQueued 不得产生待播放 PCM chunk"
    );
    assert!(
        dialogue.reply_stream_started_at.is_none(),
        "静默回复完成后必须清理 reply stream 计时状态"
    );
}

/// STR-TRACE-1：诊断日志必须用稳定的 PCM 指纹区分“同一块被重复播放”和“不同音频块”。
///
/// 数据构造（含关键数值的推导过程）：
///   first  = 4 bytes = 2 个 16-bit PCM 样本（字节为 [1, 2, 3, 4]）
///   clone  = 4 bytes = first 的逐字节副本
///   second = 4 bytes = 仅最后 1 byte 与 first 不同（字节为 [1, 2, 3, 5]）
///   Total  = 3 个相同格式、各 2 samples 的最小 PCM 块
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别计算 first、clone 与 second 的日志指纹；
///   2. 比较 first 与 clone，验证相同音频在同一次复现中可关联；
///   3. 比较 first 与 second，验证单字节变化不会被误报成同一块音频。
///
/// 预期结果：
///   - 断言 A：first 与 clone 的指纹相同，因为两者 PCM 字节完全一致；
///   - 断言 B：first 与 second 的指纹不同，因为它们不是同一段 PCM。
#[test]
fn test_pcm_trace_fingerprint_is_stable_and_distinguishes_changed_audio() {
    let first = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![1, 2, 3, 4])
        .expect("first 测试 PCM 应满足 PCM16 mono 格式约束");
    let clone = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![1, 2, 3, 4])
        .expect("clone 测试 PCM 应满足 PCM16 mono 格式约束");
    let second = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![1, 2, 3, 5])
        .expect("second 测试 PCM 应满足 PCM16 mono 格式约束");

    assert_eq!(
        pcm_trace_fingerprint(&first),
        pcm_trace_fingerprint(&clone),
        "完全相同的 PCM 必须有相同指纹，否则无法在日志中识别重复播放"
    );
    assert_ne!(
        pcm_trace_fingerprint(&first),
        pcm_trace_fingerprint(&second),
        "字节不同的 PCM 不得复用指纹，否则日志会把不同音频误判为重复"
    );
}

/// STR-CAPTURE-1：开启诊断时，播放前的 PCM 必须按原格式写入可复听的 WAV 文件。
///
/// 数据构造（含关键数值的推导过程）：
///   sample rate = 16_000 Hz（诊断用合法单声道 PCM 采样率）
///   PCM bytes   = 4 bytes = 2 个 i16 样本（[1, 0]、[2, 0]）
///   chunks      = 1（最小可验证的一段回复音频）
///   WAV samples = 4 bytes / 2 bytes 每样本 = 2 samples
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造一个 16kHz、单声道、含两个 PCM16 样本的块；
///   2. 调用诊断 WAV 写入函数，把块写到隔离临时文件；
///   3. 用 WAV reader 重新打开文件并读取格式与样本数。
///
/// 预期结果：
///   - 断言 A：文件必须存在，供人工复听合成输出；
///   - 断言 B：WAV 必须保留 16kHz mono，不能被诊断层改采样；
///   - 断言 C：WAV 必须恰有 2 个样本，不能重复或丢弃 PCM 数据。
#[test]
fn test_pcm_capture_writes_playable_wav_without_duplicate_samples() {
    let temp = tempfile::tempdir().expect("测试应能创建隔离的临时目录");
    let path = temp.path().join("reply.wav");
    let chunk = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![1, 0, 2, 0])
        .expect("测试 PCM 应满足 PCM16 mono 格式约束");

    write_pcm_chunks_to_wav(&path, &[chunk]).expect("诊断捕获应能写入临时 WAV 文件");

    let reader = hound::WavReader::open(&path).expect("写出的 WAV 应能被标准 reader 打开");
    let spec = reader.spec();
    let sample_count = reader
        .into_samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .expect("测试 WAV 中的 PCM16 样本应完整可读")
        .len();

    assert!(
        path.is_file(),
        "诊断 WAV 文件必须存在，才能用于判断重复是否已在合成阶段出现"
    );
    assert_eq!(
        spec.sample_rate, 16_000,
        "诊断 WAV 必须保留原始 16kHz 采样率"
    );
    assert_eq!(
        spec.channels, 1,
        "诊断 WAV 必须保留单声道，不能改变原始声道数"
    );
    assert_eq!(
        sample_count, 2,
        "4 bytes PCM16 必须恰好写出 2 个样本，不能重复或丢失"
    );
}

/// STR-1：流式 TTS 必须在累计至少 120ms 后才启动第一块播放。
///
/// 数据构造：22.05kHz mono 的 60ms、60ms 两个 PCM 块；二者刚好累计为 120ms。
/// 执行过程：按顺序 push 两个块，并读取每次允许调度的输出。
/// 预期结果：第一块独自到达时不允许播放；第二块到达后两个块必须按原顺序一起释放。
#[test]
fn test_stream_playback_buffer_waits_for_120ms_then_releases_in_order() {
    let mut buffer = StreamPlaybackBuffer::new(Duration::from_millis(120));
    let first = pcm16_mono_chunk(22_050, Duration::from_millis(60));
    let second = pcm16_mono_chunk(22_050, Duration::from_millis(60));

    let before_threshold = buffer.push(QueuedPlaybackChunk::from_pcm(first.clone()));
    let at_threshold = buffer.push(QueuedPlaybackChunk::from_pcm(second.clone()));

    assert!(before_threshold.is_empty(), "累计 60ms 时不得提前启动播放");
    assert_eq!(
        at_threshold
            .into_iter()
            .map(|chunk| chunk.pcm)
            .collect::<Vec<_>>(),
        vec![first, second],
        "累计恰好 120ms 后必须按输入顺序释放所有预缓冲块"
    );
    assert!(buffer.is_empty(), "释放后预缓冲队列必须清空，避免重复播放");
}

/// STR-2：模型结束时不足 120ms 的尾部音频必须立即播放，不能靠插入静音凑门槛。
///
/// 数据构造：16kHz mono 的单个 80ms PCM 块，小于 120ms 启播门槛。
/// 执行过程：push 后模拟该 segment 的 TTS worker Finished，并调用 finish。
/// 预期结果：push 阶段仍不播放；finish 必须原样释放 80ms；不得留下待播块。
#[test]
fn test_stream_playback_buffer_flushes_short_final_audio_without_fake_silence() {
    let mut buffer = StreamPlaybackBuffer::new(Duration::from_millis(120));
    let short_final = pcm16_mono_chunk(16_000, Duration::from_millis(80));

    let before_finish = buffer.push(QueuedPlaybackChunk::from_pcm(short_final.clone()));
    let after_finish = buffer.finish();

    assert!(
        before_finish.is_empty(),
        "未结束时 80ms 不得突破 120ms 预缓冲门槛"
    );
    assert_eq!(
        after_finish
            .into_iter()
            .map(|chunk| chunk.pcm)
            .collect::<Vec<_>>(),
        vec![short_final],
        "最终短音频必须原样播放，不能补静音或被丢弃"
    );
    assert!(
        buffer.is_empty(),
        "最终 flush 后不得残留 PCM，避免下一段串音"
    );
}

/// STR-TTS-1：模型或 Voice 配置变更必须失效旧代际并取消正在进行的试听。
///
/// 数据构造（含关键数值的推导过程）：
///   initial configuration generation = 0（新建 DialogueControlState）
///   preview controls                 = 1（唯一的独立试听 sink 控制器）
///   queued speech segments            = 1（模拟旧 Voice 正在等待合成）
///   configuration changes            = 1（一次模型/Voice 变更）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 把一个未取消的试听控制器注册到 DialogueControlState；
///   2. 排入一段旧语音并标记合成活跃，再记录变更前的两个 generation；
///   3. 调用统一的配置变更取消路径并读取代际、队列与试听控制器状态。
///
/// 预期结果：
///   - 断言 A：generation 必须恰好增加 1，使旧异步结果失效；
///   - 断言 B：已登记的试听必须被取消，不能与后续回复混音；
///   - 断言 C：新 generation 不得等于旧 generation。
///   - 断言 D：旧合成队列与活跃标记必须清空，不能在新 Voice 下继续出音。
#[test]
fn test_tts_configuration_change_invalidates_generation_and_preview() {
    let mut dialogue = DialogueControlState::default();
    let control = Arc::new(TtsPreviewControl::new());
    dialogue.track_tts_preview(Arc::clone(&control));
    dialogue
        .speech_backlog
        .accept_batch(vec!["old voice segment".to_string()])
        .expect("旧 Voice 测试 segment 必须进入 backlog");
    dialogue.speech_synthesis_active = true;
    let configuration_before = dialogue.tts_configuration_generation();
    let reply_before = dialogue.reply_generation;

    dialogue.cancel_tts_for_configuration_change();

    assert_eq!(
        dialogue.tts_configuration_generation(),
        configuration_before.wrapping_add(1),
        "一次模型或 Voice 配置变更必须只推进一次 TTS configuration generation"
    );
    assert!(
        control.is_cancelled(),
        "配置变更必须取消已登记的 Voice preview，避免与后续助手回复混音"
    );
    assert_ne!(
        dialogue.tts_configuration_generation(),
        configuration_before,
        "配置变更后的 generation 不得仍等于旧值，否则旧异步事件可能写回"
    );
    assert_eq!(
        dialogue.reply_generation,
        reply_before.wrapping_add(1),
        "配置变更必须推进 reply generation，使旧 synthesis/playback 事件被丢弃"
    );
    assert!(
        dialogue.speech_backlog.is_empty(),
        "配置变更必须清空旧 Voice 的待合成片段，不能在新 Voice 下继续播报"
    );
    assert!(
        !dialogue.speech_synthesis_active,
        "配置变更必须清除旧 synthesis 活跃标记，避免旧 worker 继续占用播放路径"
    );
}
