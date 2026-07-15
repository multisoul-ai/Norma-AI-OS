use super::{resolve_matcha_model_dir, resolve_matcha_vocoder_path, SherpaTtsClient};
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

/// TTS-6：生产 Matcha 的流式适配必须只交付一次完整 PCM，绕开不稳定的原生 progress callback。
///
/// 数据构造（含关键数值的推导过程）：
///   model         = 官方 Matcha 中文模型，原生采样率为 22_050Hz
///   text          = 19 个中文字符（含标点）的双短句，足以触发旧 callback 的多块输出
///   expected PCM  = 1 个完整 utterance chunk（默认 TtsClient 适配的固定行为）
///   sink          = 1 个线程安全 collecting sink，初始取消状态为 false
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用生产 SherpaTtsClient 加载 Matcha；
///   2. 调用 synthesize_stream，将完整 utterance 交给 collecting sink；
///   3. 读取 sink 的块数量与唯一块的 PCM 格式。
///
/// 预期结果：
///   - 断言 A：sink 必须恰好收到 1 块，不能把 progress callback 的重叠风险带入播放器；
///   - 断言 B：该块必须是 22.05kHz mono PCM16，保留 Matcha 原始音频格式；
///   - 断言 C：该块必须非空，不能因禁用 progress callback 而丢失整句语音。
#[test]
#[ignore = "requires downloaded Matcha Chinese TTS model; run scripts/download-matcha-tts-model.sh first"]
fn test_real_matcha_stream_adapter_forwards_one_complete_pcm_chunk() {
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
    assert_eq!(
        received.len(),
        1,
        "Matcha 流式适配必须恰好发送一个完整 PCM 块，不能继续转发多个 progress callback 块"
    );
    let chunk = &received[0];
    assert_eq!(
        chunk.format,
        PcmFormat::Pcm16,
        "完整 Matcha PCM 块必须保持 PCM16 格式"
    );
    assert_eq!(
        chunk.sample_rate_hz, 22_050,
        "完整 Matcha PCM 块必须保持原始 22.05kHz 采样率"
    );
    assert_eq!(
        chunk.channels, 1,
        "完整 Matcha PCM 块必须保持单声道，不能改变播放格式"
    );
    assert!(
        !chunk.bytes.is_empty(),
        "禁用 progress callback 后仍必须交付非空的整句语音"
    );
}

fn assert_valid_pcm_chunk(chunk: &PcmAudioChunk, expected_rate: u32, label: &str) {
    assert_eq!(chunk.format, PcmFormat::Pcm16, "{label} 应为 PCM16");
    assert_eq!(chunk.sample_rate_hz, expected_rate, "{label} 采样率应匹配");
    assert_eq!(chunk.channels, 1, "{label} 应为 mono");
    assert!(!chunk.bytes.is_empty(), "{label} bytes 不应为空");
    assert_eq!(chunk.bytes.len() % 2, 0, "{label} PCM16 字节数应为偶数");
}
