use super::*;
use audio::{PcmAudioChunk, PcmFormat};
use serde_json::Value;
use std::time::Duration;

/// L3：真实豆包 ASR 在按住模式松开后必须返回非空的完整转写。
///
/// 数据构造（含关键数值的推导过程）：NORMA_HOLD_PCM 指向系统语音合成的
///   “请只回答语音测试成功，不要执行任何项目操作”原始 PCM；
///   16,000 Hz × 2 B × 0.1 s = 3,200 B，每 100 ms 上传一帧。
/// 执行过程：读取本机已保存凭据 → 运行生产按住识别入口 → 上传测试语音
///   → 关闭 PCM 通道模拟松键 → 读取唯一最终事件并等待连接结束。
/// 预期结果：录音 id 1 的转写包含“语音测试”；不产生空识别终态或重复提交。
#[tokio::test]
#[ignore = "需要本机豆包凭据、NORMA_HOLD_PCM 测试音频与联网，消耗少量语音额度"]
async fn doubao_hold_real_transcribes_after_release() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let store =
        storage::SettingsStore::new(storage::default_app_db_path().expect("应定位本机设置数据库"));
    let key = store
        .get_string("credential.agent-provider.doubao-desktop")
        .expect("应读取已保存的豆包凭据")
        .expect("本机应已保存豆包凭据");
    let pcm = std::fs::read(std::env::var("NORMA_HOLD_PCM").expect("应指定测试 PCM 文件"))
        .expect("测试 PCM 应可读取");
    assert!(pcm.len() >= 3_200, "测试语音应至少包含 100 ms 音频");
    assert_eq!(pcm.len() % 2, 0, "PCM16 测试文件必须包含完整采样");
    let (pcm_tx, pcm_rx) = async_channel::unbounded();
    let (events_tx, events_rx) = async_channel::unbounded();
    let task = tokio::spawn(run(
        key,
        events_tx,
        CancellationToken::new(),
        Some((1, pcm_rx)),
    ));
    for frame in pcm.chunks(3_200) {
        pcm_tx
            .send(Ok(PcmAudioChunk::new(
                PcmFormat::Pcm16,
                16_000,
                1,
                frame.to_vec(),
            )
            .expect("测试音频应为单声道 PCM16")))
            .await
            .expect("识别入口应接收音频");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(events_rx.try_recv().is_err(), "松键前不能提交给 AI");
    drop(pcm_tx);
    // 10 s 连接 + 10 s 配置 + 10 s 最终识别，最多等待 30 s。
    let result = tokio::time::timeout(Duration::from_secs(30), task)
        .await
        .expect("真实按住识别应在有限时间内结束")
        .expect("识别任务不应 panic");
    assert!(result.is_ok(), "松键后的真实识别失败：{result:?}");
    let event = events_rx.try_recv().expect("松键后必须返回识别结果");
    assert!(
        matches!(event, SpeechEvent::HeldFinal(1, ref text) if text.contains("语音测试")),
        "真实测试语音必须产生非空转写，不能只返回空录音终态"
    );
    assert!(events_rx.try_recv().is_err(), "真实录音不得重复提交");
}

