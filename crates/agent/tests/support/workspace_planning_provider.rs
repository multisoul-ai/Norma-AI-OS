use agent::{ProviderConfig, ProviderProtocol};
use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
};

pub struct PlanningProvider {
    pub config: ProviderConfig,
    pub requests: Arc<Mutex<Vec<serde_json::Value>>>,
    pub server: std::thread::JoinHandle<()>,
    pub release: Option<std::sync::mpsc::Sender<()>>,
}

pub fn provider(native: bool, content: &str, finish: &str, status: u16) -> PlanningProvider {
    build_provider(native, content, finish, status, None)
}

pub fn pending_provider() -> PlanningProvider {
    let (release, wait) = std::sync::mpsc::channel();
    let mut provider = build_provider(false, "", "stop", 200, Some(wait));
    provider.release = Some(release);
    provider
}

fn build_provider(
    native: bool,
    content: &str,
    finish: &str,
    status: u16,
    wait: Option<std::sync::mpsc::Receiver<()>>,
) -> PlanningProvider {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("回环Provider");
    let address = listener.local_addr().expect("端口");
    listener.set_nonblocking(true).expect("有界accept");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    let content = content.to_owned();
    let finish = finish.to_owned();
    let server = std::thread::spawn(move || {
        let start = std::time::Instant::now();
        let (mut socket, _) = loop {
            match listener.accept() {
                Ok(connection) => break connection,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && start.elapsed() < std::time::Duration::from_secs(5) =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(5))
                }
                Err(error) => panic!("5秒内必须有完整规划请求：{error}"),
            }
        };
        socket.set_nonblocking(false).expect("响应连接恢复阻塞读取");
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .expect("有界读取");
        let mut data = Vec::new();
        let header_end = loop {
            let mut buffer = [0; 4096];
            let n = socket.read(&mut buffer).expect("读取请求");
            assert!(n > 0, "完整头部之前不可EOF");
            data.extend_from_slice(&buffer[..n]);
            if let Some(end) = data.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                break end + 4;
            }
        };
        let length: usize = String::from_utf8_lossy(&data[..header_end])
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(|value| value.trim().parse().expect("长度"))
            })
            .expect("Content-Length");
        assert!(length > 0 && length < 128 * 1024, "请求必须完整且有界");
        while data.len() < header_end + length {
            let mut buffer = [0; 4096];
            let n = socket.read(&mut buffer).expect("读取正文");
            assert!(n > 0, "完整正文之前不可EOF");
            data.extend_from_slice(&buffer[..n]);
        }
        let request: serde_json::Value =
            serde_json::from_slice(&data[header_end..header_end + length]).expect("请求JSON");
        let streamed = request["stream"].as_bool() == Some(true);
        captured.lock().expect("记录").push(request);
        let pending = wait.is_some();
        if let Some(wait) = wait {
            let _ = wait.recv_timeout(std::time::Duration::from_secs(25));
        }
        let body = if streamed && !native {
            let chunk = serde_json::json!({"id":"fixture","object":"chat.completion.chunk","created":1,"model":"workspace-fixture","choices":[{"index":0,"delta":{"role":"assistant","content":content},"finish_reason":null}]});
            let stop = serde_json::json!({"id":"fixture","object":"chat.completion.chunk","created":1,"model":"workspace-fixture","choices":[{"index":0,"delta":{},"finish_reason":finish}]});
            format!("data: {chunk}\n\ndata: {stop}\n\ndata: [DONE]\n\n")
        } else {
            (if native {
            serde_json::json!({"model":"workspace-fixture","created_at":"2026-10-11T00:00:00Z","message":{"role":"assistant","content":content},"done":true,"done_reason":finish})
        } else if status == 200 {
            serde_json::json!({"id":"fixture","object":"chat.completion","created":1,"model":"workspace-fixture","choices":[{"index":0,"message":{"role":"assistant","content":content},"finish_reason":finish}]})
        } else {
            serde_json::json!({"error":{"message":"fixture failed","type":"invalid_request_error"}})
        }).to_string()
        };
        let content_type = if streamed {
            "text/event-stream"
        } else {
            "application/json"
        };
        let sent = write!(socket,"HTTP/1.1 {status} Fixture\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
        if !pending {
            sent.expect("完整响应");
        }
    });
    PlanningProvider {
        config: ProviderConfig::new(
            if native {
                ProviderProtocol::Ollama
            } else {
                ProviderProtocol::OpenAiCompatible
            },
            if native {
                format!("http://{address}")
            } else {
                format!("http://{address}/v1")
            },
            "workspace-fixture",
            None,
        ),
        requests,
        server,
        release: None,
    }
}
