use super::{chunks_100ms, decode_fixture, fixture_path_from, start_fixture_stream};
use crate::VoiceInputError;
use std::path::Path;
use std::time::{Duration, Instant};

fn write_wav(path: &Path, rate: u32, channels: u16, frames: &[Vec<i16>]) {
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).expect("test wav must be creatable");
    for frame in frames {
        for sample in frame {
            writer.write_sample(*sample).expect("test wav sample");
        }
    }
    writer.finalize().expect("test wav finalize");
}

/// FIXTURE-1: 夹具 WAV 必须被混为单声道并重采样到 16 kHz，否则实时管线收到的速率与真实麦克风不一致。
///
/// 数据构造（含关键数值的推导过程）：
///   48 kHz 双声道，24000 帧 = 0.5 秒；左声道 16384、右声道 0
///   混音后每个采样 = (16384 + 0) / 2 / 32768 = 0.25
///   重采样后长度 = 0.5 秒 × 16000 = 8000，允许线性插值边界误差 ±4
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入临时 WAV
///   2. decode_fixture 读取、混音、重采样
///
/// 预期结果：
///   - 正断言：长度在 8000 ± 4，说明已经降到 16 kHz
///   - 正断言：中段采样约为 0.25，说明左右声道取平均而不是只取一侧
///   - 负断言：长度不是 24000，避免漏掉重采样
#[test]
fn test_decode_fixture_downmixes_and_resamples_to_16khz() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("stereo48k.wav");
    write_wav(&path, 48_000, 2, &vec![vec![16_384, 0]; 24_000]);

    let samples = decode_fixture(&path).expect("fixture must decode");

    assert!(
        (7_996..=8_004).contains(&samples.len()),
        "length {} must be about 8000",
        samples.len()
    );
    assert_ne!(samples.len(), 24_000, "must not skip resampling");
    assert!(
        (samples[4_000] - 0.25).abs() < 0.01,
        "got {}",
        samples[4_000]
    );
}

/// FIXTURE-2: 缺失的夹具文件必须报错，而不是静默变成无声的"麦克风"。
///
/// 数据构造：路径指向不存在的文件。
/// 执行过程：decode_fixture 打开文件。
/// 预期结果：返回 VoiceInputError::Audio，信息包含路径，便于排查。
#[test]
fn test_decode_fixture_reports_missing_file() {
    let error = decode_fixture(Path::new("/nonexistent/e2e-fixture.wav"))
        .expect_err("a missing fixture must fail");

    match error {
        VoiceInputError::Audio(message) => {
            assert!(message.contains("e2e-fixture.wav"), "{message}")
        }
        other => panic!("expected Audio error, got {other:?}"),
    }
}

/// FIXTURE-3: 夹具按 100 ms 切块，末尾不足一块的部分也要发出。
///
/// 数据构造（含关键数值的推导过程）：
///   4000 个采样 @16 kHz；100 ms = 1600 采样 = 3200 字节（PCM16）
///   4000 = 1600 + 1600 + 800，所以应得到 3 块：3200、3200、1600 字节
///
/// 预期结果：块数为 3、字节数依次为 3200/3200/1600，采样率 16000。
#[test]
fn test_chunks_are_100ms_with_a_short_tail() {
    let chunks = chunks_100ms(&vec![0.1; 4_000]);

    let sizes: Vec<usize> = chunks.iter().map(|chunk| chunk.bytes.len()).collect();
    assert_eq!(sizes, vec![3_200, 3_200, 1_600]);
    assert!(chunks.iter().all(|chunk| chunk.sample_rate_hz == 16_000));
}

/// FIXTURE-4: 夹具播完后必须持续输出静音，会话才不会像"麦克风被拔掉"那样断流。
///
/// 数据构造（含关键数值的推导过程）：
///   3200 个常量 0.5 的采样 = 2 块（各 1600 采样）
///   期望按真实时间每 100 ms 一块：前 2 块非零，之后全是 0
///   2 块有声 + 2 块静音 = 至少约 300 ms 的节奏，整体不得快于 250 ms（证明有按实时节奏发送）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写 16 kHz 单声道 WAV，启动夹具流
///   2. 读取 4 块
///   3. drop 会话，应在 1 秒内停止线程
///
/// 预期结果：
///   - 正断言：前两块含非零字节，后两块全 0
///   - 正断言：读取 4 块耗时不少于 250 ms（实时节奏，不是一次性灌入）
///   - 正断言：drop 在 1 秒内完成
#[test]
fn test_fixture_stream_plays_audio_then_silence_in_real_time() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("tone.wav");
    write_wav(&path, 16_000, 1, &vec![vec![16_384]; 3_200]);

    let started = Instant::now();
    let (session, receiver) = start_fixture_stream(&path).expect("fixture stream must start");
    let mut chunks = Vec::new();
    for _ in 0..4 {
        chunks.push(receiver.recv_blocking().expect("chunk must arrive"));
    }
    let elapsed = started.elapsed();
    let drop_started = Instant::now();
    drop(session);

    assert!(chunks[0].bytes.iter().any(|byte| *byte != 0));
    assert!(chunks[1].bytes.iter().any(|byte| *byte != 0));
    assert!(chunks[2].bytes.iter().all(|byte| *byte == 0));
    assert!(chunks[3].bytes.iter().all(|byte| *byte == 0));
    assert!(
        elapsed >= Duration::from_millis(250),
        "paced too fast: {elapsed:?}"
    );
    assert!(drop_started.elapsed() < Duration::from_secs(1));
}

/// FIXTURE-5: 环境变量为空或未设置时不得启用夹具，避免误把真实麦克风换成夹具。
///
/// 数据构造：None、空串、有路径三种输入。
/// 预期结果：只有非空路径返回 Some。
#[test]
fn test_fixture_path_requires_a_non_empty_value() {
    assert_eq!(fixture_path_from(None), None);
    assert_eq!(fixture_path_from(Some(String::new())), None);
    assert_eq!(
        fixture_path_from(Some("/tmp/a.wav".to_string())),
        Some("/tmp/a.wav".into())
    );
}
