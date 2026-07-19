use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;
use voice_azure::{AzureSpeechClient, AzureSpeechConfig};

struct PreparedWav {
    path: PathBuf,
    _directory: TempDir,
}

fn convert_to_azure_wav(source: &Path) -> PreparedWav {
    let directory = tempfile::tempdir().expect("在线 Demo 应创建临时音频目录");
    let output = directory.path().join("azure-speech-test.wav");
    let conversion = Command::new("afconvert")
        .arg("-f")
        .arg("WAVE")
        .arg("-d")
        .arg("LEI16@16000")
        .arg("-c")
        .arg("1")
        .arg(source)
        .arg(&output)
        .output()
        .expect("在线 Demo 需要 macOS 自带的 afconvert");
    assert!(
        conversion.status.success(),
        "afconvert 应把输入音频转换成 16 kHz 单声道 PCM/WAV；stderr={}",
        String::from_utf8_lossy(&conversion.stderr)
    );
    assert!(output.is_file(), "afconvert 成功后应生成临时 WAV 文件");
    PreparedWav {
        path: output,
        _directory: directory,
    }
}

fn normalize_transcript(text: &str) -> String {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// AZ-STT-LIVE-1：真实 Azure Speech 应能识别本地短音频。
///
/// 数据构造（含关键数值的推导过程）：
///   source = AZURE_SPEECH_TEST_AUDIO 指向的本地音频
///   converted WAV = 16,000 samples/s × 16 bits/sample × 1 channel = 256 kbps
///   region = AZURE_SPEECH_REGION（本次资源为 eastus）
///   locale = AZURE_SPEECH_LOCALE，未设置时为 zh-CN
///
/// 执行过程（逐步说明系统如何处理）：
///   1. afconvert 把 M4A/其他输入统一转换为 16 kHz 单声道 PCM/WAV
///   2. client 从环境变量读取 region 与轮换后的订阅密钥
///   3. client 上传 WAV，解析 Azure 的 RecognitionStatus 与 DisplayText
///   4. 若提供 AZURE_SPEECH_EXPECTED_TEXT，则忽略空白与标点后匹配预期文本
///
/// 预期结果：
///   - 正断言：临时 WAV 具有 RIFF/WAVE 文件标识且 Azure 返回非空文本
///   - 正断言：设置预期文本时，识别结果应包含该文本
///   - 负断言：识别文本不得包含 NUL 字符
///   - 负断言：该测试默认 ignored，不应在普通 cargo test 中访问 Azure 或消耗额度
#[test]
#[ignore = "需要真实 Azure Speech 凭据、网络与本地测试音频"]
fn test_real_azure_speech_transcribes_local_audio() {
    let key =
        std::env::var("AZURE_SPEECH_KEY").expect("运行在线 Demo 前应设置轮换后的 AZURE_SPEECH_KEY");
    let region = std::env::var("AZURE_SPEECH_REGION").unwrap_or_else(|_| "eastus".to_string());
    let locale = std::env::var("AZURE_SPEECH_LOCALE").unwrap_or_else(|_| "zh-CN".to_string());
    let source = PathBuf::from(
        std::env::var("AZURE_SPEECH_TEST_AUDIO")
            .expect("运行在线 Demo 前应设置 AZURE_SPEECH_TEST_AUDIO"),
    );
    assert!(source.is_file(), "AZURE_SPEECH_TEST_AUDIO 应指向可读文件");

    let prepared = convert_to_azure_wav(&source);
    let wav_bytes = std::fs::read(&prepared.path).expect("在线 Demo 应读取转换后的临时 WAV");
    assert!(
        wav_bytes.starts_with(b"RIFF"),
        "转换后的 Azure 测试音频应以 RIFF 标识开头"
    );
    assert!(
        wav_bytes.windows(4).any(|window| window == b"WAVE"),
        "转换后的 Azure 测试音频应包含 WAVE 标识"
    );

    let config = AzureSpeechConfig::for_region(&region, key, locale)
        .expect("AZURE_SPEECH_REGION 应能构造识别 Endpoint");
    let recognition = AzureSpeechClient::new(config)
        .transcribe_short_wav(&wav_bytes)
        .expect("真实 Azure Speech 应成功识别测试音频");

    assert!(
        !recognition.display_text.trim().is_empty(),
        "真实 Azure Speech 应返回非空 DisplayText"
    );
    assert!(
        !recognition.display_text.contains('\0'),
        "Azure Speech DisplayText 不应包含 NUL 字符"
    );
    if let Ok(expected) = std::env::var("AZURE_SPEECH_EXPECTED_TEXT") {
        let normalized_actual = normalize_transcript(&recognition.display_text);
        let normalized_expected = normalize_transcript(&expected);
        assert!(
            normalized_actual.contains(&normalized_expected),
            "忽略空白与标点后，识别文本应包含预期文本；actual={} expected={expected}",
            recognition.display_text
        );
    }
    println!("Azure Speech transcript: {}", recognition.display_text);
}
