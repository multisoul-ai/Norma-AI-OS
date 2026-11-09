use super::*;
use crate::app::{
    doubao::playback::start_playback_with,
    voice::playback::AssistantAudioSink,
    voice_window_inset::{TtsPlaybackSource, VoiceWindowInsetEventBus},
};
use audio::PcmFormat;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;

#[path = "gpt_live_hold_audio_tests.rs"]
mod hold_audio_tests;

struct SignalingSink {
    written: async_channel::Sender<()>,
    stopped: Arc<AtomicBool>,
}

/// AC11：正常关闭只消除明确的NotReady竞态，不能掩盖播放或PCM错误。
/// 数据构造：closed确认的真实用量形状，分别配硬件失败、NotReady和未确认终态。
/// 执行过程：通过通话终态错误归并入口组合取消与错误，检查最终用户可见信息。
/// 预期结果：硬件错误始终保留；只有明确请求关闭且服务端已确认时可忽略NotReady。
#[test]
fn live_close_preserves_hardware_errors_and_only_suppresses_expected_not_ready() {
    let mut outcome = live_voice::LiveOutcome {
        usage: live_voice::LiveUsage {
            seconds: 7.0,
            finalized: true,
        },
        reason: Some("close_requested".into()),
        error: None,
    };
    assert_eq!(
        local_call_error(
            Err(LoopError::Other("speaker failed".into())),
            true,
            &outcome
        ),
        Some("speaker failed".into()),
        "关闭不得隐藏实际扬声器失败"
    );
    assert_eq!(
        local_call_error(
            Err(LoopError::Live(live_voice::LiveError::NotReady)),
            true,
            &outcome
        ),
        None,
        "明确关闭且已确认时只忽略NotReady竞态"
    );
    assert_eq!(
        local_call_error(
            Err(LoopError::Live(live_voice::LiveError::NotReady)),
            false,
            &outcome
        ),
        Some("Live session is not ready for this command".into()),
        "非用户关闭的NotReady必须可见"
    );
    outcome.usage.finalized = false;
    assert_eq!(
        local_call_error(
            Err(LoopError::Live(live_voice::LiveError::NotReady)),
            true,
            &outcome
        ),
        Some("Live session is not ready for this command".into()),
        "未确认关闭不能掩盖错误"
    );
}
/// 传输任务先退出会让上传观察通道关闭，该派生错误不得盖住真正的连接错误。
/// 数据构造：上传观察结束的本地错误，分别配有传输错误和无传输错误的终态。
/// 预期结果：有传输错误时本地错误让位；没有时仍显示本地错误。
#[test]
fn upload_observation_ended_yields_to_transport_error() {
    let mut outcome = live_voice::LiveOutcome {
        usage: live_voice::LiveUsage {
            seconds: 183.0,
            finalized: false,
        },
        reason: None,
        error: Some(live_voice::LiveError::Connection),
    };
    assert_eq!(
        local_call_error(
            Err(LoopError::Other(UPLOAD_OBSERVATION_ENDED.into())),
            false,
            &outcome
        ),
        None,
        "传输错误存在时不得用派生的通道关闭错误覆盖它"
    );
    outcome.error = None;
    assert_eq!(
        local_call_error(
            Err(LoopError::Other(UPLOAD_OBSERVATION_ENDED.into())),
            false,
            &outcome
        ),
        Some(UPLOAD_OBSERVATION_ENDED.into()),
        "没有传输错误时仍要显示本地错误"
    );
}
impl AssistantAudioSink for SignalingSink {
    fn write_chunk(&mut self, _: &PcmAudioChunk) -> Result<std::time::Duration, String> {
        let _ = self.written.try_send(());
        Ok(std::time::Duration::from_millis(20))
    }
    fn stop(&mut self) {
        self.stopped.store(true, Ordering::Release);
    }
}

