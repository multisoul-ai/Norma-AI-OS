use super::SherpaTtsClient;
use audio::{PcmAudioChunk, PcmFormat};
use dialogue::TtsClient;
use std::path::{Path, PathBuf};

/// TTS-9：官方 ZipVoice INT8 发布包必须经生产安装入口完成中英文克隆合成。
///
/// 数据构造（含关键数值的推导过程）：
///   archive bytes    = 109,162,785
///   vocoder bytes    = 54,157,409
///   installed inputs = 109,162,785 + 54,157,409 = 163,320,194 bytes
///   reference WAV    = 官方 `leijun-1.wav` 重采样为 16 kHz mono PCM、时长不变
///   expected output  = 24,000 Hz × mono × PCM16，即每 frame 2 bytes
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 校验并安装官方 ZipVoice INT8 archive 与 24 kHz Vocos；
///   2. 将官方 24 kHz 示例重采样为产品接受的 16 kHz，再导入匹配转写；
///   3. 通过生产 `SherpaTtsClient` 加载模型，依次合成中文和英文；
///   4. 输出单次完整 PCM 耗时与 RTF，供 M4 实验性 gate 留证。
///
/// 预期结果：
///   - 正断言：中英文都返回非空 24 kHz mono PCM16；
///   - 负断言：两种语言不得生成完全相同 PCM，也不得退化为 16 kHz 参考输入格式。
#[test]
#[ignore = "requires official ZipVoice archive and Vocos under target/tts-candidates"]
fn test_real_zipvoice_int8_synthesizes_cloned_chinese_and_english() {
    let archive = candidate_path("sherpa-onnx-zipvoice-distill-int8-zh-en-emilia.tar.bz2");
    let vocoder = candidate_path("vocos_24khz.onnx");
    assert!(archive.is_file(), "ZipVoice INT8 测试归档必须已下载");
    assert!(vocoder.is_file(), "ZipVoice 的 24 kHz Vocos 必须已下载");
    let catalog = tts::builtin_model_catalog();
    let descriptor = catalog
        .find_by_str(tts::ZIPVOICE_DISTILL_INT8_MODEL_ID)
        .expect("内置目录必须包含 ZipVoice-Distill INT8");
    let model_root = tempfile::tempdir().expect("应创建隔离 ZipVoice 安装目录");
    let available = tts::install_downloaded_package(
        descriptor,
        &archive,
        std::slice::from_ref(&vocoder),
        model_root.path(),
    )
    .expect("官方 ZipVoice archive 与 Vocos 应通过固定哈希和安装清单校验");
    let reference_16k = model_root.path().join("leijun-1-16khz.wav");
    resample_test_wav_24k_to_16k(
        &available.root().join("test_wavs/leijun-1.wav"),
        &reference_16k,
    );
    let reference = tts::import_zipvoice_reference(
        &reference_16k,
        "那还是36年前, 1987年. 我呢考上了武汉大学的计算机系.",
        model_root.path(),
    )
    .expect("官方雷军参考 WAV 与匹配转写应通过 v1 导入契约");
    let voice_id = descriptor.resolve_voice_id(None).clone();
    let mut client = SherpaTtsClient::from_available_model_with_reference(
        &available,
        &voice_id,
        Some(&reference),
    )
    .expect("生产入口应能加载 ZipVoice INT8 与参考声音");

    let chinese_started = std::time::Instant::now();
    let chinese = client
        .synthesize("你好，我是诺玛。")
        .expect("ZipVoice 应能按参考声音合成中文");
    report_performance("Chinese", chinese_started.elapsed(), &chinese);
    let english_started = std::time::Instant::now();
    let english = client
        .synthesize("Hello, I am Norma.")
        .expect("ZipVoice 应能按同一参考声音合成英文");
    report_performance("English", english_started.elapsed(), &english);

    assert_valid_pcm_chunk(&chinese, "ZipVoice Chinese TTS");
    assert_valid_pcm_chunk(&english, "ZipVoice English TTS");
    assert_ne!(
        chinese.sample_rate_hz, 16_000,
        "ZipVoice 输出不得沿用参考音频的 16 kHz 格式"
    );
    assert_ne!(
        chinese.bytes, english.bytes,
        "不同语言的不同文本不得生成完全相同的克隆 PCM"
    );
}

fn report_performance(language: &str, elapsed: std::time::Duration, chunk: &PcmAudioChunk) {
    let duration = chunk.bytes.len() as f32 / 2.0 / 24_000.0;
    let rtf = elapsed.as_secs_f32() / duration;
    eprintln!(
        "ZipVoice {language} complete PCM: elapsed={elapsed:?}, duration={duration:.3}s, rtf={rtf:.3}"
    );
}

fn assert_valid_pcm_chunk(chunk: &PcmAudioChunk, label: &str) {
    assert_eq!(chunk.format, PcmFormat::Pcm16, "{label} 应为 PCM16");
    assert_eq!(chunk.sample_rate_hz, 24_000, "{label} 应为 24 kHz");
    assert_eq!(chunk.channels, 1, "{label} 应为 mono");
    assert!(!chunk.bytes.is_empty(), "{label} bytes 不应为空");
}

fn resample_test_wav_24k_to_16k(source: &Path, destination: &Path) {
    let mut reader = hound::WavReader::open(source).expect("官方参考 WAV 应可解码");
    let source_spec = reader.spec();
    assert_eq!(
        source_spec.sample_rate, 24_000,
        "测试 resampler 只处理官方 24 kHz ZipVoice 参考 WAV"
    );
    assert_eq!(
        source_spec.channels, 1,
        "官方 ZipVoice 参考 WAV 应保持单声道"
    );
    let samples = reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .expect("官方 ZipVoice 参考 WAV 样本应完整可读");
    // output/input = 16,000 / 24,000 = 2/3；每个输出样本在输入上的步长为 1.5。
    let output_len = samples.len() * 2 / 3;
    let output_spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::create(destination, output_spec).expect("测试应创建 16 kHz 参考 WAV");
    for output_index in 0..output_len {
        let source_position = output_index as f64 * 1.5;
        let left_index = source_position.floor() as usize;
        let right_index = (left_index + 1).min(samples.len().saturating_sub(1));
        let fraction = source_position.fract() as f32;
        let value = f32::from(samples[left_index]) * (1.0 - fraction)
            + f32::from(samples[right_index]) * fraction;
        writer
            .write_sample(value.round() as i16)
            .expect("重采样参考样本应写入测试 WAV");
    }
    writer.finalize().expect("16 kHz 测试 WAV 应成功收口");
}

fn candidate_path(filename: &str) -> PathBuf {
    super::repo_root()
        .join("target")
        .join("tts-candidates")
        .join(filename)
}
