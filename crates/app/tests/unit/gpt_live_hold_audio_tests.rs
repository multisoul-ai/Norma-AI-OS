use super::*;

async fn packet(wire: &async_channel::Receiver<Value>, kind: &str) -> Value {
    loop {
        let value = wire.recv().await.expect("服务端仍在接收");
        if value["type"] == kind {
            return value;
        }
    }
}

fn pcm(packet: &Value) -> Vec<u8> {
    base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        packet["audio"].as_str().expect("PCM字段"),
    )
    .expect("PCM解码")
}

/// 场景：真实音频循环只上传按住区间的PCM，松键后保留连接，按键不覆盖主动静音。
/// 构造：本地WebSocket、可观察capture watch与200ms标记PCM；16kHz×200ms×2字节=6400字节，共10帧。
/// 执行：初始等待→按下上传0x11→松开并注入迟到0x22→主动静音后再次按下→取消静音上传0x33。
/// 预期：松键不丢其余9帧尾音；停止确认后的0x22不上传；主动静音保持；同一连接正常关闭。
#[tokio::test]
async fn live_hold_audio_gates_pcm_without_overriding_manual_mute() {
    tokio::time::timeout(std::time::Duration::from_secs(6), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("本地服务");
        let address = listener.local_addr().expect("地址");
        let (observed, wire) = async_channel::unbounded();
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.expect("客户端");
            let mut socket = accept_async(tcp).await.expect("握手");
            let _ = socket.next().await.expect("启动消息").expect("完整消息");
            socket.send(Message::Text(json!({"type":"session.started","session":{"id":"hold-audio","model":"gpt-live-1"}}).to_string().into())).await.expect("连接就绪");
            while let Some(Ok(message)) = socket.next().await {
                let value: Value = serde_json::from_str(message.to_text().expect("JSON文本")).expect("JSON");
                if value["type"] == "session.close" { break; }
                observed.send(value).await.expect("观察上传");
            }
            socket.send(Message::Text(json!({"type":"session.closed","reason":"close_requested","session":{"id":"hold-audio","model":"gpt-live-1"},"usage":{"seconds":1.0}}).to_string().into())).await.expect("关闭确认");
        });
        let connection = live_voice::connect_loopback(address, live_voice::LiveConfig::default(), "fixture", CancellationToken::new()).await.expect("本地连接");
        let (bus, _wake) = VoiceWindowInsetEventBus::new();
        let (written, _played) = async_channel::bounded(1);
        let playback = start_playback_with(move || Box::new(SignalingSink { written, stopped: Arc::new(AtomicBool::new(false)) }), bus.sink(TtsPlaybackSource::GptLive));
        let (microphone, captured) = async_channel::bounded(8);
        let (capture_mode, mut mode) = tokio::sync::watch::channel(CaptureMode { generation: 0, muted: true });
        let (commands, controls) = async_channel::bounded(8);
        let (events, visible) = async_channel::bounded(32);
        let cancellation = CancellationToken::new();
        let worker = tokio::spawn(run_audio_loop(connection, CaptureInput { captured, capture_mode, vad: None }, playback, controls, async_channel::bounded(1).1, events, cancellation.clone()));
        assert_eq!(pcm(&packet(&wire, "session.input_audio.append").await), vec![0;640], "尚未按键只发送协议静音");
        commands.send(CallCommand::SetInputEnabled { enabled: true, revision: 1 }).await.expect("按下");
        mode.changed().await.expect("采集门控更新");
        let first = *mode.borrow_and_update();
        assert!(!first.muted, "按下开启采集");
        microphone.send(CaptureEvent::Ready { generation: first.generation }).await.expect("硬件就绪");
        microphone.send(CaptureEvent::Pcm { generation: first.generation, chunk: PcmAudioChunk::new(PcmFormat::Pcm16,16_000,1,vec![0x11;6400]).expect("按住期间已采集200ms") }).await.expect("输入PCM");
        loop { if pcm(&packet(&wire, "session.input_audio.append").await) == vec![0x11;640] { break; } }
        commands.send(CallCommand::SetInputEnabled { enabled: false, revision: 2 }).await.expect("松开");
        mode.changed().await.expect("关闭采集");
        let released = *mode.borrow_and_update();
        assert!(released.muted, "松键后采集关闭");
        assert_eq!(released.generation, first.generation, "松键保留当前代次直到已采音频排空");
        microphone.send(CaptureEvent::Stopped { generation: first.generation }).await.expect("采集已停止，全部尾音已入队");
        for _ in 0..9 { assert_eq!(pcm(&packet(&wire, "session.input_audio.append").await), vec![0x11;640], "松键后必须继续上传已采集尾音"); }
        loop { if matches!(visible.recv().await.expect("尾音排空确认"), CallEvent::InputReleased(2)) { break; } }
        microphone.send(CaptureEvent::Pcm { generation: first.generation, chunk: PcmAudioChunk::new(PcmFormat::Pcm16,16_000,1,vec![0x22;640]).expect("停止后迟到PCM") }).await.expect("迟到输入");
        for _ in 0..3 { assert_eq!(pcm(&packet(&wire, "session.input_audio.append").await), vec![0;640], "松键后不可上传迟到0x22"); }
        commands.send(CallCommand::SetMuted(true)).await.expect("主动静音");
        loop { if matches!(visible.recv().await.expect("静音状态"), CallEvent::Muted(true)) { break; } }
        commands.send(CallCommand::SetInputEnabled { enabled: true, revision: 3 }).await.expect("静音时按下");
        for _ in 0..3 { assert_eq!(pcm(&packet(&wire, "session.input_audio.append").await), vec![0;640], "按键不取消主动静音"); }
        assert!(mode.borrow().muted, "主动静音期间硬件门控保持关闭");
        commands.send(CallCommand::SetMuted(false)).await.expect("显式取消静音");
        mode.changed().await.expect("恢复采集");
        let resumed = *mode.borrow_and_update();
        assert!(!resumed.muted, "只有显式取消静音才恢复采集");
        microphone.send(CaptureEvent::Pcm { generation: resumed.generation, chunk: PcmAudioChunk::new(PcmFormat::Pcm16,16_000,1,vec![0x33;640]).expect("新PCM") }).await.expect("新输入");
        loop { if pcm(&packet(&wire, "session.input_audio.append").await) == vec![0x33;640] { break; } }
        cancellation.cancel();
        let result = worker.await.expect("音频循环结束");
        assert!(result.error.is_none(), "门控不破坏同一连接：{:?}", result.error);
        assert!(result.usage.finalized, "同一通话正常关闭");
        server.await.expect("服务端完成");
    }).await.expect("按住音频验收有界");
}