/// AC02/AC04/AC11：桌面音频循环经真实WS同时推进上下行、双向字幕，并在返回前停止播放器。
/// 数据构造：本地假Live服务、fake Key、100ms麦克风PCM及20ms输出；可观测写入的共享sink。
/// 执行过程：服务端输出并连续接收至少2帧→确认sink实际接到PCM→挂断→核对最终用量和字幕。
/// 预期结果：输出不阻断输入；上下行字幕保留；返回前sink已stop；此测试不代表真实扬声器播放。
#[tokio::test]
async fn live_audio_loop_streams_both_directions_and_releases_playback() {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let listener=TcpListener::bind("127.0.0.1:0").await.expect("本地服务");
        let address=listener.local_addr().expect("地址");
        let (duplex, verified)=tokio::sync::oneshot::channel();
        let server=tokio::spawn(async move {
            let (tcp,_)=listener.accept().await.expect("客户端");
            let mut socket=accept_async(tcp).await.expect("WS握手");
            let start=socket.next().await.expect("start").expect("start完整");
            assert!(start.to_text().expect("文本").contains("session.start"),"Live首包");
            for event in [
                json!({"type":"session.started","session":{"id":"live_app_loop","model":"gpt-live-1"}}),
                json!({"type":"session.output_audio.delta","delta":base64::Engine::encode(&base64::engine::general_purpose::STANDARD, vec![0_u8;640])}),
                json!({"type":"session.usage.updated","usage":{"seconds":1.25}}),
                json!({"type":"session.input_transcript.delta","event_id":"u","delta":"你好","start_ms":0,"end_ms":100}),
                json!({"type":"session.output_transcript.delta","event_id":"a","delta":"你好，我在听","start_ms":0,"end_ms":100}),
            ] {socket.send(Message::Text(event.to_string().into())).await.expect("ready/输出");}
            let mut heard_microphone = false;
            for _ in 0..6 {
                let packet=socket.next().await.expect("上行帧").expect("完整上行");
                let packet:Value=serde_json::from_str(packet.to_text().expect("文本")).expect("JSON");
                assert_eq!(packet["type"],"session.input_audio.append","下行期间仍持续上行");
                let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, packet["audio"].as_str().expect("PCM base64")).expect("上行PCM");
                assert_eq!(bytes.len(),640,"每帧20ms PCM");
                heard_microphone |= bytes.iter().all(|byte| *byte == 0x11);
            }
            assert!(heard_microphone,"上行必须包含真实采集PCM，合成静音不能冒充收音");
            duplex.send(()).expect("双工已验证");
            loop {
                let event=socket.next().await.expect("close前事件").expect("完整事件");
                if event.to_text().expect("文本").contains("session.close") {break;}
            }
            socket.send(Message::Text(json!({"type":"session.closed","reason":"close_requested","session":{"id":"live_app_loop","model":"gpt-live-1"},"usage":{"seconds":3.5}}).to_string().into())).await.expect("最终用量");
        });
        let connection=live_voice::connect_loopback(address,live_voice::LiveConfig::default(),"fixture-key",CancellationToken::new()).await.expect("Live连接");
        let (bus,_wake)=VoiceWindowInsetEventBus::new();
        let (written, played)=async_channel::bounded(1);
        let stopped=Arc::new(AtomicBool::new(false));
        let stop_observer=stopped.clone();
        let playback=start_playback_with(move ||Box::new(SignalingSink{written,stopped}),bus.sink(TtsPlaybackSource::Doubao));
        let (input,captured)=async_channel::bounded(4);
        input.send(CaptureEvent::Ready{generation:0}).await.expect("采集就绪");
        input.send(CaptureEvent::Pcm{generation:0,chunk:PcmAudioChunk::new(PcmFormat::Pcm16,16000,1,vec![0x11;3200]).expect("100ms PCM")}).await.expect("麦克风PCM");
        let (_commands, controls)=async_channel::bounded(4);
        let (events, visible)=async_channel::bounded(16);
        let (capture_mode,_mode)=tokio::sync::watch::channel(CaptureMode{generation:0,muted:false});
        let cancellation=CancellationToken::new();
        let capture = CaptureInput {captured,capture_mode,vad:Some(voice_input::SherpaVadClient::deterministic_for_tests())};
        let worker=tokio::spawn(run_audio_loop(connection,capture,playback,controls,async_channel::bounded(1).1,events,cancellation.clone()));
        verified.await.expect("真实WS双工");
        played.recv().await.expect("PCM已真正进入sink");
        cancellation.cancel();
        let result=worker.await.expect("音频worker");
        assert!(result.error.is_none(),"正常双工和关闭不报错：{:?}",result.error);
        assert_eq!(result.usage.seconds,3.5,"保留最终服务端用量");
        assert!(result.usage.finalized,"收到closed才确认用量");
        assert!(stop_observer.load(Ordering::Acquire),"返回前播放器必须完成stop");
        let mut speakers=Vec::new();
        let mut interim_usage = None;
        let mut speech_started = false;
        while let Ok(event)=visible.try_recv(){
            match event {
                CallEvent::Transcript(part) => speakers.push(part.speaker),
                CallEvent::Usage(usage) => interim_usage = Some(usage),
                CallEvent::SpeechActivity(observation) => speech_started |= observation.events.contains(&voice_input::VadEvent::SpeechStart),
                _ => {},
            }
        }
        assert_eq!(interim_usage.expect("中间用量必须经真实WS转发到UI").seconds,1.25,"上游累计快照不应被忽略");
        assert!(speakers.contains(&live_voice::Speaker::User),"用户字幕独立存在");
        assert!(speakers.contains(&live_voice::Speaker::Assistant),"助手字幕独立存在");
        assert!(speech_started,"只有实际成功上传的非静音PCM经过共享VAD后才向宿主报告新语音");
        server.await.expect("服务端断言通过");
    }).await.expect("桌面音频集成测试有界");
}

