#![cfg(feature = "test-support")]
use futures::{SinkExt, StreamExt};
use live_voice::{connect_loopback, LiveConfig, LiveEvent, Speaker};
use serde_json::json;
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;

/// AC06：恢复播放必须丢弃传输队列中旧PCM，且保留字幕、用量与后续新PCM。
/// 构造：真实本地WS积压20ms旧PCM、字幕及usage，再在恢复后发一个不同PCM帧。
/// 执行：不消费事件直到排队完成，停止→恢复→消费；用两端barrier控制时序。
/// 预期：旧音频不复活，字幕/用量不丢，新PCM正常返回；控制不发送虚构的取消协议。
#[tokio::test]
async fn output_epoch_discards_queued_audio_across_stop_and_resume() {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("监听");
        let address = listener.local_addr().expect("地址");
        let (release, resumed) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.expect("客户端");
            let mut ws = accept_async(tcp).await.expect("WS");
            let _ = ws.next().await.expect("start").expect("完整");
            for event in [
                json!({"type":"session.started","session":{"id":"output-gate","model":"gpt-live-1"}}),
                json!({"type":"session.output_audio.delta","delta":base64::Engine::encode(&base64::engine::general_purpose::STANDARD,vec![0x11_u8;640])}),
                json!({"type":"session.output_transcript.delta","event_id":"a1","delta":"旧输出字幕","start_ms":0,"end_ms":20}),
                json!({"type":"session.usage.updated","usage":{"seconds":1.0}}),
            ] {ws.send(Message::Text(event.to_string().into())).await.expect("排队事件");}
            resumed.await.expect("恢复barrier");
            ws.send(Message::Text(json!({"type":"session.output_audio.delta","delta":base64::Engine::encode(&base64::engine::general_purpose::STANDARD,vec![0x22_u8;640])}).to_string().into())).await.expect("新PCM");
            while let Some(Ok(event)) = ws.next().await {if event.to_text().expect("文本").contains("session.close") {break;}}
            ws.send(Message::Text(json!({"type":"session.closed","reason":"close_requested","session":{"id":"output-gate","model":"gpt-live-1"},"usage":{"seconds":2.0}}).to_string().into())).await.expect("关闭");
        });
        let mut connection = connect_loopback(address, LiveConfig::default(), "fixture", CancellationToken::new()).await.expect("连接");
        while connection.queued_events() < 3 {tokio::task::yield_now().await;}
        connection.control().set_output_blocked(true);
        connection.control().set_output_blocked(false);
        assert!(matches!(connection.next_event().await,Some(LiveEvent::Transcript(part)) if part.speaker == Speaker::Assistant && part.delta == "旧输出字幕"),"跳过旧PCM但保留字幕");
        assert!(matches!(connection.next_event().await,Some(LiveEvent::Usage(usage)) if usage.seconds == 1.0),"不丢失用量");
        release.send(()).expect("恢复后发新PCM");
        assert!(matches!(connection.next_event().await,Some(LiveEvent::OutputAudio(pcm)) if pcm.bytes == vec![0x22;640]),"只允许恢复后新接收PCM");
        assert!(connection.finish().await.error.is_none(),"正常关闭");
        server.await.expect("服务端断言");
    }).await.expect("测试有界");
}
