use serde_json::Value;
use std::os::unix::net::UnixStream;
use std::time::Duration;
use tungstenite::{Message, WebSocket};

pub fn read(socket: &mut WebSocket<UnixStream>) -> Value {
    loop {
        if let Message::Text(text) = socket.read().expect("应收到协议帧") {
            return serde_json::from_str(&text).expect("应为JSON");
        }
    }
}
pub fn send(socket: &mut WebSocket<UnixStream>, value: Value) {
    socket
        .send(Message::Text(value.to_string().into()))
        .expect("应写出协议帧");
}
pub fn timed(stream: UnixStream) -> UnixStream {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
}
