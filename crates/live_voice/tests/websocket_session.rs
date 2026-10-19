use audio::{PcmAudioChunk, PcmFormat};
use futures::{SinkExt, StreamExt};
use live_voice::{connect_loopback, LiveConfig, LiveEvent};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        handshake::server::{Request, Response},
        Message,
    },
};
use tokio_util::sync::CancellationToken;

/// AC01/AC02/AC11：真实TCP与WS边界交错输入、输出，并等到closed才确认结束。
///
/// 数据构造：使用127.0.0.1随机端口与假Key；两段PCM分别是样本[1,2]、[3,4]。
/// 执行过程：假上游验证鉴权与start，发送ready和音频；客户端上传输入并挂断。
/// 预期结果：输出期间仍收到麦克风PCM，客户端消费在途包及最终16.5秒用量。
#[tokio::test]
async fn live_websocket_streams_both_directions_and_finalizes() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("应绑定本地假上游");
        let address = listener.local_addr().expect("应有本地地址");
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.expect("应接受客户端");
            let mut socket = accept_hdr_async(tcp, |request: &Request, response: Response| {
                assert_eq!(request.uri().path(), "/v1/live/sessions", "使用Live会话路由");
                assert!(request.uri().query().is_none(), "模型不能进入URL查询参数");
                assert_eq!(request.headers()["authorization"], "Bearer fixture-key", "假Key应仅进入认证头");
                Ok(response)
            }).await.expect("应完成WS握手");
            let start = socket.next().await.expect("应有首包").expect("首包不能报错");
            let start: Value = serde_json::from_str(start.to_text().expect("Live首包应为文本"))
                .expect("首包应为JSON");
            assert_eq!(start["type"], "session.start", "首包是session.start");
            assert_eq!(start["session"]["model"], "gpt-live-1", "不能伪装为Realtime");
            for event in [
                json!({"type":"session.started","session":{"id":"live_socket","model":"gpt-live-1"}}),
                json!({"type":"session.output_audio.delta","delta":"AQACAA=="}),
            ] {
                socket.send(Message::Text(event.to_string().into())).await.expect("应发送ready和音频");
            }
            let input = socket.next().await.expect("输出期间应收到麦克风").expect("输入不得报错");
            let input: Value = serde_json::from_str(input.to_text().expect("音频走JSON文本"))
                .expect("输入应有效");
            assert_eq!(input["type"], "session.input_audio.append", "持续上行是Live append");
            assert_eq!(input["audio"], "AQACAA==", "上行保留样本");
            socket.send(Message::Text(json!({"type":"session.output_audio.delta","delta":"AwAEAA=="}).to_string().into()))
                .await.expect("应发送第二段输出");
            let close = socket.next().await.expect("应收到关闭命令").expect("关闭命令不得报错");
            let close: Value = serde_json::from_str(close.to_text().expect("关闭为JSON文本"))
                .expect("关闭事件有效");
            assert_eq!(close["type"], "session.close", "不能直接切断socket代替正常close");
            for event in [
                json!({"type":"session.output_audio.delta","delta":"BQAGAA=="}),
                json!({"type":"session.closed","reason":"close_requested","session":{"id":"live_socket","model":"gpt-live-1"},"usage":{"seconds":16.5}}),
            ] {
                socket.send(Message::Text(event.to_string().into())).await.expect("应排空到closed");
            }
        });
        let mut client = connect_loopback(address, LiveConfig::default(), "fixture-key", CancellationToken::new())
            .await.expect("应完成Live连接及ready屏障");
        let pcm = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![1,0,2,0]).expect("输入PCM有效");
        client.send_audio(pcm.clone()).expect("应在处理输出时继续上传麦克风");
        assert_eq!(client.next_event().await, Some(LiveEvent::OutputAudio(pcm)), "第一段输出进入客户端");
        let second = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![3,0,4,0]).expect("输出PCM有效");
        assert_eq!(client.next_event().await, Some(LiveEvent::OutputAudio(second)), "输入不会暂停输出接收");
        let outcome = client.finish().await;
        assert_eq!(outcome.usage.seconds, 16.5, "保留服务端最终小数秒用量");
        assert!(outcome.usage.finalized, "收到closed才能确认最终用量");
        assert!(outcome.error.is_none(), "正常双工及关闭不应报错");
        server.await.expect("假上游的协议断言应全部通过");
    }).await.expect("整个协议测试必须有界结束");
}
