use crate::{
    import_zipvoice_reference, load_zipvoice_reference, remove_zipvoice_reference,
    zipvoice_reference_root, ZipVoiceReferenceError,
};
use std::fs;
use std::path::Path;

/// TTS-REFERENCE-1：合法参考声音必须被复制、持久加载，并能连同转写一起移除。
///
/// 数据构造（含关键数值的推导过程）：
///   sample rate    = 16,000 Hz
///   channels       = 1
///   bits per sample = 16（PCM signed integer）
///   duration       = 3 s
///   sample count   = 16,000 × 3 × 1 = 48,000 samples
///   PCM bytes      = 48,000 × 2 = 96,000 bytes
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在外部临时目录创建满足 v1 契约的 WAV 与准确转写；
///   2. 导入到模型根目录，确认返回路径不再指向源文件；
///   3. 从受控目录重新加载浮点 PCM 与转写；
///   4. 移除参考声音并再次检查受控音频和转写文件。
///
/// 预期结果：
///   - 正断言：导入副本、48,000 个样本、16 kHz 和准确转写均存在；
///   - 负断言：副本不得等于源路径，移除后音频和转写不得残留。
#[test]
fn test_valid_zipvoice_reference_import_load_and_remove_lifecycle() {
    let temp = tempfile::tempdir().expect("测试应创建隔离的模型根目录");
    let source = temp.path().join("speaker.wav");
    write_pcm_wav(&source, 16_000, 1, 3);
    let model_root = temp.path().join("models");

    let imported = import_zipvoice_reference(&source, "这是准确的参考音频转写。", &model_root)
        .expect("合法 16 kHz 单声道 3 秒 PCM WAV 应成功导入");
    let loaded =
        load_zipvoice_reference(&model_root).expect("成功导入后应能从受控目录加载参考声音");

    assert!(
        imported.audio_path().is_file(),
        "导入后受控 WAV 副本必须存在"
    );
    assert_ne!(
        imported.audio_path(),
        source.as_path(),
        "参考声音必须复制到应用数据目录，不能继续依赖用户源文件"
    );
    assert_eq!(
        loaded.sample_rate_hz(),
        16_000,
        "重新加载的参考声音必须保持 16 kHz"
    );
    assert_eq!(
        loaded.samples().len(),
        48_000,
        "3 秒单声道参考声音必须精确解码为 48,000 个样本"
    );
    assert_eq!(
        loaded.transcript(),
        "这是准确的参考音频转写。",
        "导入时的准确转写必须与音频一起持久化"
    );

    remove_zipvoice_reference(&model_root).expect("移除参考声音应删除受控音频与转写");
    let reference_root = zipvoice_reference_root(&model_root);
    assert!(
        !reference_root.join("reference.wav").exists(),
        "移除后受控参考 WAV 不得残留"
    );
    assert!(
        !reference_root.join("reference.txt").exists(),
        "移除后准确转写不得残留"
    );
}

/// TTS-REFERENCE-2：不满足格式或转写要求的导入不得覆盖已有有效参考声音。
///
/// 数据构造（含关键数值的推导过程）：
///   valid samples   = 16,000 Hz × 3 s × 1 channel = 48,000
///   invalid samples = 44,100 Hz × 3 s × 2 channels = 264,600
///   valid transcript length   > 0
///   invalid transcript length = 0（trim 后为空）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 先导入合法 16 kHz 单声道参考声音作为旧配置；
///   2. 尝试导入 44.1 kHz 双声道 WAV，预期在复制前失败；
///   3. 尝试用空白转写导入合法 WAV，预期在复制前失败；
///   4. 重新加载受控配置，核对仍是第一次的音频与转写。
///
/// 预期结果：
///   - 正断言：两次无效导入分别返回格式错误和空转写错误；
///   - 负断言：旧转写和 48,000 个样本不得被无效输入替换。
#[test]
fn test_invalid_zipvoice_reference_does_not_replace_existing_configuration() {
    let temp = tempfile::tempdir().expect("测试应创建隔离的模型根目录");
    let valid = temp.path().join("valid.wav");
    let invalid = temp.path().join("stereo-44k.wav");
    write_pcm_wav(&valid, 16_000, 1, 3);
    write_pcm_wav(&invalid, 44_100, 2, 3);
    let model_root = temp.path().join("models");
    import_zipvoice_reference(&valid, "原始有效转写", &model_root)
        .expect("测试前置的合法参考声音应成功导入");

    let format_error = import_zipvoice_reference(&invalid, "错误格式转写", &model_root)
        .expect_err("44.1 kHz 双声道 WAV 必须被拒绝");
    let transcript_error =
        import_zipvoice_reference(&valid, "   ", &model_root).expect_err("空白转写必须被拒绝");
    let loaded =
        load_zipvoice_reference(&model_root).expect("无效导入后旧的有效参考声音必须仍可加载");

    assert!(
        matches!(
            format_error,
            ZipVoiceReferenceError::UnsupportedFormat { .. }
        ),
        "错误采样率和声道必须报告 UnsupportedFormat，实际为 {format_error:?}"
    );
    assert!(
        matches!(transcript_error, ZipVoiceReferenceError::EmptyTranscript),
        "空白转写必须报告 EmptyTranscript，实际为 {transcript_error:?}"
    );
    assert_eq!(
        loaded.transcript(),
        "原始有效转写",
        "无效导入不得覆盖旧的准确转写"
    );
    assert_eq!(
        loaded.samples().len(),
        48_000,
        "无效导入不得用 44.1 kHz 双声道数据替换旧的 48,000 个样本"
    );
}