/// AC02/AC03：已到期的上传应先于输入突发，并追赶调度卡顿积压，避免可排出的音频被误判为溢出。
/// 数据构造：可控时钟；200ms本地+1块或100ms本地+2块100ms采集，16kHz×2B/样本=32B/ms。
/// 执行过程：等待首帧→填满200ms→在20ms到期时加入新包→检查服务端解码出的真PCM→挂断。
/// 预期结果：积压与连续采集块按序完整上传，不能误判溢出、丢块或只上传首帧。
#[tokio::test]
async fn live_audio_due_tick_precedes_ready_capture_burst() {
    assert_capture_burst(200, 1).await;
    assert_capture_burst(100, 2).await;
}

async fn assert_capture_burst(initial_ms: usize, capture_blocks: usize) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("本地服务");
    let address = listener.local_addr().expect("地址");
    let (first_tx, first_rx) = tokio::sync::oneshot::channel();
    let (mic_tx, mic_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.expect("客户端");
        let mut socket = accept_async(tcp).await.expect("WS");
        let _ = socket.next().await.expect("start").expect("start完整");
        socket.send(Message::Text(json!({"type":"session.started","session":{"id":"fair-clock","model":"gpt-live-1"}}).to_string().into())).await.expect("started");
        let packet = socket.next().await.expect("首帧").expect("完整帧");
        assert!(
            packet
                .to_text()
                .expect("文本")
                .contains("session.input_audio.append"),
            "上传时钟已启动"
        );
        first_tx.send(()).expect("首帧barrier");
        let expected: Vec<u8> = [
            vec![0x33; initial_ms * 32],
            vec![0x44; capture_blocks * 3200],
        ]
        .concat();
        let mut received = Vec::new();
        // A wall-clock tick can land before the client pauses time; skip those silence frames.
        loop {
            let packet = socket.next().await.expect("真PCM").expect("完整PCM");
            let packet: Value =
                serde_json::from_str(packet.to_text().expect("文本")).expect("JSON");
            assert_eq!(
                packet["type"], "session.input_audio.append",
                "不能因为可排出的输入而提前关闭"
            );
            let bytes = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                packet["audio"].as_str().expect("PCM base64"),
            )
            .expect("上行PCM");
            if received.is_empty() && bytes.iter().all(|byte| *byte == 0) {
                continue;
            }
            received.extend(bytes);
            if received.len() >= expected.len() {
                break;
            }
        }
        assert_eq!(
            received, expected,
            "全部真实PCM须按序上传，不能丢弃后续采集块"
        );
        mic_tx.send(()).expect("PCM确认");
        while let Some(Ok(packet)) = socket.next().await {
            if packet.to_text().expect("文本").contains("session.close") {
                break;
            }
        }
        socket.send(Message::Text(json!({"type":"session.closed","reason":"close_requested","session":{"id":"fair-clock","model":"gpt-live-1"},"usage":{"seconds":0.04}}).to_string().into())).await.expect("closed");
    });
    let connection = live_voice::connect_loopback(
        address,
        live_voice::LiveConfig::default(),
        "fixture",
        CancellationToken::new(),
    )
    .await
    .expect("连接");
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
    let (input, captured) = async_channel::bounded(4);
    let capture_observer = captured.clone();
    let (_commands, controls) = async_channel::bounded(4);
    let (events, _visible) = async_channel::bounded(16);
    let (capture_mode, _mode) = tokio::sync::watch::channel(CaptureMode {
        generation: 0,
        muted: false,
    });
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
        _ = &mut worker => panic!("首帧前通话不能结束"),
        first = first_rx => first.expect("首帧完成"),
    }
    tokio::time::pause();
    input
        .send(CaptureEvent::Pcm {
            generation: 0,
            chunk: PcmAudioChunk::new(PcmFormat::Pcm16, 16000, 1, vec![0x33; initial_ms * 32])
                .expect("初始PCM"),
        })
        .await
        .expect("填满输入");
    assert!(
        futures::poll!(&mut worker).is_pending(),
        "填充PCM后等待下一tick"
    );
    assert!(capture_observer.is_empty(), "初始PCM已进入媒体循环");
    for _ in 0..capture_blocks {
        input
            .send(CaptureEvent::Pcm {
                generation: 0,
                chunk: PcmAudioChunk::new(PcmFormat::Pcm16, 16000, 1, vec![0x44; 3200])
                    .expect("100ms"),
            })
            .await
            .expect("新输入到达");
    }
    tokio::time::advance(std::time::Duration::from_millis(20)).await;
    tokio::task::yield_now().await;
    assert!(
        futures::poll!(&mut worker).is_pending(),
        "到期时排出PCM并继续通话"
    );
    tokio::time::resume();
    let received = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        tokio::select! {
            result = &mut worker => panic!("PCM确认前意外结束：{:?}", result.error),
            result = mic_rx => result,
        }
    })
    .await;
    cancellation.cancel();
    let result = worker.await;
    assert!(
        result.error.is_none(),
        "到期tick必须先腾出输入预算：{:?}",
        result.error
    );
    received
        .expect("真PCM在20ms后继续上传")
        .expect("服务端已解码确认");
    server.await.expect("服务端通过");
}

