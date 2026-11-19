use super::*;

/// Fake worker speaking the real frame protocol: 3 chunks of 4 bytes per request, "fail" errors.
const FAKE: &str = r#"
import json, struct, sys, time
out = sys.stdout.buffer
out.write(struct.pack("<I", 0)); out.flush()
for line in sys.stdin.buffer:
    message = json.loads(line)
    if "cancel" in message:
        continue
    if message["text"] == "fail":
        out.write(struct.pack("<I", 0xFFFFFFFF)); out.flush(); continue
    for index in range(3):
        out.write(struct.pack("<I", 4) + bytes([index, 0, index, 0])); out.flush()
    out.write(struct.pack("<I", 0)); out.flush()
"#;

fn speak(text: &str, accept: usize) -> (Result<(), tts::TtsError>, Vec<Vec<u8>>) {
    let mut received = Vec::new();
    let result = synthesize_with(
        FAKE,
        Path::new("/usr/bin/python3"),
        Path::new("/nonexistent-model"),
        "vivian",
        text,
        |chunk| {
            received.push(chunk.bytes.clone());
            received.len() < accept
        },
    );
    (result, received)
}

/// 场景：同一 worker 依次处理合成、中途取消与失败，协议边界保持正确。
/// 构造：假 worker 每次请求输出 3 个 4 字节 PCM16 帧，文本 fail 返回错误帧。
/// 执行：先完整接收，再在第 1 帧后取消，再请求 fail，最后再完整接收一次。
/// 预期：完整请求得到 3 帧且采样内容按序；取消只交付 1 帧且下一请求不串帧；失败报 Protocol 后可重启恢复。
#[test]
fn qwen3_worker_streams_cancels_and_recovers() {
    let _serial = lock_tests();
    shutdown_qwen3_tts();
    let (result, chunks) = speak("你好", usize::MAX);
    result.expect("完整合成");
    assert_eq!(
        chunks,
        vec![vec![0, 0, 0, 0], vec![1, 0, 1, 0], vec![2, 0, 2, 0]],
        "按序交付全部帧"
    );

    let (result, chunks) = speak("打断", 1);
    result.expect("取消后应正常排空");
    assert_eq!(chunks.len(), 1, "取消后不得继续交付音频");

    let (result, chunks) = speak("下一句", usize::MAX);
    result.expect("取消后同一 worker 可继续使用");
    assert_eq!(
        chunks[0],
        vec![0, 0, 0, 0],
        "下一请求必须从新帧开始，不能读到上次残留"
    );

    let (result, _) = speak("fail", usize::MAX);
    assert_eq!(
        result.expect_err("错误帧必须失败").kind(),
        tts::TtsErrorKind::Protocol
    );
    let (result, chunks) = speak("恢复", usize::MAX);
    result.expect("失败后应自动重启 worker");
    assert_eq!(chunks.len(), 3, "重启后完整合成");
    shutdown_qwen3_tts();
}

fn lock_tests() -> std::sync::MutexGuard<'static, ()> {
    static SERIAL: Mutex<()> = Mutex::new(());
    SERIAL.lock().unwrap_or_else(|error| error.into_inner())
}

/// 场景：真实 Qwen3-TTS worker 经 Rust 协议流式合成中文。
/// 构造：NORMA_A1_PYTHON 指向受管引擎解释器，NORMA_QWEN3_TTS_MODEL 指向已下载模型目录。
/// 执行：合成一句中文并收集全部 PCM。
/// 预期：至少两帧（证明流式）、24 kHz，总时长在 1~10 秒之间且非全静音。
#[test]
#[ignore = "requires the managed A1 engine and downloaded Qwen3-TTS weights"]
fn qwen3_real_worker_streams_chinese_speech() {
    let _serial = lock_tests();
    let python = PathBuf::from(std::env::var_os("NORMA_A1_PYTHON").expect("引擎解释器"));
    let model = PathBuf::from(std::env::var_os("NORMA_QWEN3_TTS_MODEL").expect("模型目录"));
    let mut chunks = Vec::new();
    synthesize_with(
        WORKER_SOURCE,
        &python,
        &model,
        "vivian",
        "好的，已经帮你新建了一个 Claude 卡片。",
        |chunk| {
            assert_eq!(chunk.sample_rate_hz, 24_000, "Qwen3-TTS 输出 24 kHz");
            chunks.push(chunk.bytes);
            true
        },
    )
    .expect("真实合成");
    shutdown_qwen3_tts();
    let bytes: usize = chunks.iter().map(Vec::len).sum();
    let seconds = bytes as f64 / 2.0 / 24_000.0;
    assert!(
        chunks.len() >= 2,
        "必须分块流式输出，实际 {} 块",
        chunks.len()
    );
    assert!((1.0..10.0).contains(&seconds), "时长异常: {seconds}s");
    assert!(chunks.iter().flatten().any(|&b| b != 0), "不能是全静音");
}
