use sherpa_onnx::{
    GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsMatchaModelConfig,
    OfflineTtsModelConfig,
};
use std::path::{Path, PathBuf};
use std::time::Instant;

struct DemoText {
    id: &'static str,
    text: &'static str,
}

/// TTS-DEMO-1: 使用 sherpa-onnx OfflineTts 对三段中文文本做本地合成并打印耗时。
///
/// 数据构造（含关键数值的推导过程）：
///   text_count = 3（短句、中句、长句各一条）
///   speaker_id = env MULTISOUL_TTS_SPEAKER_ID，默认 0
///   speed      = env MULTISOUL_TTS_SPEED，默认 1.0
///   threads    = 2（与 sherpa 官方 Rust Matcha 中文示例保持一致）
///   输出目录   = env MULTISOUL_TTS_OUTPUT_DIR，默认 target/tts-demo/matcha
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 Matcha 中文模型目录和 vocoder 路径
///   2. 构造 OfflineTtsConfig 和 GenerationConfig
///   3. 逐条合成中文文本，保存为 zh-short.wav / zh-medium.wav / zh-long.wav
///   4. 打印 elapsed、audio duration、RTF、sample_rate、samples 和 wav 路径
///
/// 预期结果：
///   - 正断言：每条文本都生成非空 samples
///   - 正断言：每个 wav 文件都写入成功且存在
///   - 负断言：耗时和音频时长不应为 0，否则说明没有真正完成合成
///
/// 手动运行：
///   cargo test -p voice_sherpa --test tts_demo -- --ignored --nocapture
#[test]
#[ignore = "requires downloaded Matcha Chinese TTS model; run scripts/download-matcha-tts-model.sh first"]
fn test_sherpa_offline_tts_chinese_demo_prints_latency() {
    let model_dir = resolve_matcha_model_dir();
    let vocoder = resolve_matcha_vocoder_path();
    let output_dir = resolve_output_dir();
    std::fs::create_dir_all(&output_dir).expect("TTS demo output dir 应能创建");

    let config = OfflineTtsConfig {
        model: OfflineTtsModelConfig {
            matcha: OfflineTtsMatchaModelConfig {
                acoustic_model: Some(path_string(&model_dir.join("model-steps-3.onnx"))),
                vocoder: Some(path_string(&vocoder)),
                lexicon: Some(path_string(&model_dir.join("lexicon.txt"))),
                tokens: Some(path_string(&model_dir.join("tokens.txt"))),
                dict_dir: Some(path_string(&model_dir.join("dict"))),
                noise_scale: 0.667,
                length_scale: 1.0,
                ..Default::default()
            },
            num_threads: 2,
            provider: Some("cpu".to_string()),
            debug: false,
            ..Default::default()
        },
        rule_fsts: Some(format!(
            "{},{},{}",
            path_string(&model_dir.join("phone.fst")),
            path_string(&model_dir.join("date.fst")),
            path_string(&model_dir.join("number.fst"))
        )),
        ..Default::default()
    };
    let tts = OfflineTts::create(&config).expect("OfflineTts 应能用 Matcha 中文模型创建");
    let generation = GenerationConfig {
        sid: env_i32("MULTISOUL_TTS_SPEAKER_ID").unwrap_or(0),
        speed: env_f32("MULTISOUL_TTS_SPEED").unwrap_or(1.0),
        ..Default::default()
    };

    println!("TTS engine: sherpa-onnx OfflineTts Matcha zh");
    println!("TTS model: {}", model_dir.display());
    println!("Vocoder: {}", vocoder.display());
    println!("Output dir: {}", output_dir.display());
    println!("Sample rate: {}", tts.sample_rate());
    println!("Num speakers: {}", tts.num_speakers());
    println!(
        "Options: speaker_id={}, speed={:.2}",
        generation.sid, generation.speed
    );

    for sample in demo_texts() {
        let start = Instant::now();
        let audio = tts
            .generate_with_config(sample.text, &generation, None::<fn(&[f32], f32) -> bool>)
            .expect("中文 TTS 应返回 GeneratedAudio");
        let elapsed = start.elapsed();
        let wav_path = output_dir.join(format!("{}.wav", sample.id));
        let saved = audio.save(&path_string(&wav_path));
        let sample_count = audio.samples().len();
        let audio_seconds = sample_count as f32 / audio.sample_rate() as f32;
        let rtf = elapsed.as_secs_f32() / audio_seconds.max(f32::EPSILON);

        println!(
            "[{}] elapsed={:.3}s audio={:.3}s rtf={:.3} sample_rate={} samples={} wav={}",
            sample.id,
            elapsed.as_secs_f32(),
            audio_seconds,
            rtf,
            audio.sample_rate(),
            sample_count,
            wav_path.display()
        );
        println!("  text={}", sample.text);

        assert!(
            sample_count > 0,
            "{} 应生成非空 samples，0 表示合成失败或模型未正常输出",
            sample.id
        );
        assert!(
            saved,
            "{} 的 WAV 应保存成功：{}",
            sample.id,
            wav_path.display()
        );
        assert!(
            wav_path.exists(),
            "{} 的 WAV 文件应存在于输出目录",
            sample.id
        );
        assert!(
            elapsed.as_secs_f32() > 0.0,
            "{} 的合成耗时应大于 0，证明实际执行了推理",
            sample.id
        );
        assert!(
            audio_seconds > 0.0,
            "{} 的音频时长应大于 0，空音频不可接受",
            sample.id
        );
    }
}

