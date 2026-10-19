use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn serve(responses: Vec<Value>) -> (String, tokio::task::JoinHandle<Vec<Value>>) {
    serve_http(responses, false, false).await
}

pub async fn serve_ollama(responses: Vec<Value>) -> (String, tokio::task::JoinHandle<Vec<Value>>) {
    serve_http(responses, true, false).await
}

pub async fn serve_openai(responses: Vec<Value>) -> (String, tokio::task::JoinHandle<Vec<Value>>) {
    serve_http(responses, false, true).await
}

async fn serve_http(
    responses: Vec<Value>,
    ollama: bool,
    validate_openai: bool,
) -> (String, tokio::task::JoinHandle<Vec<Value>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("回环SSE服务");
    let endpoint = format!("http://{}/v1", listener.local_addr().expect("SSE端口"));
    let task = tokio::spawn(async move {
        // 5秒为单个回环HTTP交换的期限，避免失败测试挂住整个测试进程。
        tokio::time::timeout(std::time::Duration::from_secs(5), async move {
            let mut requests: Vec<Value> = Vec::new();
            for delta in responses {
                let (mut socket, _) = listener.accept().await.expect("收到模型请求");
                let mut data = Vec::new();
                let header_end = loop {
                    let mut bytes = [0u8; 4096];
                    let count = socket.read(&mut bytes).await.expect("读取请求");
                    assert!(count > 0, "HTTP头不能提前关闭");
                    data.extend_from_slice(&bytes[..count]);
                    if let Some(end) = data.windows(4).position(|part| part == b"\r\n\r\n") { break end+4; }
                };
                let length: usize = String::from_utf8_lossy(&data[..header_end]).lines().find_map(|line| {
                    line.to_ascii_lowercase().strip_prefix("content-length:").map(|value| value.trim().parse().expect("正文长度"))
                }).expect("请求必须有长度");
                while data.len() < header_end + length {
                    let mut bytes = [0u8; 4096];
                    let count = socket.read(&mut bytes).await.expect("读取正文");
                    assert!(count > 0, "正文不能提前关闭");
                    data.extend_from_slice(&bytes[..count]);
                }
                requests.push(serde_json::from_slice(&data[header_end..header_end+length]).expect("请求JSON"));
                if validate_openai {
                    let tools = requests.last().expect("实际请求")["tools"].as_array().expect("工具列表");
                    let invalid = tools.iter().find(|tool| {
                        let parameters = &tool["function"]["parameters"];
                        parameters["type"] != "object" || ["oneOf", "anyOf", "allOf", "enum", "const", "not"]
                            .iter().any(|keyword| parameters.get(*keyword).is_some())
                    });
                    if let Some(tool) = invalid {
                        let body = json!({"error": {
                            "message": format!("Invalid schema for function '{}': unsupported root schema", tool["function"]["name"].as_str().expect("工具名")),
                            "type": "invalid_request_error", "code": "invalid_function_parameters"
                        }}).to_string();
                        socket.write_all(format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.expect("复现官方400");
                        return requests;
                    }
                }
                let finish = if delta.get("tool_calls").is_some() { "tool_calls" } else { "stop" };
                let chunk = json!({"id":"fixture","object":"chat.completion.chunk","created":1,"model":"live-backend-fixture","choices":[{"index":0,"delta":delta,"finish_reason":null}]});
                let stop = json!({"id":"fixture","object":"chat.completion.chunk","created":1,"model":"live-backend-fixture","choices":[{"index":0,"delta":{},"finish_reason":finish}]});
                let body = if ollama {
                    format!("{}\n", json!({"model":"fixture","created_at":"2026-10-18T00:00:00Z",
                        "message":delta,"done":true,"done_reason":"stop"}))
                } else { format!("data: {chunk}\n\ndata: {stop}\n\ndata: [DONE]\n\n") };
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.expect("响应SSE");
            }
            requests
        }).await.expect("SSE夹具不能超时")
    });
    (endpoint, task)
}
