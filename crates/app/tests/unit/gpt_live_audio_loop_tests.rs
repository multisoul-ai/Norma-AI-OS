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

/// AC02/AC03：已到期的20ms上传应先于普通输入突发，避免可排出的音频被误判为溢出。
/// 数据构造：可控Tokio时钟；首个静音帧后放入200ms真PCM，下一tick同刻再到20ms。
/// 执行过程：等待首帧→填满200ms→在20ms到期时加入新包→检查服务端解码出的真PCM→挂断。
/// 预期结果：先发送20ms腾出预算，再接收新包；无溢出；服务端收到0x33而非只有静音。
#[tokio::test]
async fn live_audio_due_tick_precedes_ready_capture_burst() {
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
        let packet = socket.next().await.expect("真PCM").expect("完整PCM");
        let packet: Value = serde_json::from_str(packet.to_text().expect("文本")).expect("JSON");
        assert_eq!(
            packet["type"], "session.input_audio.append",
            "不能因为可排出的输入而提前关闭"
        );
        let bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            packet["audio"].as_str().expect("PCM base64"),
        )
        .expect("上行PCM");
        assert_eq!(bytes, vec![0x33; 640], "下一到期帧是真实麦克风PCM");
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
            chunk: PcmAudioChunk::new(PcmFormat::Pcm16, 16000, 1, vec![0x33; 6400]).expect("200ms"),
        })
        .await
        .expect("填满输入");
    assert!(
        futures::poll!(&mut worker).is_pending(),
        "填充PCM后等待下一tick"
    );
    assert!(capture_observer.is_empty(), "200ms已进入媒体循环");
    input
        .send(CaptureEvent::Pcm {
            generation: 0,
            chunk: PcmAudioChunk::new(PcmFormat::Pcm16, 16000, 1, vec![0x44; 640]).expect("20ms"),
        })
        .await
        .expect("新输入到达");
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