/// TTS-REFERENCE-3：超过 15 秒的 WAV 必须仅凭 header 在读取 PCM 前被拒绝。
///
/// 数据构造（含关键数值的推导过程）：
///   sample rate      = 16,000 Hz
///   declared seconds = 16 s（比允许上限 15 s 多 1 s）
///   declared samples = 16,000 × 16 × 1 = 256,000 samples
///   declared bytes   = 256,000 × 2 = 512,000 PCM bytes
///   actual PCM bytes = 0（文件仅包含 44-byte WAV header）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入声明 16 秒 data chunk、但不附带 PCM payload 的合法格式 header；
///   2. 尝试导入参考声音；
///   3. 检查系统在枚举样本前按声明时长拒绝文件。
///
/// 预期结果：
///   - 正断言：返回 UnsupportedDuration，并报告 16,000 ms；
///   - 负断言：不得先读取缺失 payload 后返回 WAV 解码错误，也不得创建受控副本。
#[test]
fn test_oversized_zipvoice_reference_rejects_header_before_reading_pcm_payload() {
    let temp = tempfile::tempdir().expect("测试应创建隔离的模型根目录");
    let source = temp.path().join("declared-16-seconds.wav");
    write_declared_pcm_wav_header(&source, 16_000, 1, 16);
    let model_root = temp.path().join("models");

    let error = import_zipvoice_reference(&source, "不会被导入的转写", &model_root)
        .expect_err("声明 16 秒的参考 WAV 必须在读取 PCM payload 前被拒绝");

    assert!(
        matches!(
            error,
            ZipVoiceReferenceError::UnsupportedDuration {
                duration_millis: 16_000
            }
        ),
        "超长 WAV 必须按 header 返回 16,000 ms UnsupportedDuration，实际为 {error:?}"
    );
    assert!(
        !zipvoice_reference_root(&model_root).exists(),
        "超长 WAV 被拒绝后不得创建参考声音受控目录"
    );
}

/// TTS-REFERENCE-4：中断删除留下的 tombstone 必须在下一次正常加载扫描中彻底清理。
///
/// 数据构造（含关键数值的推导过程）：
///   active files       = reference.wav(1) + reference.txt(1) = 2
///   simulated rename   = active root → sibling `.removing`（1 次原子目录移动）
///   expected remnants = 0 active roots + 0 tombstones
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 导入一份合法参考配置；
///   2. 手动把完整配置目录改名为 `.removing`，模拟进程在提交删除后中断；
///   3. 直接执行正常加载，让启动扫描使用的恢复路径清理 tombstone；
///   4. 检查加载结果和两个目录状态。
///
/// 预期结果：
///   - 正断言：正常加载明确返回 NotConfigured，并自动清理 tombstone；
///   - 负断言：正式目录与 `.removing` tombstone 均不得残留。
#[test]
fn test_interrupted_remove_tombstone_is_cleaned_without_reviving_reference() {
    let temp = tempfile::tempdir().expect("测试应创建隔离的模型根目录");
    let source = temp.path().join("speaker.wav");
    write_pcm_wav(&source, 16_000, 1, 3);
    let model_root = temp.path().join("models");
    import_zipvoice_reference(&source, "用于删除恢复测试的准确转写", &model_root)
        .expect("测试前置参考声音应成功导入");
    let root = zipvoice_reference_root(&model_root);
    let removing = root.with_file_name("zipvoice-distill-int8-zh-en-emilia.removing");
    fs::rename(&root, &removing).expect("测试应模拟目录级删除已经提交但尚未清理");

    let load_error = load_zipvoice_reference(&model_root)
        .expect_err("正常加载扫描应清理 tombstone，且不得复活参考配置");

    assert!(
        matches!(load_error, ZipVoiceReferenceError::NotConfigured(_)),
        "清理 tombstone 后加载必须报告 NotConfigured，实际为 {load_error:?}"
    );
    assert!(!root.exists(), "删除恢复后正式参考目录不得重新出现");
    assert!(
        !removing.exists(),
        "删除恢复后 `.removing` tombstone 不得残留"
    );
}

fn write_pcm_wav(path: &Path, sample_rate: u32, channels: u16, duration_seconds: u32) {
    let frames = sample_rate * duration_seconds;
    let sample_count = frames * u32::from(channels);
    let data_bytes = sample_count * 2;
    let byte_rate = sample_rate * u32::from(channels) * 2;
    let block_align = channels * 2;
    let mut bytes = Vec::with_capacity((44 + data_bytes) as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&byte_rate.to_le_bytes());
    bytes.extend_from_slice(&block_align.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_bytes.to_le_bytes());
    bytes.resize((44 + data_bytes) as usize, 0);
    fs::write(path, bytes).expect("测试 WAV 应写入临时目录");
}

fn write_declared_pcm_wav_header(
    path: &Path,
    sample_rate: u32,
    channels: u16,
    duration_seconds: u32,
) {
    let sample_count = sample_rate * duration_seconds * u32::from(channels);
    let data_bytes = sample_count * 2;
    let byte_rate = sample_rate * u32::from(channels) * 2;
    let block_align = channels * 2;
    let mut bytes = Vec::with_capacity(44);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&byte_rate.to_le_bytes());
    bytes.extend_from_slice(&block_align.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_bytes.to_le_bytes());
    fs::write(path, bytes).expect("测试 WAV header 应写入临时目录");
}