fn response(value: Value, last: bool) -> Message {
    let payload = serde_json::to_vec(&value).expect("测试响应应可编码");
    let mut frame = vec![0x11, if last { 0x93 } else { 0x91 }, 0x10, 0];
    frame.extend_from_slice(&(if last { -3_i32 } else { 2_i32 }).to_be_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    Message::Binary(frame.into())
}

/// 按住时的句间停顿不能触发 AI，松开后的最终帧必须合并完整转写。
///
/// 数据构造（含关键数值的推导过程）：100 ms PCM = 16,000 × 2 × 0.1 = 3,200 B；
///   先返回“第一句”的 definite，再在松开后的 full 最终帧返回“第一句第二句”。
/// 执行过程：本地假上游确认配置 → 接收音频并提前断句 → 关闭音频通道模拟松键
///   → 上游接收负序号结束帧 → 返回最终转写 → 检查唯一提交。
/// 预期结果：松键前没有 Final；松键后仅有“第一句第二句”；网络使用结束帧且成功释放。
#[tokio::test]
async fn test_doubao_hold_asr_submits_once_after_release() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("应监听本地端口");
    let url = format!("ws://{}", listener.local_addr().expect("应取得端口"));
    let (paused_tx, paused_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("应接收 ASR 连接");
        let mut ws = tokio_tungstenite::accept_async(stream)
            .await
            .expect("应完成本地握手");
        let config = ws.next().await.expect("应收到配置").expect("配置帧应有效");
        let Message::Binary(config) = config else {
            panic!("ASR 配置必须是二进制帧");
        };
        let mut decoded = String::new();
        std::io::Read::read_to_string(
            &mut flate2::read::GzDecoder::new(&config[12..]),
            &mut decoded,
        )
        .expect("应解码配置");
        let config: Value = serde_json::from_str(&decoded).expect("配置应为 JSON");
        assert_eq!(
            config["request"]["result_type"], "full",
            "按住录音必须请求累计结果，否则最后一句可能覆盖此前停顿分句"
        );
        ws.send(response(json!({"code":1000}), false))
            .await
            .expect("应确认配置");
        ws.next().await.expect("应收到音频").expect("音频帧应有效");
        ws.send(response(
            json!({"result":{"utterances":[{"text":"第一句","end_time":100,"definite":true}]}}),
            false,
        ))
        .await
        .expect("应返回句间停顿");
        paused_tx.send(()).expect("应通知测试断句已返回");
        let Message::Binary(last) = ws
            .next()
            .await
            .expect("应收到结束帧")
            .expect("结束帧应有效")
        else {
            panic!("松键必须发送二进制结束帧");
        };
        assert_eq!(last[1], 0x23, "松键必须发送带 last flag 的音频帧");
        assert!(
            i32::from_be_bytes(last[4..8].try_into().expect("序号应为四字节")) < 0,
            "结束帧必须使用负序号"
        );
        ws.send(response(json!({"result":{"text":"第一句第二句","utterances":[{"text":"第一句","end_time":100,"definite":true},{"text":"第二句","end_time":200,"definite":false}]}}), true)).await.expect("应返回完整最终结果");
    });
    let (pcm_tx, pcm_rx) = async_channel::unbounded();
    let (events_tx, events_rx) = async_channel::unbounded();
    let task = tokio::spawn(run_at(
        url,
        "synthetic-key".into(),
        events_tx,
        CancellationToken::new(),
        Some((1, pcm_rx)),
    ));
    pcm_tx
        .send(Ok(PcmAudioChunk::new(
            PcmFormat::Pcm16,
            16_000,
            1,
            vec![0; 3_200],
        )
        .expect("测试 PCM 格式应有效")))
        .await
        .expect("应提供 100 ms 音频");
    // 正向等待容许 40 × 50 ms 调度周期 = 2 s；负向观察窗口为 1 × 50 ms。
    tokio::time::timeout(Duration::from_secs(2), paused_rx)
        .await
        .expect("假上游应及时返回停顿")
        .expect("上游任务不应提前退出");
    while let Ok(event) = events_rx.try_recv() {
        assert!(
            !matches!(event, SpeechEvent::Final(_) | SpeechEvent::HeldFinal(_, _)),
            "按住时出现 definite 也不能提前提交 AI"
        );
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(50), events_rx.recv())
            .await
            .is_err(),
        "松键前不能产生后续提交"
    );
    drop(pcm_tx);
    let event = tokio::time::timeout(Duration::from_secs(2), events_rx.recv())
        .await
        .expect("松键后应及时提交")
        .expect("应返回最终结果");
    assert!(
        matches!(event, SpeechEvent::HeldFinal(1, ref text) if text == "第一句第二句"),
        "最终提交必须包含两句且不重复第一句"
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("识别应及时结束")
            .expect("识别任务不应 panic")
            .is_ok(),
        "结束帧应成功释放识别连接"
    );
    assert!(events_rx.try_recv().is_err(), "单次录音不得重复提交");
    server.await.expect("假上游不应 panic");
}

/// 上游在用户松键前异常结束不能提前触发 AI。
///
/// 数据构造（含关键数值的推导过程）：音频通道保持打开；上游返回带负序号的
///   “提前结束”终帧；无录音结束信号。等待上限 2 s = 40 × 50 ms 调度周期。
/// 执行过程：完成本地握手 → 上游直接返回终帧 → 等待识别任务结束并检查事件。
/// 预期结果：识别返回提前终止错误；不产生 HeldFinal 或普通 Final 提交。
#[tokio::test]
async fn test_doubao_hold_asr_rejects_final_before_release() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("应监听本地端口");
    let url = format!("ws://{}", listener.local_addr().expect("应取得端口"));
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("应接收连接");
        let mut ws = tokio_tungstenite::accept_async(stream)
            .await
            .expect("应完成握手");
        ws.next().await.expect("应收到配置").expect("配置应有效");
        ws.send(response(json!({"result":{"text":"提前结束"}}), true))
            .await
            .expect("应发送终帧");
    });
    let (_pcm_tx, pcm_rx) = async_channel::unbounded();
    let (events_tx, events_rx) = async_channel::unbounded();
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        run_at(
            url,
            "synthetic-key".into(),
            events_tx,
            CancellationToken::new(),
            Some((1, pcm_rx)),
        ),
    )
    .await
    .expect("识别应及时结束");
    assert_eq!(
        result,
        Err(LocalizedText::new("doubao.asr_ended_before_release")),
        "未松键的上游终帧必须作为异常处理"
    );
    assert!(events_rx.try_recv().is_err(), "提前终帧不能提交给 AI");
    server.await.expect("假上游不应 panic");
}

