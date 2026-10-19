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

/// AC03：消费者不取音频时，下行积压超过1秒必须显式失败，不能无界排队。
///
/// 数据构造：假上游连续发两个32000字节PCM块，总2秒，超过1秒预算。
/// 执行过程：连接后不调用next_event，等待driver自己因溢出结束，再读取结束结果。
/// 预期结果：Capacity错误可见，usage未最终确认；没有等待消费者造成无限挂起。
#[tokio::test]
async fn slow_audio_consumer_fails_at_duration_budget() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("应绑定");
        let address = listener.local_addr().expect("应有端口");
        let cancellation = CancellationToken::new();
        let peer_done = cancellation.clone();
        let server = tokio::spawn(async move {
            let mut socket = live_server::ready(listener, "live_slow").await;
            let event = Message::Text(json!({"type":"session.output_audio.delta","delta":STANDARD.encode(vec![0; 32_000])}).to_string().into());
            socket.send(event.clone()).await.expect("第一秒应送达");
            socket.send(event).await.expect("第二秒应送达");
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