/// 场景：最后20ms已经离开本地麦克风缓冲，但仍在transport队列时，不能开放项目工具。
/// 构造：本地WS与可控Tokio时钟；640字节=16kHz单声道PCM的20ms，使用真实transport预算。
/// 执行：先确认上传时钟→暂停时间→采集一帧并松键→推进20ms→仅轮询媒体循环，不调度writer。
/// 预期：尾帧待发时不能放行；恢复writer后会放行，即使之后还有8帧协议静音等待发送。
#[tokio::test]
async fn live_hold_waits_for_transport_to_send_the_last_frame() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("本地服务");
    let address = listener.local_addr().expect("地址");
    let (first, ready) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.expect("客户端");
        let mut socket = accept_async(tcp).await.expect("握手");
        let _ = socket.next().await.expect("启动").expect("完整启动");
        socket.send(Message::Text(json!({"type":"session.started","session":{"id":"hold-fence","model":"gpt-live-1"}}).to_string().into())).await.expect("就绪");
        let _ = socket.next().await.expect("首帧").expect("完整首帧");
        first.send(()).expect("上传时钟就绪");
        while let Some(Ok(message)) = socket.next().await {
            if message.to_text().expect("文本").contains("session.close") {
                break;
            }
        }
        socket.send(Message::Text(json!({"type":"session.closed","reason":"close_requested","session":{"id":"hold-fence","model":"gpt-live-1"},"usage":{"seconds":1.0}}).to_string().into())).await.expect("关闭确认");
    });
    let connection = live_voice::connect_loopback(
        address,
        live_voice::LiveConfig::default(),
        "fixture",
        CancellationToken::new(),
    )
    .await
    .expect("本地连接");
    let transport = connection.control();
    let (bus, _wake) = VoiceWindowInsetEventBus::new();
    let (written, _played) = async_channel::bounded(1);
    let playback = start_playback_with(
        move || {
            Box::new(SignalingSink {
                written,
                stopped: Arc::new(AtomicBool::new(false)),
            })
        },
        bus.sink(TtsPlaybackSource::GptLive),
    );
    let (microphone, captured) = async_channel::bounded(8);
    let (capture_mode, _mode) = tokio::sync::watch::channel(CaptureMode {
        generation: 0,
        muted: false,
    });
    let (commands, controls) = async_channel::bounded(8);
    let (events, visible) = async_channel::bounded(32);
    let cancellation = CancellationToken::new();
    let worker = run_audio_loop(
        connection,
        CaptureInput {
            captured,
            capture_mode,
            vad: None,
        },
        playback,
        controls,
        async_channel::bounded(1).1,
        events,
        cancellation.clone(),
    );
    tokio::pin!(worker);
    tokio::select! {
        _ = &mut worker => panic!("首帧前不能结束通话"),
        result = ready => result.expect("首帧确认"),
    }
    tokio::time::pause();
    microphone
        .send(CaptureEvent::Pcm {
            generation: 0,
            chunk: PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0x55; 640])
                .expect("尾帧20ms"),
        })
        .await
        .expect("尾帧进入采集队列");
    assert!(
        futures::poll!(&mut worker).is_pending(),
        "尾帧先进入本地缓冲"
    );
    commands
        .send(CallCommand::SetInputEnabled {
            enabled: false,
            revision: 1,
        })
        .await
        .expect("松键");
    microphone
        .send(CaptureEvent::Stopped { generation: 0 })
        .await
        .expect("硬件停止且尾帧入队完成");
    assert!(
        futures::poll!(&mut worker).is_pending(),
        "处理停止确认但仍等待上传"
    );
    tokio::time::advance(std::time::Duration::from_millis(20)).await;
    assert!(
        futures::poll!(&mut worker).is_pending(),
        "尾帧交给transport后继续等待发送"
    );
    assert!(
        transport.queued_audio_bytes() > 0,
        "测试必须确实留有待发transport帧，不能以空队列假通过"
    );
    while let Ok(event) = visible.try_recv() {
        assert!(
            !matches!(event, CallEvent::InputReleased(1)),
            "尾帧尚未发送不能提前开放项目工具"
        );
    }
    for _ in 0..8 {
        transport
            .send_silence(320)
            .expect("最后真实尾帧之后仍有协议静音积压");
    }
    tokio::time::resume();
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            tokio::select! {
                outcome = &mut worker => panic!("发送恢复前不能结束通话：{:?}", outcome.error),
                event = visible.recv() => if matches!(event.expect("上传确认"), CallEvent::InputReleased(1)) { break; },
            }
        }
    }).await.expect("真实尾帧上传后必须最终恢复工具准入");
    assert!(
        transport.queued_audio_bytes() > 0,
        "已释放输入时仍有后续静音积压，不能等待全局队列归零"
    );
    cancellation.cancel();
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(3), worker)
        .await
        .expect("关闭有界");
    assert!(
        outcome.error.is_none(),
        "关闭保留原通话生命周期：{:?}",
        outcome.error
    );
    server.await.expect("服务端结束");
}
