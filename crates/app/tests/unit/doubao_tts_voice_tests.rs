use super::*;

/// 豆包 TTS 请求使用设置选定的小何音色，替代旧的固定 Vivi。
///
/// 数据构造（含关键数值的推导过程）：
///   选定小何 2.0、合成文本“你好”、合成密钥 synthetic-key；不连接真实云端。
///   输出沿用播放器格式：24 kHz = 24,000 Hz，单声道 PCM16。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从官方音色目录选择小何 → 构建实际发送的 HTTP 请求。
///   2. 解析请求头和 JSON 请求体 → 检查所选音色和音频格式。
///
/// 预期结果：
///   - speaker 是小何，不能仍为 Vivi；资源保持 seed-tts-2.0。
///   - 文本完整保留；输出为 24 kHz PCM，不能改成 MP3。
#[test]
fn test_doubao_tts_request_uses_selected_voice() {
    let voice = tts::providers::doubao::find_voice("zh_female_xiaohe_uranus_bigtts")
        .expect("官方目录应包含小何 2.0");
    let request = build_request(
        &Client::new(),
        "synthetic-key",
        "你好",
        voice,
        "https://openspeech.bytedance.com/api/v3/plan/tts/unidirectional",
    )
    .expect("选定音色请求应可编码");
    let body: Value = serde_json::from_slice(
        request
            .body()
            .and_then(reqwest::Body::as_bytes)
            .expect("请求体应包含完整 JSON"),
    )
    .expect("请求体应为 JSON");
    assert_eq!(
        request.headers()["X-Api-Resource-Id"],
        "seed-tts-2.0",
        "系统音色必须使用 2.0 合成资源"
    );
    assert_eq!(
        body["req_params"]["speaker"], "zh_female_xiaohe_uranus_bigtts",
        "实际请求必须使用所选小何"
    );
    assert_ne!(
        body["req_params"]["speaker"], "zh_female_vv_uranus_bigtts",
        "实际请求不能仍固定为 Vivi"
    );
    assert_eq!(
        body["req_params"]["text"], "你好",
        "音色切换不能改变回复文本"
    );
    assert_eq!(
        body["req_params"]["audio_params"]["sample_rate"], 24_000,
        "必须保持播放器的 24 kHz 采样率"
    );
    assert_eq!(
        body["req_params"]["audio_params"]["format"], "pcm",
        "必须使用 PCM 格式"
    );
    assert_ne!(
        body["req_params"]["audio_params"]["format"], "mp3",
        "压缩音频不能直接进入 PCM 播放器"
    );
}
