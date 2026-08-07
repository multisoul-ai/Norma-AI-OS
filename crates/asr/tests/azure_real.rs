use asr::{
    AsrProviderCapabilities, AzureSpeechClient, AzureSpeechConfig, AzureSpeechRecognizer,
    AzureSpeechSecret, InputEndReason, RecognitionEvent, RecognitionPipeline,
};
use audio::{PcmAudioChunk, PcmFormat};
use std::path::{Path, PathBuf};

/// L3-AZURE-REAL-1：用户提供的真实 M4A 必须通过公开 RecognitionPipeline 识别。
///
/// 数据构造（含关键数值的推导过程）：
///   normalized audio = 16,000 samples/s × 2 bytes × mono
///   pipeline chunk    = 3,200 bytes = 100 ms PCM
///   request timeout   = 30 s（Azure short-audio client 固定上限）
///   stable fragments  = 4（Java / 创建一个项目 / 打开Java项目 / 我正在学习Java）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. ffmpeg 将真实 M4A 标准化为 16 kHz mono PCM16
///   2. 通过公开 RecognitionPipeline 分 100 ms chunk 推送全部 PCM
///   3. 以 User 原因结束输入，Azure Cloud Provider 提交 WAV
///   4. 从统一事件流读取唯一 Final 并匹配稳定片段
///
/// 预期结果：
///   - 正断言：四个稳定片段均出现在真实 Final 中
///   - 负断言：不得返回 Failed/Cancelled，也不得把通道关闭当作成功
#[test]
#[ignore = "requires Azure credentials, ffmpeg, network access, and the real user M4A"]
fn real_m4a_is_recognized_through_public_pipeline() {
    let secret = azure_test_secret();
    let region = std::env::var("AZURE_SPEECH_REGION").unwrap_or_else(|_| "eastus".to_string());
    let locale = std::env::var("AZURE_SPEECH_LOCALE").unwrap_or_else(|_| "zh-CN".to_string());
    let audio_path = std::env::var_os("AZURE_SPEECH_TEST_AUDIO")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/Users/alan/Documents/plugins/java-asr-repro.m4a"));
    assert!(
        audio_path.is_file(),
        "真实 Azure 验收音频必须存在；请设置 AZURE_SPEECH_TEST_AUDIO"
    );
    let temp = tempfile::tempdir().expect("真实 Azure 验收应创建隔离临时目录");
    let pcm_path = temp.path().join("azure-real-test.pcm");
    normalize_m4a_to_pcm16(&audio_path, &pcm_path);
    let pcm = std::fs::read(&pcm_path).expect("应读取 ffmpeg 标准化后的 PCM");
    assert!(!pcm.is_empty(), "真实 M4A 标准化后不得产生空 PCM");

    let config = AzureSpeechConfig::new(region, locale, AzureSpeechSecret::new(secret))
        .expect("真实 Azure region、locale 与 Key 应构成有效配置");
    let recognizer = AzureSpeechRecognizer::new(AzureSpeechClient::new(config));
    let session =
        RecognitionPipeline::new(AsrProviderCapabilities::new(false, Some(60))).start(recognizer);
    for bytes in pcm.chunks(3_200) {
        let chunk = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, bytes.to_vec())
            .expect("ffmpeg 输出的每个 chunk 都应是标准 PCM16");
        session
            .push_pcm(chunk)
            .expect("真实 PCM 应通过公开 RecognitionPipeline");
    }
    session
        .finish(InputEndReason::User)
        .expect("不足 60 秒的真实音频应由 User 原因结束");
    let events = session.collect_events();

    assert!(
        !events.iter().any(|event| matches!(
            event,
            RecognitionEvent::Failed(_) | RecognitionEvent::Cancelled
        )),
        "真实 Azure 事件流不得以 Failed 或 Cancelled 结束"
    );
    let transcript = events
        .iter()
        .find_map(|event| match event {
            RecognitionEvent::Final(text) => Some(text.as_str()),
            _ => None,
        })
        .expect("真实 Azure 事件流必须包含 Final，不能仅关闭通道");
    let normalized = normalize_transcript(transcript);
    for fragment in ["java", "创建一个项目", "打开java项目", "我正在学习java"] {
        assert!(
            normalized.contains(fragment),
            "真实 Azure Final 缺少稳定片段 `{fragment}`"
        );
    }
}

fn azure_test_secret() -> String {
    if let Ok(secret) = std::env::var("AZURE_SPEECH_KEY") {
        return secret;
    }
    let path = std::env::var_os("AZURE_SPEECH_KEY_FILE")
        .map(PathBuf::from)
        .expect("请设置 AZURE_SPEECH_KEY 或 AZURE_SPEECH_KEY_FILE");
    std::fs::read_to_string(path)
        .expect("应读取未纳入版本控制的 Azure secret 文件")
        .trim()
        .to_string()
}

fn normalize_m4a_to_pcm16(input: &Path, output: &Path) {
    let status = std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(input)
        .args(["-ar", "16000", "-ac", "1", "-f", "s16le"])
        .arg(output)
        .status()
        .expect("真实 Azure 验收需要可执行的 ffmpeg");
    assert!(status.success(), "ffmpeg 必须成功标准化真实 M4A");
}

fn normalize_transcript(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|character| !character.is_whitespace() && !character.is_ascii_punctuation())
        .collect()
}
