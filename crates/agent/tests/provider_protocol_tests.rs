use agent::{
    build_ollama_chat_request, build_openai_chat_request, parse_ollama_response,
    parse_openai_response, parse_openai_stream, AgentMessage, MessageRole, ProviderConfig,
    ProviderProtocol, ProviderRequest, ProviderStreamEvent, ProviderToolCall,
};

fn request() -> ProviderRequest {
    ProviderRequest {
        messages: vec![AgentMessage {
            role: MessageRole::User,
            content: "创建一个终端".to_string(),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }],
        tools: Vec::new(),
    }
}

/// P-1：两种 Provider 协议应生成各自的 wire body，且不泄漏另一协议字段。
///
/// 数据构造：同一条 user message、model=local-model、空 API key。
/// 执行过程：分别构造 Ollama 与 OpenAI-compatible request body。
/// 预期结果：两者都保留消息与模型，但只出现对应协议的 stream/tool 字段。
#[test]
fn provider_request_codecs_keep_protocol_boundary() {
    let ollama = ProviderConfig::new(
        ProviderProtocol::Ollama,
        "http://127.0.0.1:11434",
        "local-model",
        None,
    );
    let openai = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        "http://127.0.0.1:8080/v1",
        "local-model",
        Some("secret-key".to_string()),
    );

    let ollama_body =
        build_ollama_chat_request(&ollama, &request()).expect("Ollama body should build");
    let openai_body =
        build_openai_chat_request(&openai, &request()).expect("OpenAI body should build");

    assert!(
        ollama_body.contains("/api/chat"),
        "Ollama request 应使用 /api/chat 协议路径"
    );
    assert!(
        ollama_body.contains("local-model"),
        "Ollama body 应保留 model"
    );
    assert!(
        !ollama_body.contains("Authorization"),
        "空 API key 不应伪造 Authorization"
    );
    assert!(
        openai_body.contains("/chat/completions"),
        "OpenAI-compatible 应使用 chat completions 路径"
    );
    assert!(
        openai_body.contains("Bearer secret-key"),
        "OpenAI-compatible body metadata 应携带 API key"
    );
    assert!(
        !openai_body.contains("/api/chat"),
        "OpenAI-compatible 不得泄漏 Ollama 路径"
    );
}

/// P-1b：assistant tool call 必须按各协议的 function wire shape 编码，而不能直接暴露领域字段名。
///
/// 数据构造：assistant 返回 card_task，arguments_json 为 JSON 字符串。
/// 执行过程：分别构造 Ollama 与 OpenAI-compatible request body。
/// 预期结果：两者都使用 function.name/arguments，不出现内部 arguments_json 字段。
#[test]
fn provider_request_codecs_encode_assistant_tool_calls() {
    let request = ProviderRequest {
        messages: vec![AgentMessage {
            role: MessageRole::Assistant,
            content: String::new(),
            tool_call_id: None,
            tool_calls: vec![ProviderToolCall {
                id: "call-1".to_string(),
                name: "card_task".to_string(),
                arguments_json: r#"{"action":"start","intent":"创建终端"}"#.to_string(),
            }],
        }],
        tools: Vec::new(),
    };
    let ollama = ProviderConfig::new(
        ProviderProtocol::Ollama,
        "http://127.0.0.1:11434",
        "model",
        None,
    );
    let openai = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        "http://127.0.0.1:8080/v1",
        "model",
        None,
    );
    let ollama_body =
        build_ollama_chat_request(&ollama, &request).expect("Ollama body should build");
    let openai_body =
        build_openai_chat_request(&openai, &request).expect("OpenAI body should build");

    assert!(
        ollama_body.contains("\"function\""),
        "Ollama tool call 应使用 function 结构"
    );
    assert!(
        openai_body.contains("\"function\""),
        "OpenAI tool call 应使用 function 结构"
    );
    assert!(
        !ollama_body.contains("arguments_json"),
        "领域字段名不得泄漏到 Ollama wire body"
    );
    assert!(
        !openai_body.contains("arguments_json"),
        "领域字段名不得泄漏到 OpenAI wire body"
    );
}

