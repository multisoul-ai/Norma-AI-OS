use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, MemorySessionContext,
    NormaSession, ProviderConfig, ProviderProtocol, ToolContext,
};
use futures::future::BoxFuture;
use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
};

struct MemoryHost(Arc<Mutex<MemorySessionContext>>);
impl AgentCardAdapter for MemoryHost {
    fn memory_context(&self) -> BoxFuture<'_, Result<Option<MemorySessionContext>, AdapterError>> {
        Box::pin(async { Ok(Some(self.0.lock().expect("host state").clone())) })
    }
    fn execute<'a>(
        &'a self,
        _: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async { Err(AdapterError::Rejected("fixture never calls a tool".into())) })
    }
}

/// 持续个人会话在关闭后重开读取时清理旧注入，Provider只收到当前目录与个人工具。
///
/// 数据构造：回环SSE接收4次请求；目录OLD-MEMORY→NEW-MEMORY；人设默认→专业；第四轮关闭记忆。
/// 执行过程：两轮正常聊天→撤销代次0→1及人设修改→第三轮→关闭记忆→第四轮，核对真实HTTP。
/// 预期结果：第二轮保留历史；第三轮无旧目录/旧回复且专业人设不混入暖心表达规则；第四轮无人设之外的记忆目录。
#[tokio::test]
async fn recall_epoch_clears_previous_injection_and_personal_tools_are_limited() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("回环服务");
    let address = listener.local_addr().expect("服务端口");
    let captured = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let requests = captured.clone();
    let server = std::thread::spawn(move || {
        for _ in 0..4 {
            let (mut socket, _) = listener.accept().expect("四次请求");
            // 5s = local exchange budget, comfortably above fixture response time (<1s).
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .expect("读取超时");
            let mut data = Vec::new();
            let header_end = loop {
                let mut buffer = [0u8; 4096];
                let count = socket.read(&mut buffer).expect("HTTP请求");
                assert!(count > 0, "完整请求前连接不可关闭");
                data.extend_from_slice(&buffer[..count]);
                if let Some(end) = data.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let header = String::from_utf8_lossy(&data[..header_end]);
            let length: usize = header
                .lines()
                .find_map(|line| {
                    line.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|text| text.trim().parse().expect("请求长度"))
                })
                .expect("有长度");
            while data.len() < header_end + length {
                let mut buffer = [0u8; 4096];
                let count = socket.read(&mut buffer).expect("HTTP正文");
                assert!(count > 0, "正文不能提前终止");
                data.extend_from_slice(&buffer[..count]);
            }
            requests.lock().expect("记录请求").push(
                serde_json::from_slice(&data[header_end..header_end + length]).expect("JSON"),
            );
            let chunk = serde_json::json!({"id":"fixture","object":"chat.completion.chunk","created":1,"model":"memory-fixture","choices":[{"index":0,"delta":{"role":"assistant","content":"OLD-ANSWER"},"finish_reason":null}]});
            let stop = serde_json::json!({"id":"fixture","object":"chat.completion.chunk","created":1,"model":"memory-fixture","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]});
            let body = format!("data: {chunk}\n\ndata: {stop}\n\ndata: [DONE]\n\n");
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).expect("SSE响应");
        }
    });
    let state = Arc::new(Mutex::new(MemorySessionContext {
        catalog: "OLD-MEMORY".into(),
        ..Default::default()
    }));
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        format!("http://{address}/v1"),
        "memory-fixture",
        Some("fixture".into()),
    );
    let mut session =
        NormaSession::new("", &config, Arc::new(MemoryHost(state.clone()))).expect("个人会话");
    session.turn("第一轮").await.expect("首轮");
    session.turn("第二轮").await.expect("次轮");
    {
        let mut state = state.lock().expect("host");
        state.recall_epoch = 1;
        state.catalog = "NEW-MEMORY".into();
        state.persona.tone = agent::PersonaTone::Professional;
        state.persona.instructions = "优先中文".into();
    }
    session.turn("第三轮").await.expect("撤销后的新轮");
    state.lock().expect("host").preferences.recall = false;
    session.turn("第四轮").await.expect("关闭记忆后的新轮");
    server.join().expect("服务结束");
    let requests = captured.lock().expect("四次实发请求");
    assert!(
        requests[2]["messages"][0]["content"]
            .to_string()
            .contains("专业清晰"),
        "下一轮的真实提示词必须采用新风格"
    );
    let professional_prompt = requests[2]["messages"][0]["content"].to_string();
    assert!(
        professional_prompt.contains("你的固定姓名是 Norma"),
        "选择专业风格必须保留固定身份"
    );
    assert!(
        professional_prompt.contains("工具必须串行执行"),
        "选择专业风格必须保留工具执行边界"
    );
    assert!(
        !professional_prompt.contains("语气柔软、自然、口语化"),
        "专业风格不能继续受默认暖心口语要求约束"
    );
    assert!(
        !professional_prompt.contains("不使用 Markdown"),
        "专业风格不能继承默认口语的格式禁令"
    );
    assert!(
        requests[3]["messages"][0]["content"]
            .to_string()
            .contains("优先中文"),
        "关闭记忆不能关闭用户设置的人设"
    );
    assert!(
        !requests[3]["messages"].to_string().contains("NEW-MEMORY"),
        "关闭记忆后不能泄漏之前的目录"
    );
    assert!(
        requests[1]["messages"].to_string().contains("OLD-ANSWER"),
        "未撤销时应保留对话历史"
    );
    assert!(
        requests[2]["messages"].to_string().contains("NEW-MEMORY"),
        "撤销后应注入当前目录"
    );
    assert!(
        !requests[2]["messages"].to_string().contains("OLD-MEMORY"),
        "旧记忆不得重新发给模型"
    );
    assert!(
        !requests[2]["messages"].to_string().contains("OLD-ANSWER"),
        "旧上下文应整体清除"
    );
    let tools = requests[2]["tools"].as_array().expect("工具目录");
    assert_eq!(tools.len(), 4, "个人会话只能有四个记忆工具");
    assert!(
        tools
            .iter()
            .any(|tool| tool["function"]["name"] == "memory_read"),
        "个人读取应存在"
    );
    assert!(
        !tools
            .iter()
            .any(|tool| tool["function"]["name"] == "create_terminal"),
        "个人不授予项目操作"
    );
}
