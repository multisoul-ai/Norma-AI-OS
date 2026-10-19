use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};

pub async fn ready(listener: TcpListener, id: &str) -> WebSocketStream<TcpStream> {
    let (tcp, _) = listener.accept().await.expect("应接受本地测试连接");
    let mut socket = accept_async(tcp).await.expect("WS握手应成功");
    let start = socket
        .next()
        .await
        .expect("应收到启动消息")
        .expect("启动消息应完整");
    let event: Value =
        serde_json::from_str(start.to_text().expect("启动应为文本")).expect("启动应为JSON");
    assert_eq!(event["type"], "session.start", "必须先启动Live会话");
    socket
        .send(Message::Text(
            json!({"type":"session.started","session":{"id":id,"model":"gpt-live-1"}})
                .to_string()
                .into(),
        ))
        .await
        .expect("应发送ready");
    socket
}

pub async fn finish(socket: &mut WebSocketStream<TcpStream>, id: &str) {
    let message = socket
        .next()
        .await
        .expect("应优雅关闭")
        .expect("应收到有效关闭消息");
    let event: Value =
        serde_json::from_str(message.to_text().expect("关闭应为文本")).expect("关闭应为JSON");
    assert_eq!(event["type"], "session.close", "取消应优先于尚未发送的输入");
    socket.send(Message::Text(json!({"type":"session.closed","reason":"close_requested","session":{"id":id,"model":"gpt-live-1"},"usage":{"seconds":0.5}}).to_string().into()))
        .await.expect("应发送最终关闭确认");
}