/// P-2：Provider response parser 应归一化 OpenAI tool call 与 Ollama tool call。
///
/// 数据构造：两份协议响应都包含同名 create_card 和 JSON 参数。
/// 执行过程：分别解析 response payload。
/// 预期结果：Dialogue/CardTask 只看到统一 ProviderToolCall，不需要知道协议差异。
#[test]
fn provider_response_codecs_normalize_tool_calls() {
    let openai = parse_openai_response(
        r#"{"choices":[{"message":{"content":"","tool_calls":[{"id":"call-1","type":"function","function":{"name":"create_card","arguments":"{\"kind\":\"terminal\"}"}}]}}]}"#,
    )
    .expect("OpenAI response should parse");
    let ollama = parse_ollama_response(
        r#"{"message":{"content":"","tool_calls":[{"id":"call-1","function":{"name":"create_card","arguments":{"kind":"terminal"}}}]}}"#,
    )
    .expect("Ollama response should parse");

    assert_eq!(
        openai.tool_calls[0].id, "call-1",
        "OpenAI tool call id 应归一化"
    );
    assert_eq!(
        openai.tool_calls[0].name, "create_card",
        "OpenAI tool name 应归一化"
    );
    assert_eq!(
        ollama.tool_calls, openai.tool_calls,
        "两种协议应产生等价 tool call"
    );
}

/// P-3：Provider Debug 与 redacted view 不得包含明文 API key。
///
/// 数据构造：api_key="top-secret"。
/// 执行过程：格式化 config 的 Debug 与 redacted view。
/// 预期结果：两种输出均不含明文，且 redacted view 明确标识已脱敏。
#[test]
fn provider_config_redacts_api_key() {
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        "http://127.0.0.1:8080/v1",
        "model",
        Some("top-secret".to_string()),
    );
    let debug = format!("{config:?}");
    let redacted = config.redacted().to_string();

    assert!(!debug.contains("top-secret"), "Debug 不得输出 API key 明文");
    assert!(
        !redacted.contains("top-secret"),
        "脱敏配置不得输出 API key 明文"
    );
    assert!(
        redacted.contains("<redacted>"),
        "脱敏配置应明确显示 redacted 标记"
    );
}

fn _keep_import(_: ProviderToolCall) {}

/// P-4：OpenAI SSE 的分片 tool arguments 应在流结束后组成完整 JSON。
///
/// 数据构造：同一个 index=0 的 tool call 分两块返回 action 字段，且包含一个文本 delta。
/// 执行过程：调用公开 stream parser，并记录 TextDelta 与 ToolCall 事件。
/// 预期结果：文本只拼接一次；最终 ToolCall 保留 id/name 和完整 arguments JSON。
#[test]
fn openai_stream_assembles_split_tool_call_arguments() {
    let raw = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"正在派发\",\"tool_calls\":[{\"index\":0,\"id\":\"call-7\",\"function\":{\"name\":\"card_task\",\"arguments\":\"{\\\"action\\\":\"}}]}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"start\\\"}\"}}]}}]}\n\n",
        "data: [DONE]\n\n"
    );
    let mut events = Vec::new();

    let response = parse_openai_stream(raw, &mut |event| events.push(event))
        .expect("合法 OpenAI SSE 应成功解析");

    assert_eq!(response.text, "正在派发", "文本 delta 应按顺序拼接");
    assert_eq!(response.tool_calls.len(), 1, "应只归一化一个 tool call");
    assert_eq!(
        response.tool_calls[0].arguments_json, r#"{"action":"start"}"#,
        "跨 chunk arguments 应拼成完整 JSON"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            ProviderStreamEvent::TextDelta(text) if text == "正在派发"
        )),
        "stream parser 应发出文本增量"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            ProviderStreamEvent::ToolCall(call)
                if call.id == "call-7" && call.name == "card_task"
        )),
        "stream 结束时应发出完整 ToolCall"
    );
}
