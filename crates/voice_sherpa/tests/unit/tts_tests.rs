use super::{
    resolve_matcha_model_dir, resolve_matcha_vocoder_path, ProgressivePcmEncoder, SherpaTtsClient,
};
use crate::FakeTtsClient;
use audio::{PcmAudioChunk, PcmFormat};
use dialogue::{DialogueError, TtsClient};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

struct CollectingTtsSink {
    chunks: Arc<Mutex<Vec<PcmAudioChunk>>>,
    cancelled: AtomicBool,
}

impl dialogue::TtsChunkSink for CollectingTtsSink {
    fn send(&self, chunk: PcmAudioChunk) -> bool {
        self.chunks
            .lock()
            .expect("stream collecting sink lock should not be poisoned")
            .push(chunk);
        true
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// TTS-1: 空文本合成应返回 DialogueError。
///
/// 执行过程：
///   1. FakeTtsClient::synthesize("")
///
/// 预期结果：
///   - 负断言：返回 Err，消息包含 empty
#[test]
fn test_tts_empty_text_returns_error() {
    let mut tts = FakeTtsClient::silent_pcm16_16k();

    assert_eq!(
        tts.synthesize(""),
        Err(DialogueError::Message("empty TTS text".to_string())),
        "空文本不应合成"
    );
    assert_eq!(
        tts.synthesize("   "),
        Err(DialogueError::Message("empty TTS text".to_string())),
        "仅空白文本不应合成"
    );
}

/// TTS-2: 非空文本应返回合法 PCM16 mono 块（16k 或 24k）。
///
/// 预期结果：
///   - 正断言：sample_rate_hz 为 16_000 或 24_000
///   - 正断言：bytes 非空且长度为偶数
#[test]
fn test_fake_tts_returns_valid_pcm_chunk() {
    let mut tts_16k = FakeTtsClient::silent_pcm16_16k();
    let chunk_16k = tts_16k
        .synthesize("你好")
        .expect("非空文本应成功合成 16k 块");
    assert_valid_pcm_chunk(&chunk_16k, 16_000, "16k fake TTS");

    let mut tts_24k = FakeTtsClient::silent_pcm16_24k();
    let chunk_24k = tts_24k
        .synthesize("你好")
        .expect("非空文本应成功合成 24k 块");
    assert_valid_pcm_chunk(&chunk_24k, 24_000, "24k fake TTS");
}

/// TTS-2a：目录已验证的 48kHz 官方 TTS 输出不得被历史三档采样率白名单拒绝。
///
/// 数据构造（含关键数值的推导过程）：
///   sample rate = 48_000 Hz（高质量官方模型的原生单声道输出）
///   samples     = 1（一个 f32 sample 足以验证格式而非音频内容）
///   PCM bytes   = 1 sample × 2 bytes/sample = 2 bytes
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 48_000Hz 创建渐进 PCM 编码器；
///   2. 送入一个 sample，取得一个可播放 PCM16 chunk；
///   3. 读取 chunk 的原生采样率。
///
/// 预期结果：
///   - 断言 A：48kHz 编码器能创建，未来官方模型不会在回调前被拒绝；
///   - 断言 B：chunk 保留 48kHz，播放层可根据原生格式配置输出；
///   - 断言 C：chunk 不得被旧 24kHz 默认值改写。
#[test]
fn test_progressive_pcm_encoder_accepts_48khz_catalog_output() {
    let mut encoder =
        ProgressivePcmEncoder::new(48_000).expect("48kHz 官方单声道 PCM16 应被渐进 TTS 编码器接受");
    let chunk = encoder
        .take_new_chunk(&[0.25])
        .expect("48kHz sample 编码不应报错")
        .expect("非空 48kHz sample 应产生 PCM chunk");

    assert_eq!(
        chunk.sample_rate_hz, 48_000,
        "48kHz 模型输出必须原样交给播放层"
    );
    assert_eq!(chunk.bytes.len(), 2, "一个 PCM16 sample 应编码为 2 bytes");
    assert_ne!(
        chunk.sample_rate_hz, 24_000,
        "48kHz 模型输出不得被旧 24kHz 默认值替代"
    );
}

/// TTS-2b：sherpa 累积 callback 只能转发新增样本，避免播放队列重复旧音频。
///
/// 数据构造：
///   第一次 callback = [0.0, 0.5] 共 2 samples；
///   第二次 callback = [0.0, 0.5, -0.5] 共 3 samples，前 2 个与第一次相同；
///   第三次 callback 与第二次完全相同。
///
/// 执行过程：
///   1. 依次交给 `ProgressivePcmEncoder::take_new_chunk`；
///   2. 读取每次产生的 PCM16 byte 数。
///
/// 预期结果：
///   - 正断言：第一次产生 2 samples = 4 bytes，第二次只产生新增 1 sample = 2 bytes；
///   - 正断言：相同累计快照不再产生 chunk；
///   - 负断言：第二次不得重复发送第一次的 4 bytes。
#[test]
fn test_progressive_pcm_encoder_emits_only_new_callback_samples() {
    let mut encoder = ProgressivePcmEncoder::new(22_050).expect("22.05kHz 应为合法 Matcha 采样率");

    let first = encoder
        .take_new_chunk(&[0.0, 0.5])
        .expect("首次累计 samples 应可编码")
        .expect("首次 callback 应产生 PCM");
    let second = encoder
        .take_new_chunk(&[0.0, 0.5, -0.5])
        .expect("增长后的累计 samples 应可编码")
        .expect("新增 sample 应产生 PCM");
    let repeated = encoder
        .take_new_chunk(&[0.0, 0.5, -0.5])
        .expect("相同累计 samples 不应报错");

    assert_eq!(first.bytes.len(), 4, "2 个 PCM16 samples 应编码为 4 bytes");
    assert_eq!(second.bytes.len(), 2, "第二次只应编码新增 1 个 sample");
    assert_eq!(
        second.sample_rate_hz, 22_050,
        "增量 chunk 应保留 Matcha 的 22.05kHz 采样率"
    );
    assert!(repeated.is_none(), "相同累计 callback 不得重复产生 PCM");
    assert_ne!(
        second.bytes.len(),
        first.bytes.len(),
        "第二次不得把首次 2 个 samples 再次转发"
    );
    assert_eq!(
        encoder.trailing_samples_from_final(&[0.0, 0.5, -0.5, 0.25]),
        &[0.25],
        "最终 aggregate 比 callback 多出的尾部 sample 必须单独补发，不能丢字尾"
    );
}

/// TTS-2c：Matcha 回调短于前一块时代表新的有序音频片段，而不是错误的倒退快照。
///
/// 数据构造：第一个片段含 3 个 samples；第二个片段含 2 个 samples，长度故意更短。
/// 执行过程：按 Matcha 回调顺序交给 ProgressivePcmEncoder。
/// 预期结果：两次均输出完整片段；第二次不得报“moved backwards”，也不得丢掉尾句音频。
#[test]
fn test_progressive_pcm_encoder_accepts_shorter_following_matcha_piece() {
    let mut encoder = ProgressivePcmEncoder::new(22_050).expect("22.05kHz 应为合法 Matcha 采样率");

    let first = encoder
        .take_new_chunk(&[0.0, 0.5, -0.5])
        .expect("首个 Matcha 片段应可编码")
        .expect("首个 Matcha 片段应产生 PCM");
    let second = encoder
        .take_new_chunk(&[0.25, -0.25])
        .expect("更短的后续 Matcha 片段不应被误判为错误")
        .expect("更短的后续 Matcha 片段应产生 PCM");

    assert_eq!(first.bytes.len(), 6, "首个 3-sample 片段应编码为 6 bytes");
    assert_eq!(
        second.bytes.len(),
        4,
        "第二个 2-sample 片段应完整编码为 4 bytes"
    );
    assert!(
        encoder.emitted_callback_audio(),
        "回调送出音频后应记录，避免最终 aggregate 重复播放"
    );
}

/// TTS-3: 官方 Matcha 中文模型的 22050Hz 输出必须能通过生产客户端返回。
///
/// 数据构造（含关键数值的推导过程）：
///   模型 = scripts/download-matcha-tts-model.sh 下载的官方 Matcha 中文模型；
///   vocoder = vocos-22khz-univ.onnx；
///   官方引擎输出 sample_rate = 22_050Hz。
///
/// 执行过程：
///   1. 用生产 SherpaTtsClient 加载真实模型；
///   2. 合成短句「你好」；
///   3. 检查返回的中立 PcmAudioChunk。
///
/// 预期结果：
///   - 正断言：生产客户端返回 22050Hz mono PCM16；
///   - 正断言：音频字节非空；
///   - 负断言：不得以 unsupported TTS sample rate 拒绝官方模型输出。
#[test]
#[ignore = "requires downloaded Matcha Chinese TTS model; run scripts/download-matcha-tts-model.sh first"]
fn test_real_matcha_tts_client_accepts_official_22050hz_output() {
    let model_dir = resolve_matcha_model_dir().expect("Matcha 模型目录应存在");
    let vocoder = resolve_matcha_vocoder_path().expect("Matcha vocoder 应存在");
    let mut tts = SherpaTtsClient::from_model_dir(&model_dir, &vocoder)
        .expect("生产 SherpaTtsClient 应能加载官方模型");

    let chunk = tts
        .synthesize("你好")
        .expect("官方 Matcha 的 22050Hz 输出应成为合法 PCM chunk");

    assert_valid_pcm_chunk(&chunk, 22_050, "official Matcha TTS");
    assert_ne!(
        chunk.sample_rate_hz, 24_000,
        "官方 Matcha 不应被伪装为 24kHz，否则播放速度会错误"
    );
}

/// TTS-4: Matcha 对长回复必须生成显著长于「好的」的完整 PCM，不能只保留首词。
///
/// 数据构造（含关键数值的推导过程）：
///   short =「好的」；long =「好的，我已经听清楚了，现在完整回答你的问题」；
///   两者使用同一生产 Matcha/vocoder，比较 PCM frame 数而不是主观听感。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别合成 short 与 long；
///   2. 以 PCM16 mono 每 frame 2 bytes 推导 frame 数。
///
/// 预期结果：
///   - 正断言：long frames 至少为 short 的 2 倍且采样率一致；
///   - 负断言：长回复不得退化成与「好的」近似长度的音频。
#[test]
#[ignore = "requires downloaded Matcha Chinese TTS model; run scripts/download-matcha-tts-model.sh first"]
fn test_real_matcha_tts_long_reply_is_not_truncated_to_okay() {
    let model_dir = resolve_matcha_model_dir().expect("Matcha 模型目录应存在");
    let vocoder = resolve_matcha_vocoder_path().expect("Matcha vocoder 应存在");
    let mut tts = SherpaTtsClient::from_model_dir(&model_dir, &vocoder)
        .expect("生产 SherpaTtsClient 应能加载官方模型");

    let short = tts.synthesize("好的").expect("短确认词应可合成");
    let long = tts
        .synthesize("好的，我已经听清楚了，现在完整回答你的问题。")
        .expect("完整回复应可合成");
    let short_frames = short.bytes.len() / 2;
    let long_frames = long.bytes.len() / 2;

    assert_eq!(
        long.sample_rate_hz, short.sample_rate_hz,
        "长短文本应使用相同 Matcha 采样率"
    );
    assert!(
        long_frames >= short_frames.saturating_mul(2),
        "完整回复 PCM 应至少为『好的』两倍长，short={short_frames}, long={long_frames}"
    );
    assert_ne!(
        long_frames, short_frames,
        "Matcha 不得把完整回复截断成首词音频"
    );
}

/// TTS-5: 默认生产 TTS 必须使用 Matcha 中文模型合成中文文本。
///
/// 数据构造（含关键数值的推导过程）：
///   text =「你好，多灵魂桌面正在测试本地中文语音合成。」；
///   Matcha 官方模型输出 sample_rate = 22_050Hz；
///   新 Kokoro 模型输出 sample_rate = 24_000Hz。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 通过应用生产入口 `SherpaTtsClient::from_env_or_default` 加载默认 TTS；
///   2. 将中文句子一次性交给 `TtsClient::synthesize`；
///   3. 检查返回的中立 PCM chunk。
///
/// 预期结果：
///   - 正断言：返回 22_050Hz mono PCM16 且音频非空；
///   - 负断言：不得返回 Kokoro 的 24_000Hz，证明默认后端已切回 Matcha。
#[test]
#[ignore = "requires downloaded Matcha Chinese TTS model; run scripts/download-matcha-tts-model.sh first"]
fn test_default_real_tts_synthesizes_chinese_with_matcha() {
    let mut tts =
        SherpaTtsClient::from_env_or_default().expect("默认生产 TTS 应加载 Matcha 中文模型");

    let chunk = tts
        .synthesize("你好，多灵魂桌面正在测试本地中文语音合成。")
        .expect("Matcha 应能合成完整中文句");

    assert_valid_pcm_chunk(&chunk, 22_050, "default Matcha Chinese TTS");
    assert_ne!(
        chunk.sample_rate_hz, 24_000,
        "默认 TTS 不得继续使用 24kHz 的 Kokoro 模型"
    );
}

/// TTS-6：生产 Matcha 的流式接口必须交出可播放的 PCM，不能只在内部完成合成。
///
/// 数据构造：默认 Matcha 模型、线程安全的 collecting sink，以及含两个短句的中文文本。
/// 执行过程：调用 `synthesize_stream`，收集它主动送出的每一个 PCM 块。
/// 预期结果：至少一个块到达 sink；每块均为 22.05kHz mono PCM16；sink 未被取消。
#[test]
#[ignore = "requires downloaded Matcha Chinese TTS model; run scripts/download-matcha-tts-model.sh first"]
fn test_real_matcha_streams_pcm_to_incremental_sink() {
    let mut tts =
        SherpaTtsClient::from_env_or_default().expect("默认生产 TTS 应加载 Matcha 中文模型");
    let chunks = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(CollectingTtsSink {
        chunks: Arc::clone(&chunks),
        cancelled: AtomicBool::new(false),
    });

    let text = "第一句已经准备好了。第二句继续说明。";
    tts.synthesize_stream(text, sink)
        .expect("Matcha 流式合成应把 PCM 送入 sink");

    let received = chunks
        .lock()
        .expect("stream collecting sink lock should not be poisoned");
    assert!(!received.is_empty(), "流式 Matcha 至少应送出一个 PCM 块");
    assert!(
        received.iter().all(|chunk| {
            chunk.format == PcmFormat::Pcm16
                && chunk.sample_rate_hz == 22_050
                && chunk.channels == 1
                && !chunk.bytes.is_empty()
        }),
        "每个增量 PCM 块都必须保持 Matcha 的 22.05kHz mono PCM16 格式"
    );
    let streamed_frame_count = received
        .iter()
        .map(|chunk| chunk.bytes.len() / 2)
        .sum::<usize>();
    drop(received);
    let complete = tts
        .synthesize(text)
        .expect("同一 Matcha 文本应能生成完整基准 PCM");

    let complete_frame_count = complete.bytes.len() / 2;
    let frame_delta = streamed_frame_count.abs_diff(complete_frame_count);
    assert!(
        frame_delta <= 256,
        "流式 callback 的 PCM 总帧数必须与完整合成处于 256 frames（约 12ms）以内，不能重复或截断句子；streamed={streamed_frame_count}, complete={complete_frame_count}, delta={frame_delta}"
    );
}

fn assert_valid_pcm_chunk(chunk: &PcmAudioChunk, expected_rate: u32, label: &str) {
    assert_eq!(chunk.format, PcmFormat::Pcm16, "{label} 应为 PCM16");
    assert_eq!(chunk.sample_rate_hz, expected_rate, "{label} 采样率应匹配");
    assert_eq!(chunk.channels, 1, "{label} 应为 mono");
    assert!(!chunk.bytes.is_empty(), "{label} bytes 不应为空");
    assert_eq!(chunk.bytes.len() % 2, 0, "{label} PCM16 字节数应为偶数");
}
