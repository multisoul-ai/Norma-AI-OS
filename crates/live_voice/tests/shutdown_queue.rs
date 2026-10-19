#![cfg(feature = "test-support")]

use futures::{SinkExt, StreamExt};
use live_voice::{connect_loopback, LiveConfig};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;

/// AC03/AC11：应用停止消费已满的事件队列后，正常挂断仍须接收最终用量。
///
/// 数据构造：64个转写填满64项事件队列，close后先发usage再发closed。
/// 执行过程：观测队列实际达到64→finish不再消费事件→等待服务端终态。
/// 预期结果：没有Capacity假失败，finalized=true，最终用量为0.5秒。
#[tokio::test]
async fn full_application_queue_cannot_prevent_graceful_finalization() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("本地服务");
        let address = listener.local_addr().expect("本地地址");
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.expect("本地连接");
            let mut socket = accept_async(tcp).await.expect("WS握手");
            let start = socket.next().await.expect("start").expect("start完整");
            let start: Value = serde_json::from_str(start.to_text().expect("start文本")).expect("start JSON");
            assert_eq!(start["type"], "session.start", "先发送启动消息");
            socket.send(Message::Text(json!({"type":"session.started","session":{"id":"live_full_queue","model":"gpt-live-1"}}).to_string().into())).await.expect("ready");
            // 64 是公开事件队列的上限，不能超出后才测试正常挂断。
            for index in 0..64 {
                socket.send(Message::Text(json!({"type":"session.input_transcript.delta","event_id":format!("u{index}"),"delta":"进度","start_ms":index,"end_ms":index+1}).to_string().into())).await.expect("发送转写");
            }
            let message = socket.next().await.expect("收到close").expect("close有效");
            let close: Value = serde_json::from_str(message.to_text().expect("close文本")).expect("close JSON");
            assert_eq!(close["type"], "session.close", "应尝试优雅挂断");
            socket.send(Message::Text(json!({"type":"session.usage.updated","usage":{"seconds":0.25}}).to_string().into())).await.expect("中间用量");
            socket.send(Message::Text(json!({"type":"session.closed","reason":"close_requested","session":{"id":"live_full_queue"},"usage":{"seconds":0.5}}).to_string().into())).await.expect("最终用量");
        });
        let client = connect_loopback(address, LiveConfig::default(), "fixture-key", CancellationToken::new()).await.expect("连接ready");
        while client.queued_events() < 64 { tokio::task::yield_now().await; }
        assert_eq!(client.queued_events(), 64, "挂断前必须已实际填满队列");
        let outcome = client.finish().await;
        assert!(outcome.error.is_none(), "正常关闭不能依赖应用继续排空事件：{:?}", outcome.error);
        assert!(outcome.usage.finalized, "必须接收到closed而非停在中间usage");
        assert_eq!(outcome.usage.seconds, 0.5, "保留最终累计秒数");
        server.await.expect("服务端完整退出");
    }).await.expect("满队列挂断测试必须有界");
}