/// TTS-DEMO-2: demo 样本文本应保持短、中、长递增，便于比较不同长度的合成耗时。
///
/// 数据构造（含关键数值的推导过程）：
///   text_count = 3
///   len_short  = demo_texts()[0].text.chars().count()
///   len_medium = demo_texts()[1].text.chars().count()
///   len_long   = demo_texts()[2].text.chars().count()
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 demo_texts()
///   2. 计算三段文本字符数
///   3. 检查 id 和长度递增关系
///
/// 预期结果：
///   - 正断言：恰好三段样本
///   - 正断言：短、中、长 id 稳定
///   - 负断言：短句不应长于中句，中句不应长于长句
#[test]
fn test_tts_demo_texts_cover_short_medium_long() {
    let texts = demo_texts();
    let short_len = texts[0].text.chars().count();
    let medium_len = texts[1].text.chars().count();
    let long_len = texts[2].text.chars().count();

    assert_eq!(texts.len(), 3, "TTS demo 应固定生成短、中、长三段中文样本");
    assert_eq!(texts[0].id, "zh-short", "第一段应是短句，便于观察启动成本");
    assert_eq!(
        texts[1].id, "zh-medium",
        "第二段应是中等文本，便于观察常规句子耗时"
    );
    assert_eq!(
        texts[2].id, "zh-long",
        "第三段应是长文本，便于观察连续合成耗时"
    );
    assert!(
        short_len < medium_len,
        "短句字符数应小于中句：short={short_len}, medium={medium_len}"
    );
    assert!(
        medium_len < long_len,
        "中句字符数应小于长句：medium={medium_len}, long={long_len}"
    );
    assert_ne!(
        texts[0].text, texts[2].text,
        "短句和长文本不应相同，否则无法比较不同长度的耗时"
    );
}

fn demo_texts() -> &'static [DemoText] {
    &[
        DemoText {
            id: "zh-short",
            text: "你好，多灵魂桌面正在测试本地中文语音合成。",
        },
        DemoText {
            id: "zh-medium",
            text: "请把这段话合成为语音，并记录耗时。我们想知道短句和中等长度文本在当前机器上的实时率。",
        },
        DemoText {
            id: "zh-long",
            text: "当一个桌面工作台拥有自己的声音时，它就不只是等待输入的界面，而像一个可以回应、确认和陪伴用户完成任务的伙伴。今天的测试会生成三段中文语音：短句用于检查启动成本，中等文本用于观察稳定速度，长文本用于评估连续合成时的总体耗时和实时率。",
        },
    ]
}

fn resolve_matcha_model_dir() -> PathBuf {
    if let Ok(path) = std::env::var("MULTISOUL_MATCHA_TTS_MODEL_DIR") {
        return PathBuf::from(path);
    }
    repo_root().join("assets/models/tts/matcha-icefall-zh-baker")
}

fn resolve_matcha_vocoder_path() -> PathBuf {
    if let Ok(path) = std::env::var("MULTISOUL_MATCHA_TTS_VOCODER") {
        return PathBuf::from(path);
    }
    repo_root().join("assets/models/tts/vocos-22khz-univ.onnx")
}

fn resolve_output_dir() -> PathBuf {
    std::env::var("MULTISOUL_TTS_OUTPUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root().join("target/tts-demo/matcha"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("voice_sherpa crate should live under crates/voice_sherpa")
        .to_path_buf()
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn env_i32(name: &str) -> Option<i32> {
    std::env::var(name).ok()?.parse().ok()
}

fn env_f32(name: &str) -> Option<f32> {
    std::env::var(name).ok()?.parse().ok()
}