/// 新录音已排队时，旧录音的最终结果断线不能终止整个会话。
///
/// 数据构造（含关键数值的推导过程）：录音 1 通道已关闭，录音 2 通道也已关闭；
///   各自发送 100 ms 静音 = 16,000 × 2 × 0.1 = 3,200 B 结束帧；
///   等待上限 2 s = 40 × 50 ms 调度周期。
/// 执行过程：旧请求握手并等待终帧 → 入队新请求 → 旧连接断开
///   → 新请求建立并收到最终结果 → 取消会话。
/// 预期结果：只提交录音 2 的“新录音”；旧错误不结束监听；取消后正确退出。
#[tokio::test]
async fn test_doubao_hold_asr_replaces_old_request_before_error() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("应监听本地端口");
    let url = format!("ws://{}", listener.local_addr().expect("应取得端口"));
    let (waiting_tx, waiting_rx) = tokio::sync::oneshot::channel();
    let (disconnect_tx, disconnect_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("应接收旧连接");
        let mut old = tokio_tungstenite::accept_async(stream)
            .await
            .expect("应完成旧握手");
        old.next()
            .await
            .expect("应收到旧配置")
            .expect("旧配置应有效");
        old.send(response(json!({"code":1000}), false))
            .await
            .expect("应确认旧配置");
        old.next()
            .await
            .expect("应收到旧结束帧")
            .expect("旧结束帧应有效");
        waiting_tx.send(()).expect("应通知旧请求已等待终帧");
        disconnect_rx.await.expect("应等待新录音入队");
        drop(old);
        let (stream, _) = listener.accept().await.expect("应接收新连接");
        let mut new = tokio_tungstenite::accept_async(stream)
            .await
            .expect("应完成新握手");
        new.next()
            .await
            .expect("应收到新配置")
            .expect("新配置应有效");
        new.send(response(json!({"code":1000}), false))
            .await
            .expect("应确认新配置");
        new.next()
            .await
            .expect("应收到新结束帧")
            .expect("新结束帧应有效");
        new.send(response(json!({"result":{"text":"新录音"}}), true))
            .await
            .expect("应返回新转写");
    });
    let (recordings_tx, recordings_rx) = async_channel::unbounded();
    let (events_tx, events_rx) = async_channel::unbounded();
    let cancellation = CancellationToken::new();
    let task = tokio::spawn(listen_recordings_at(
        url,
        "synthetic-key".into(),
        events_tx,
        cancellation.clone(),
        recordings_rx,
    ));
    let (old_tx, old_rx) = async_channel::unbounded();
    drop(old_tx);
    recordings_tx.send((1, old_rx)).await.expect("应提交旧录音");
    tokio::time::timeout(Duration::from_secs(2), waiting_rx)
        .await
        .expect("旧请求应及时开始等待")
        .expect("上游不应提前结束");
    let (new_tx, new_rx) = async_channel::unbounded();
    drop(new_tx);
    recordings_tx.send((2, new_rx)).await.expect("应提交新录音");
    disconnect_tx.send(()).expect("应触发旧断线");
    let event = tokio::time::timeout(Duration::from_secs(2), events_rx.recv())
        .await
        .expect("新录音应及时完成")
        .expect("旧断线不能关闭事件通道");
    assert!(
        matches!(event, SpeechEvent::HeldFinal(2, ref text) if text == "新录音"),
        "只应提交新请求转写"
    );
    assert!(
        events_rx.try_recv().is_err(),
        "旧请求不能产生提交或覆盖新请求"
    );
    assert!(
        !task.is_finished(),
        "新请求完成后会话必须继续等待下一次录音"
    );
    cancellation.cancel();
    assert!(
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("取消后应及时退出")
            .expect("监听任务不应 panic")
            .is_ok(),
        "被替换的旧错误不能成为会话结果"
    );
    server.await.expect("假上游不应 panic");
}