/// 单个采集块超过200ms时仍须拒绝，不能通过追赶逻辑放宽输入或网络预算。
/// 数据构造：16kHz×2B×200ms=6400B，再加一个2B样本；本地WS不连接云端。
/// 执行过程：送入6402B非零PCM，服务端区分静音保活与真PCM，再确认关闭。
/// 预期结果：保留缓冲超限错误，服务端没有收到任何真实麦克风字节。
#[tokio::test]
async fn live_audio_rejects_a_capture_block_over_200ms() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("本地服务");
    let address = listener.local_addr().expect("地址");
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.expect("客户端");
        let mut socket = accept_async(tcp).await.expect("WS");
        let _ = socket.next().await.expect("start").expect("完整start");
        socket
            .send(Message::Text(
                json!({"type":"session.started","session":{"id":"oversize","model":"gpt-live-1"}})
                    .to_string()
                    .into(),
            ))
            .await
            .expect("started");
        while let Some(Ok(packet)) = socket.next().await {
            let packet: Value =
                serde_json::from_str(packet.to_text().expect("文本")).expect("JSON");
            if packet["type"] == "session.close" {
                break;
            }
            assert_eq!(
                packet["type"], "session.input_audio.append",
                "仅允许静音保活"
            );
            let bytes = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                packet["audio"].as_str().expect("PCM"),
            )
            .expect("base64");
            assert!(
                bytes.iter().all(|byte| *byte == 0),
                "超大真PCM不能进入传输队列"
            );
        }
        socket.send(Message::Text(json!({"type":"session.closed","reason":"close_requested","session":{"id":"oversize","model":"gpt-live-1"},"usage":{"seconds":0.0}}).to_string().into())).await.expect("closed");
    });
    let connection = live_voice::connect_loopback(
        address,
        live_voice::LiveConfig::default(),
        "fixture",
        CancellationToken::new(),
    )
    .await
    .expect("连接");
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
    let (input, captured) = async_channel::bounded(1);
    input
        .send(CaptureEvent::Pcm {
            generation: 0,
            chunk: PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0x33; 6402])
                .expect("超大合法PCM"),
        })
        .await
        .expect("输入");
    let (_commands, controls) = async_channel::bounded(1);
    let (events, _visible) = async_channel::bounded(16);
    let (capture_mode, _mode) = tokio::sync::watch::channel(CaptureMode {
        generation: 0,
        muted: false,
    });
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        run_audio_loop(
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
            CancellationToken::new(),
        ),
    )
    .await
    .expect("超限关闭有界");
    assert_eq!(
        result.error.as_deref(),
        Some("Microphone audio exceeded the 200ms buffer"),
        "保留单块输入上限"
    );
    server.await.expect("服务端未收到超大真PCM");
}
