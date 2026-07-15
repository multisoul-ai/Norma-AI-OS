use super::*;
use crate::{card_tool_specs, ChatMessage, ChatRequest, Role};
use serde_json::Value;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

fn start_fake_ollama(
    handler: impl Fn(&str, &str, &str) -> String + Send + 'static,
) -> (String, Arc<Mutex<Vec<String>>>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("fake Ollama listener 应绑定本地端口");
    let endpoint = format!(
        "http://{}",
        listener
            .local_addr()
            .expect("fake Ollama listener 应能读取本地地址")
    );
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_thread = Arc::clone(&requests);
    let handle = thread::spawn(move || loop {
        let (mut stream, _) = listener.accept().expect("fake Ollama 应接受 LLM 请求");
        let request = read_http_request(&mut stream);
        let (method, path, body) = split_http_request(&request);
        requests_for_thread
            .lock()
            .expect("fake Ollama request lock 不应 poisoned")
            .push(request.clone());
        let response = handler(method, path, body);
        write_json_response(&mut stream, &response);
        if path == "/api/chat" {
            break;
        }
    });

    (endpoint, requests, handle)
}

fn start_fake_ollama_tags_only(body: &'static str) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("fake Ollama listener 应绑定本地端口");
    let endpoint = format!(
        "http://{}",
        listener
            .local_addr()
            .expect("fake Ollama listener 应能读取本地地址")
    );
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener
            .accept()
            .expect("fake Ollama 应接受 readiness 请求");
        let _request = read_http_request(&mut stream);
        write_json_response(&mut stream, body);
    });
    (endpoint, handle)
}

fn read_http_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("fake Ollama stream 应能设置读取超时");
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 1024];

    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                bytes.extend_from_slice(&buffer[..count]);
                if http_request_complete(&bytes) {
                    break;
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                break;
            }
            Err(error) => panic!("fake Ollama 读取 HTTP 请求失败: {error}"),
        }
    }

    String::from_utf8(bytes).expect("HTTP request 应是 UTF-8")
}

fn http_request_complete(bytes: &[u8]) -> bool {
    let request = String::from_utf8_lossy(bytes);
    let Some(header_end) = request.find("\r\n\r\n") else {
        return false;
    };
    let content_length = request
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    bytes.len().saturating_sub(header_end + 4) >= content_length
}

fn split_http_request(request: &str) -> (&str, &str, &str) {
    let request_line = request.lines().next().expect("HTTP request 应包含请求行");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().expect("HTTP request 应包含 method");
    let path = parts.next().expect("HTTP request 应包含 path");
    let body = request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or("");
    (method, path, body)
}

fn write_json_response(stream: &mut TcpStream, body: &str) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .expect("fake Ollama 应能写 HTTP response");
}

fn request_body(requests: &Arc<Mutex<Vec<String>>>, index: usize) -> String {
    let requests = requests
        .lock()
        .expect("fake Ollama request lock 不应 poisoned");
    requests[index]
        .split_once("\r\n\r\n")
        .expect("HTTP request 应包含 header/body 分隔符")
        .1
        .to_string()
}

