use super::*;
use crate::{
    CardRef, PlannedCardKind, PlannedCommand, PlannedTarget, VoiceCardKind, VoicePlanningRequest,
};
use norma::protocol::AgentProfile;
use serde_json::Value;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

fn start_fake_ollama(responses: Vec<String>) -> (String, Arc<Mutex<Vec<String>>>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("fake Ollama listener 应绑定本地端口");
    let endpoint = format!(
        "http://{}",
        listener
            .local_addr()
            .expect("fake Ollama listener 应能读取本地地址")
    );
    let requests = Arc::new(Mutex::new(Vec::new()));
    let requests_for_thread = Arc::clone(&requests);
    let handle = thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().expect("fake Ollama 应接受 planner 请求");
            let request = read_http_request(&mut stream);
            requests_for_thread
                .lock()
                .expect("fake Ollama request lock 不应 poisoned")
                .push(request);
            write_ollama_response(&mut stream, &response);
        }
    });

    (endpoint, requests, handle)
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
            Err(error) => panic!("fake Ollama 读取 planner 请求失败: {error}"),
        }
    }

    String::from_utf8(bytes).expect("planner HTTP request 应是 UTF-8")
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

fn write_ollama_response(stream: &mut TcpStream, response: &str) {
    let body = serde_json::json!({ "response": response }).to_string();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .expect("fake Ollama 应能写 HTTP response");
}

fn request_count(requests: &Arc<Mutex<Vec<String>>>) -> usize {
    requests
        .lock()
        .expect("fake Ollama request lock 不应 poisoned")
        .len()
}

fn request_prompt(requests: &Arc<Mutex<Vec<String>>>, index: usize) -> String {
    let requests = requests
        .lock()
        .expect("fake Ollama request lock 不应 poisoned");
    let body = requests[index]
        .split_once("\r\n\r\n")
        .expect("HTTP request 应包含 header/body 分隔符")
        .1;
    let value: Value = serde_json::from_str(body).expect("Ollama request body 应是 JSON");
    value["prompt"]
        .as_str()
        .expect("Ollama request body 应包含 prompt 字符串")
        .to_string()
}

/// O-R1: 显式 create 语音遇到坏 JSON 时，应先走确定性 fallback，不再二次调用模型。
///
/// 数据构造（含关键数值的推导过程）：
///   transcript = "打 开 两 个 终 端 合 一 个  codex"
///   显式实体 = Codex(1) + Terminal(2)
///   Total commands = 3，低于 max_steps(8)
///   fake Ollama responses = 1 个坏 JSON response，模拟小模型输出不可解析文本
///
/// 执行过程（逐步说明系统如何处理）：
///   1. fake Ollama 第一次返回 "not json"
///   2. planner 解析失败后检查 transcript 中的显式 create intent
///   3. 使用确定性 required plan 生成 3 条 create 命令
///
/// 预期结果：
///   - 正断言：planner 返回 3 条 create 命令，不把 provider error 暴露给 app
///   - 正断言：两条 Terminal 都存在，不能把“两个终端”合并
///   - 负断言：fake Ollama 只收到 1 次请求，说明确定性 fallback 没有再走模型
#[test]
fn test_plan_recovers_malformed_json_with_deterministic_create_fallback() {
    let (endpoint, requests, handle) = start_fake_ollama(vec!["not json".to_string()]);
    let request = VoicePlanningRequest {
        transcript: "打 开 两 个 终 端 合 一 个  codex".to_string(),
        cards: Vec::new(),
        focused_node_id: None,
        selected_node_ids: Vec::new(),
        max_steps: 8,
    };
    let planner = OllamaVoicePlanner::new(endpoint, "qwen3.5:0.8b");

    let plan = planner
        .plan(&request)
        .expect("显式 create 语音的坏 JSON 应被确定性 fallback 恢复");
    handle.join().expect("fake Ollama thread 应正常结束");

    assert_eq!(
        plan.commands.len(),
        3,
        "Codex(1) + Terminal(2) 应恢复成 3 条 create 命令"
    );
    assert_eq!(
        plan.commands[0],
        PlannedCommand::Create {
            kind: PlannedCardKind::Agent {
                profile: AgentProfile::Codex,
            },
        },
        "确定性 fallback 应识别 codex 并创建 Codex agent"
    );
    assert_eq!(
        plan.commands[1],
        PlannedCommand::Create {
            kind: PlannedCardKind::Terminal,
        },
        "第 1 个 Terminal create 应存在"
    );
    assert_eq!(
        plan.commands[2],
        PlannedCommand::Create {
            kind: PlannedCardKind::Terminal,
        },
        "第 2 个 Terminal create 应存在，不能合并计数"
    );
    assert_ne!(
        request_count(&requests),
        2,
        "确定性 fallback 成功时不应再进行 JSON repair retry"
    );
    assert_eq!(
        request_count(&requests),
        1,
        "确定性 fallback 路径只需要初始 planner 请求"
    );
}

