#![cfg(feature = "test-support")]

#[path = "support/live_server.rs"]
mod live_server;

use audio::{PcmAudioChunk, PcmFormat};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use futures::SinkExt;
use live_voice::{connect_loopback, LiveConfig, LiveError};
use serde_json::json;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

/// AC03/AC11：输入积压不能超过1秒；关闭须抢在未发送的音频前。
///
/// 数据构造：1秒16k PCM16是32000字节，再加640字节20ms应超限。
/// 执行过程：单线程runtime里在让出执行前入队两包，再立即关闭并核对WS首包。
/// 预期结果：第二包拒绝、输入预算有界；关闭后释放队列，不发送陈旧PCM。
#[tokio::test]
async fn microphone_budget_is_bounded_and_close_discards_queued_input() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("应绑定");
        let address = listener.local_addr().expect("应有端口");
        let server = tokio::spawn(async move {
            let mut socket = live_server::ready(listener, "live_budget").await;
            live_server::finish(&mut socket, "live_budget").await;
        });
        let client = connect_loopback(
            address,
            LiveConfig::default(),
            "fixture-key",
            CancellationToken::new(),
        )
        .await
        .expect("应ready");
        let control = client.control();
        client
            .send_audio(
                PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0; 32_000])
                    .expect("1秒PCM应有效"),
            )
            .expect("恰好1秒可入队");
        let extra =
            PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0; 640]).expect("20ms PCM有效");
        assert_eq!(
            client.send_audio(extra.clone()),
            Err(LiveError::Capacity),
            "不能因包数尚少而突破时长预算"
        );
        assert_eq!(
            control.queued_audio_bytes(),
            32_000,
            "排队与发送中音频总量不超过1秒"
        );
        let outcome = client.finish().await;
        assert!(outcome.error.is_none(), "主动关闭应正常完成");
        assert!(outcome.usage.finalized, "应收到closed");
        assert_eq!(control.queued_audio_bytes(), 0, "结束后应释放所有输入预算");
        assert_eq!(
            control.send_audio(extra),
            Err(LiveError::NotReady),
            "迟到输入不得重新进入旧会话"
        );
        server.await.expect("假上游应只看到close而没有陈旧音频");
    })
    .await
    .expect("关闭测试必须有界");
}

/// 下行语音是有损实时流：消费者落后时丢弃音频，不能终止通话。
///
/// 数据构造：假上游连续发两个32000字节PCM块，总2秒，超过1秒预算；消费者不取事件。
/// 执行过程：等待driver丢弃超限音频，再主动关闭。
/// 预期结果：恰好丢弃第二秒音频，通话正常结束并确认用量。
#[tokio::test]
async fn slow_audio_consumer_drops_audio_without_ending_the_call() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("应绑定");
        let address = listener.local_addr().expect("应有端口");
        let server = tokio::spawn(async move {
            let mut socket = live_server::ready(listener, "live_slow").await;
            let event = Message::Text(json!({"type":"session.output_audio.delta","delta":STANDARD.encode(vec![0; 32_000])}).to_string().into());
            socket.send(event.clone()).await.expect("第一秒应送达");
            socket.send(event).await.expect("第二秒应送达");
            live_server::finish(&mut socket, "live_slow").await;
        });
        let client = connect_loopback(address, LiveConfig::default(), "fixture-key", CancellationToken::new())
            .await.expect("应ready");
        let control = client.control();
        while control.dropped_output_audio_bytes() < 32_000 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(control.dropped_output_audio_bytes(), 32_000, "只丢超出预算的第二秒");
        let outcome = client.finish().await;
        assert!(outcome.error.is_none(), "丢音频不能终止通话");
        assert!(outcome.usage.finalized, "应收到closed");
        server.await.expect("假上游应退出");
    }).await.expect("背压测试必须有界");
}

/// 语音突发不能挤掉控制事件：队列须给转写、确认等控制事件留槽。
///
/// 数据构造：60个20ms音频块后紧跟一条转写；消费者先不取事件。
/// 执行过程：等driver丢弃多余音频，再逐个取事件直到转写出现。
/// 预期结果：有音频被丢弃、通话存活，且转写事件仍能送达。
#[tokio::test]
async fn audio_burst_never_starves_control_events() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("应绑定");
        let address = listener.local_addr().expect("应有端口");
        let server = tokio::spawn(async move {
            let mut socket = live_server::ready(listener, "live_burst").await;
            let audio = Message::Text(json!({"type":"session.output_audio.delta","delta":STANDARD.encode(vec![0; 640])}).to_string().into());
            for _ in 0..60 { socket.send(audio.clone()).await.expect("音频应送达"); }
            socket.send(Message::Text(json!({"type":"session.output_transcript.delta","event_id":"a1","delta":"好","start_ms":0,"end_ms":10}).to_string().into()))
                .await.expect("转写应送达");
            live_server::finish(&mut socket, "live_burst").await;
        });
        let mut client = connect_loopback(address, LiveConfig::default(), "fixture-key", CancellationToken::new())
            .await.expect("应ready");
        let control = client.control();
        while control.dropped_output_audio_bytes() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        loop {
            match client.next_event().await.expect("通话不应因突发语音结束") {
                live_voice::LiveEvent::Transcript(_) => break,
                _ => continue,
            }
        }
        let outcome = client.finish().await;
        assert!(outcome.error.is_none(), "突发语音不能终止通话");
        server.await.expect("假上游应退出");
    }).await.expect("背压测试必须有界");
}

/// 控制事件队列溢出说明消费者已失效，仍须显式失败，不能被吞成正常结束。
///
/// 数据构造：假上游连发80条转写，超过64槽队列；消费者不取事件。
/// 预期结果：Capacity错误可见，usage未最终确认。
#[tokio::test]
async fn control_event_overflow_still_fails_the_call() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("应绑定");
        let address = listener.local_addr().expect("应有端口");
        let cancellation = CancellationToken::new();
        let peer_done = cancellation.clone();
        let server = tokio::spawn(async move {
            let mut socket = live_server::ready(listener, "live_control").await;
            for i in 0..80 {
                socket.send(Message::Text(json!({"type":"session.output_transcript.delta","event_id":format!("a{i}"),"delta":"x","start_ms":i * 10,"end_ms":i * 10 + 5}).to_string().into()))
                    .await.expect("转写应送达");
            }
            peer_done.cancelled().await;
        });
        let client = connect_loopback(address, LiveConfig::default(), "fixture-key", cancellation.clone())
            .await.expect("应ready");
        tokio::time::timeout(Duration::from_secs(3), cancellation.cancelled()).await
            .expect("driver必须自行拒绝超限，而非一直阻塞接收");
        let outcome = client.finish().await;
        assert_eq!(outcome.error, Some(LiveError::Capacity), "溢出不能被吞成正常结束");
        assert!(!outcome.usage.finalized, "没有closed不能确认最终计费用量");
        server.await.expect("假上游应退出");
    }).await.expect("背压测试必须有界");
}