/// OL-1: probe_ollama_ready 在 /api/tags 返回 models 时应判定为可用。
///
/// 数据构造（含关键数值的推导过程）：
///   fake Ollama 仅响应一次 GET /api/tags
///   body.models = 1 个 qwen3.5:0.8b 条目
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动本地 TCP listener 返回 tags JSON
///   2. 调用 probe_ollama_ready(endpoint)
///
/// 预期结果：
///   - 正断言：返回 Ok(())
///   - 负断言：不应返回 Message 错误
#[test]
fn test_probe_ollama_ready_accepts_tags_response() {
    let (endpoint, handle) =
        start_fake_ollama_tags_only(r#"{"models":[{"name":"qwen3.5:0.8b","modified_at":"now"}]}"#);

    probe_ollama_ready(&endpoint).expect("tags 响应含 models 时应判定 Ollama 可用");
    handle.join().expect("fake Ollama thread 应正常结束");
}

/// OL-8: readiness 必须确认配置模型已经安装，不能把空模型服务判为可对话。
///
/// 数据构造（含关键数值的推导过程）：
///   fake Ollama /api/tags 返回 models = []；目标模型 = qwen3.5:0.8b
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 probe_ollama_model_ready(endpoint, model)
///   2. 解析已安装模型列表并匹配目标模型
///
/// 预期结果：
///   - 正断言：返回错误且包含目标模型与 `ollama pull` 修复提示
///   - 负断言：空模型列表不得返回 Ok，避免 ASR 完成后才静默失败
#[test]
fn test_probe_ollama_model_ready_rejects_empty_model_list() {
    let (endpoint, handle) = start_fake_ollama_tags_only(r#"{"models":[]}"#);

    let result = probe_ollama_model_ready(&endpoint, "qwen3.5:0.8b");
    handle.join().expect("fake Ollama thread 应正常结束");

    assert!(
        matches!(result, Err(LlmError::Message(ref message))
            if message.contains("qwen3.5:0.8b") && message.contains("ollama pull")),
        "未安装目标模型时应返回可执行的下载提示，实际: {result:?}"
    );
    assert!(result.is_err(), "空模型列表不应被判定为 ready");
}

/// OL-2: probe_ollama_ready 在连接失败时应返回可读错误。
///
/// 数据构造（含关键数值的推导过程）：
///   endpoint = http://127.0.0.1:1（本地保留端口，预期无服务监听）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 对不可达端口调用 probe_ollama_ready
///
/// 预期结果：
///   - 正断言：返回 Err(LlmError::Message(...))
///   - 负断言：不应返回 Ok(())
#[test]
fn test_probe_ollama_ready_rejects_unreachable_endpoint() {
    let result = probe_ollama_ready("http://127.0.0.1:1");

    assert!(
        matches!(result, Err(LlmError::Message(ref msg)) if msg.contains("ollama request failed")),
        "不可达 endpoint 应映射为 ollama request failed 错误，实际: {result:?}"
    );
}

/// OL-3: OllamaLlmClient::chat 应向 /api/chat 提交 tools 并解析 assistant 文本。
///
/// 数据构造（含关键数值的推导过程）：
///   model = qwen3.5:0.8b
///   user transcript = 你好
///   tools = card_tool_specs() 共 3 个（Create/Focus/SendText）
///   fake response content = 收到：你好
///
/// 执行过程（逐步说明系统如何处理）：
///   1. fake Ollama 先响应 /api/tags（若被调用），再响应 /api/chat
///   2. chat 发送 POST /api/chat，body 含 messages 与 tools
///   3. 解析 message.content
///
/// 预期结果：
///   - 正断言：assistant_text == 收到：你好
///   - 正断言：请求 body.tools 长度为 3
///   - 负断言：tool_calls 应为空
#[test]
fn test_ollama_chat_posts_tools_and_parses_assistant_text() {
    let (endpoint, requests, handle) =
        start_fake_ollama(|method, path, _body| match (method, path) {
            ("POST", "/api/chat") => {
                r#"{"message":{"role":"assistant","content":"收到：你好"}}"#.to_string()
            }
            _ => r#"{"models":[]}"#.to_string(),
        });
    let mut client = OllamaLlmClient::new(endpoint, "qwen3.5:0.8b");
    let request = ChatRequest {
        messages: vec![
            ChatMessage {
                role: Role::System,
                content: "你是 Norma 语音助手。".into(),
                tool_call_id: None,
                tool_calls: vec![],
            },
            ChatMessage {
                role: Role::User,
                content: "你好".into(),
                tool_call_id: None,
                tool_calls: vec![],
            },
        ],
        tools: card_tool_specs(),
    };

    let response = client
        .chat(&request)
        .expect("fake Ollama chat 响应应可解析");

    handle.join().expect("fake Ollama thread 应正常结束");

    assert_eq!(
        response.assistant_text, "收到：你好",
        "assistant 文本应来自 message.content"
    );
    assert!(
        response.tool_calls.is_empty(),
        "纯文本回复不应包含 tool_calls"
    );

    let body = request_body(&requests, 0);
    let value: Value = serde_json::from_str(&body).expect("chat request body 应是 JSON");
    assert_eq!(
        value["model"].as_str(),
        Some("qwen3.5:0.8b"),
        "chat 请求应携带配置模型名"
    );
    assert_eq!(
        value["tools"].as_array().map(Vec::len),
        Some(3),
        "chat 请求应附带 3 个 card tools"
    );
    assert_eq!(
        value["stream"].as_bool(),
        Some(false),
        "chat 请求应关闭 stream"
    );
}

/// OL-9: 实时语音 chat 请求必须关闭深度思考并限制回复长度。
///
/// 数据构造（含关键数值的推导过程）：
///   model = qwen3.5:0.8b；单轮用户消息；回复上限 = 128 tokens
///   128 足以覆盖简短语音答复，同时避免默认 thinking 长时间占满 30 秒超时。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. build_chat_body 序列化 Ollama `/api/chat` 请求
///   2. 解析 JSON 并检查低延迟参数
///
/// 预期结果：
///   - 正断言：think == false
///   - 正断言：options.num_predict == 128
///   - 负断言：请求不能依赖模型默认的无界 thinking
#[test]
fn test_chat_body_disables_thinking_and_caps_voice_reply_output() {
    let request = ChatRequest {
        messages: vec![ChatMessage {
            role: Role::User,
            content: "你好".into(),
            tool_call_id: None,
            tool_calls: vec![],
        }],
        tools: card_tool_specs(),
    };

    let body = build_chat_body("qwen3.5:0.8b", &request).expect("实时语音 chat body 应可序列化");
    let value: Value = serde_json::from_slice(&body).expect("chat body 应是 JSON");

    assert_eq!(
        value["think"].as_bool(),
        Some(false),
        "实时语音请求应显式关闭 thinking"
    );
    assert_eq!(
        value["options"]["num_predict"].as_u64(),
        Some(128),
        "实时语音回复应限制为 128 tokens"
    );
    assert_ne!(
        value["think"],
        Value::Null,
        "请求不得依赖 thinking 模型的默认行为"
    );
}

/// OL-L3-1: 本机真实 Ollama 应在语音轮次超时内返回可播报文本。
///
/// 数据构造（含关键数值的推导过程）：
///   endpoint = MULTISOUL_OLLAMA_ENDPOINT 或默认 127.0.0.1:11434
///   model = MULTISOUL_DIALOGUE_MODEL 或默认 qwen3.5:0.8b
///   30 秒上限与生产 OllamaLlmClient timeout 一致
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 真实调用 `/api/chat`，携带生产 tools、think=false 与 128-token 上限
///   2. 解析 assistant response 并记录耗时
///
/// 预期结果：
///   - 正断言：30 秒内返回非空可播报文本
///   - 负断言：不应返回 tool call 或卡在默认 thinking 直到超时
#[test]
#[ignore = "requires running Ollama and the configured dialogue model"]
fn test_live_ollama_chat_returns_bounded_voice_reply() {
    let endpoint = std::env::var("MULTISOUL_OLLAMA_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string());
    let model =
        std::env::var("MULTISOUL_DIALOGUE_MODEL").unwrap_or_else(|_| "qwen3.5:0.8b".to_string());
    probe_ollama_model_ready(&endpoint, &model)
        .expect("真实 Ollama 应运行且已安装配置的 dialogue model");
    let mut client = OllamaLlmClient::new(endpoint, model);
    let request = ChatRequest {
        messages: vec![
            ChatMessage {
                role: Role::System,
                content: "你是 Norma 语音助手，请用一句简短中文回答。".into(),
                tool_call_id: None,
                tool_calls: vec![],
            },
            ChatMessage {
                role: Role::User,
                content: "你好，请回复实时语音测试成功。".into(),
                tool_call_id: None,
                tool_calls: vec![],
            },
        ],
        tools: card_tool_specs(),
    };
    let started = std::time::Instant::now();

    let response = client.chat(&request).expect("真实 Ollama chat 应成功");
    let elapsed = started.elapsed();

    assert!(
        !response.assistant_text.trim().is_empty(),
        "真实 Ollama 应返回可播报文本，实际: {response:?}"
    );
    assert!(
        response.tool_calls.is_empty(),
        "普通问候不应错误触发 card tools，实际: {:?}",
        response.tool_calls
    );
    assert!(
        elapsed < Duration::from_secs(30),
        "真实语音回复应在生产超时前完成，实际耗时: {elapsed:?}"
    );
    eprintln!(
        "live Ollama voice reply in {:.3}s: {}",
        elapsed.as_secs_f64(),
        response.assistant_text
    );
}

/// OL-4: OllamaLlmClient::chat 应解析 tool_calls 并保留 function.arguments JSON 字符串。
///
/// 数据构造（含关键数值的推导过程）：
///   fake tool call name = Create
///   arguments.kind = terminal
///   tool id = call-create-1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. fake Ollama 返回含 tool_calls 的 assistant message
///   2. chat 将 arguments 对象序列化为 JSON 字符串
///
/// 预期结果：
///   - 正断言：tool_calls.len() == 1
///   - 正断言：name == Create 且 arguments_json 含 terminal
///   - 负断言：assistant_text 应为空字符串
#[test]
fn test_ollama_chat_parses_tool_calls() {
    let (endpoint, _requests, handle) = start_fake_ollama(|_method, path, _body| {
        if path == "/api/chat" {
            return r#"{
                "message": {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [
                        {
                            "id": "call-create-1",
                            "function": {
                                "name": "Create",
                                "arguments": {"kind": "terminal"}
                            }
                        }
                    ]
                }
            }"#
            .to_string();
        }
        r#"{"models":[]}"#.to_string()
    });
    let mut client = OllamaLlmClient::new(endpoint, "qwen3.5:0.8b");
    let request = ChatRequest {
        messages: vec![ChatMessage {
            role: Role::User,
            content: "打开终端".into(),
            tool_call_id: None,
            tool_calls: vec![],
        }],
        tools: card_tool_specs(),
    };

    let response = client
        .chat(&request)
        .expect("含 tool_calls 的 chat 响应应可解析");

    handle.join().expect("fake Ollama thread 应正常结束");

    assert!(
        response.assistant_text.is_empty(),
        "仅 tool call 回复时 assistant_text 应为空"
    );
    assert_eq!(response.tool_calls.len(), 1, "应解析出 1 个 tool call");
    assert_eq!(
        response.tool_calls[0].id, "call-create-1",
        "tool call id 应保留模型返回值"
    );
    assert_eq!(
        response.tool_calls[0].name, "Create",
        "tool call name 应映射 function.name"
    );
    assert!(
        response.tool_calls[0].arguments_json.contains("terminal"),
        "arguments 对象应序列化为 JSON 字符串"
    );
}
