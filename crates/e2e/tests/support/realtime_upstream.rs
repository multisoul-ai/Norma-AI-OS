use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tungstenite::Message;

pub fn attach(listener: &std::net::TcpListener) -> tungstenite::WebSocket<std::net::TcpStream> {
    listener.set_nonblocking(true).expect("bounded accept");
    let deadline = Instant::now() + Duration::from_secs(5);
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "Mac没有连接本次隔离sideband上游");
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => panic!("sideband accept: {error}"),
        }
    };
    stream
        .set_nonblocking(false)
        .expect("blocking websocket stream");
    // 5s = 3s ordinary wire exchange + 2s mobile event coalescing margin.
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("有界失败");
    let mut socket = tungstenite::accept(stream).expect("上游 websocket");
    send(
        &mut socket,
        json!({"type":"session.created","session":{"type":"realtime","instructions":"用中文交谈",
        "tools":[{"type":"function","name":"set_behavior","parameters":{"type":"object"}}]}}),
    );
    let update = read(&mut socket);
    assert!(
        update["session"]["tools"]
            .as_array()
            .is_some_and(|tools| tools.iter().any(|tool| tool["name"] == "create_agent")),
        "必须向同一媒体会话注册现有任务工具"
    );
    assert!(
        update["session"]["tools"]
            .as_array()
            .is_some_and(|tools| tools.iter().any(|tool| tool["name"] == "set_behavior")),
        "Mac 注册不得丢弃手机角色工具"
    );
    send(
        &mut socket,
        json!({"type":"session.updated","session":update["session"]}),
    );
    socket
}

pub fn send(socket: &mut tungstenite::WebSocket<std::net::TcpStream>, value: Value) {
    socket
        .send(Message::text(value.to_string()))
        .expect("上游协议事件");
}
pub fn read(socket: &mut tungstenite::WebSocket<std::net::TcpStream>) -> Value {
    serde_json::from_str(
        socket
            .read()
            .expect("实时协议响应")
            .to_text()
            .expect("文本"),
    )
    .expect("JSON")
}
pub fn turn(socket: &mut tungstenite::WebSocket<std::net::TcpStream>, id: &str, text: &str) {
    send(
        socket,
        json!({"type":"input_audio_buffer.speech_started","item_id":id}),
    );
    send(
        socket,
        json!({"type":"input_audio_buffer.speech_stopped","item_id":id}),
    );
    send(
        socket,
        json!({"type":"conversation.item.input_audio_transcription.completed","item_id":id,"transcript":text}),
    );
}
pub fn call(
    socket: &mut tungstenite::WebSocket<std::net::TcpStream>,
    response: &str,
    call: &str,
    task: &str,
) {
    send(
        socket,
        json!({"type":"response.created","response":{"id":response}}),
    );
    send(
        socket,
        json!({"type":"response.done","response":{"id":response,"status":"completed","output":[{
        "type":"function_call","call_id":call,"name":"create_agent","arguments":json!({"profile":"codex","task":task}).to_string()}]}}),
    );
}
pub fn output(socket: &mut tungstenite::WebSocket<std::net::TcpStream>, call: &str) -> Value {
    let event = read(socket);
    assert_eq!(
        event["type"], "conversation.item.create",
        "工具结果必须写回同一会话"
    );
    assert_eq!(
        event["item"]["call_id"], call,
        "工具回执必须使用 function call_id"
    );
    serde_json::from_str(event["item"]["output"].as_str().expect("函数输出")).expect("结果")
}
pub fn continuation(socket: &mut tungstenite::WebSocket<std::net::TcpStream>) {
    assert_eq!(
        read(socket)["type"],
        "response.create",
        "每组工具完成后只继续一次回复"
    );
}