/// O-R2: 非确定性坏 JSON 应触发一次 JSON repair retry，并使用 retry 返回的计划。
///
/// 数据构造（含关键数值的推导过程）：
///   transcript = "给 codex 输入 hello"，只有输入意图，没有打开/创建动词
///   cards = 1 个已有 Codex(node_id=20)，用于 node_id 路由
///   fake Ollama responses = 第 1 次坏 JSON + 第 2 次合法 VoicePlan
///   retry upper bound = 1 次，因此总请求数应为 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 初始 planner response 无法解析为 VoicePlan
///   2. 因没有显式 create intent，不能使用 deterministic create fallback
///   3. planner 发起一次 JSON repair prompt
///   4. retry response 解析为 send_text 到 node_id=20
///
/// 预期结果：
///   - 正断言：fake Ollama 收到 2 次请求，说明执行了一次 repair retry
///   - 正断言：第二次 prompt 是 JSON repair prompt
///   - 正断言：返回计划发送 hello 到 node_id=20
///   - 负断言：请求数不应超过 2，避免无限重试拖慢语音链路
#[test]
fn test_plan_retries_once_to_repair_non_deterministic_malformed_json() {
    let repaired_json = r#"{"commands":[{"type":"send_text","target":{"type":"node_id","node_id":20},"text":"hello","submit":false}],"confidence":0.92,"risk":"low","requires_confirmation":false,"reason":"send hello to existing Codex"}"#;
    let (endpoint, requests, handle) = start_fake_ollama(vec![
        "commands: send hello".to_string(),
        repaired_json.to_string(),
    ]);
    let request = VoicePlanningRequest {
        transcript: "给 codex 输入 hello".to_string(),
        cards: vec![CardRef {
            node_id: 20,
            title: "Codex".to_string(),
            alias: None,
            kind: VoiceCardKind::Agent {
                profile: AgentProfile::Codex,
            },
            selected: false,
            focused: false,
        }],
        focused_node_id: None,
        selected_node_ids: Vec::new(),
        max_steps: 8,
    };
    let planner = OllamaVoicePlanner::new(endpoint, "qwen3.5:0.8b");

    let plan = planner
        .plan(&request)
        .expect("非 create 坏 JSON 应通过一次 repair retry 恢复");
    handle.join().expect("fake Ollama thread 应正常结束");

    assert_eq!(
        request_count(&requests),
        2,
        "初始请求 + 1 次 JSON repair retry 应合计 2 次请求"
    );
    assert!(
        request_prompt(&requests, 1).contains("Repair the malformed VoicePlan JSON"),
        "第二次请求应使用 JSON repair prompt，而不是重复完整 planner prompt"
    );
    assert_eq!(
        plan.commands,
        vec![PlannedCommand::SendText {
            target: PlannedTarget::NodeId { node_id: 20 },
            text: "hello".to_string(),
            submit: false,
        }],
        "retry response 应解析为发送 hello 到已有 Codex node_id=20"
    );
    assert_ne!(
        request_count(&requests),
        3,
        "planner 不应超过一次 JSON repair retry"
    );
}

/// O-R3: endpoint/network 类失败不应进入 JSON repair retry。
///
/// 数据构造（含关键数值的推导过程）：
///   endpoint = "https://127.0.0.1:11434"，不符合 provider 只支持 http://host:port 的约束
///   transcript = "给 codex 输入 hello"，无确定性 create fallback
///   retry 前提 = 必须先拿到 Ollama response；InvalidEndpoint 没有 response 可修复
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 invalid endpoint planner
///   2. 调用 plan
///   3. 检查错误仍是 InvalidEndpoint
///
/// 预期结果：
///   - 正断言：返回 InvalidEndpoint
///   - 负断言：错误不应被转换为 MissingBody/Json，说明没有进入 response repair 路径
#[test]
fn test_plan_does_not_retry_endpoint_failures_as_json_repair() {
    let request = VoicePlanningRequest {
        transcript: "给 codex 输入 hello".to_string(),
        cards: Vec::new(),
        focused_node_id: None,
        selected_node_ids: Vec::new(),
        max_steps: 8,
    };
    let planner = OllamaVoicePlanner::new("https://127.0.0.1:11434", "qwen3.5:0.8b");

    let error = planner
        .plan(&request)
        .expect_err("invalid endpoint 应直接返回 provider error");

    assert!(
        matches!(error, VoicePlanProviderError::InvalidEndpoint),
        "endpoint 失败应保持 InvalidEndpoint，不应进入 JSON repair retry: {error:?}"
    );
    assert!(
        !matches!(
            error,
            VoicePlanProviderError::MissingBody | VoicePlanProviderError::Json(_)
        ),
        "endpoint 失败不应被误归类为 response JSON repair 错误"
    );
}
