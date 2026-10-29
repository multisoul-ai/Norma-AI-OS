use norma::agent_hook::{forward_question, ClaudeQuestion, HookForwardEnv, HookSource};
use norma::protocol::QuestionAnswer;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

fn raw() -> Value {
    serde_json::from_str(include_str!("fixtures/questions/claude-pre-tool-use.json")).unwrap()
}

fn respond(body: String, status: &str) -> (HookForwardEnv, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let status = status.to_owned();
    let worker = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let (mut socket, _) = loop {
            match listener.accept() {
                Ok(pair) => break pair,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(10))
                }
                other => panic!("bridge必须在测试期限内连接: {other:?}"),
            }
        };
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(&socket);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(
            line, "POST /agent-hooks/question HTTP/1.1\r\n",
            "答题不可混进审批入口"
        );
        let mut length = 0;
        loop {
            line.clear();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length: ") {
                length = value.trim().parse().unwrap();
            }
        }
        let mut bytes = vec![0; length];
        reader.read_exact(&mut bytes).unwrap();
        let request: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(request["run_id"], "run", "请求绑定当前运行实例");
        assert_eq!(
            request["event"]["raw_json"]["tool_use_id"],
            raw()["tool_use_id"],
            "工具调用身份必须保留"
        );
        write!(
            socket,
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    (
        HookForwardEnv {
            node_id: 7,
            endpoint: format!("http://{address}/agent-hooks"),
            token: "test".into(),
        },
        worker,
    )
}

/// 场景：原生CLI安装的专用question Hook必须将真实HTTP答案输出到原调用stdout。
/// 构造：真实bridge子进程、PreToolUse夹具和loopback服务，运行身份只由环境提供。
/// 执行：stdin输入原题，服务端回Green，等待进程退出并解析唯一stdout JSON。
/// 预期：答案完整回填，原输入不变；不输出普通事件回执或权限审批JSON。
#[test]
fn real_question_bridge_returns_answer_to_native_stdout() {
    use std::process::{Command, Stdio};
    let answers = vec![QuestionAnswer {
        key: "question-0".into(),
        selected: vec!["option-1".into()],
        text: None,
    }];
    let output = ClaudeQuestion::from_hook(&raw())
        .unwrap()
        .answer_output(&answers)
        .unwrap();
    let (env, worker) = respond(output.to_string(), "200 OK");
    let mut child = Command::new(env!("CARGO_BIN_EXE_multisoul-agent-hook"))
        .args(["forward", "--source", "claude", "--request-question"])
        .env("MULTISOUL_NODE_ID", "7")
        .env("MULTISOUL_HOOK_TOKEN", &env.token)
        .env("MULTISOUL_HOOK_ENDPOINT", &env.endpoint)
        .env("NORMA_CONTROL_RUN_ID", "run")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(raw().to_string().as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "原生bridge必须正常返回: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let actual: Value =
        serde_json::from_slice(&result.stdout).expect("stdout必须是当前问题的单个答案JSON");
    worker.join().unwrap();
    assert_eq!(actual, output, "CLI只能返回当前AskUserQuestion的答案");
}

/// 场景：问答bridge返回完整答案，但不能接受服务端改写原题或权限配置。
/// 构造：真实PreToolUse与本机HTTP响应；分别返回有效答案、被替换的问题、204人工回退。
/// 执行：调用真实TCP转发，读取独立question路由的正文并等待服务端回收。
/// 预期：仅有效答案变成allow+updatedInput；篡改拒绝、204没有输出。
#[test]
fn bridge_preserves_question_and_falls_back_without_fake_success() {
    let answers = vec![QuestionAnswer {
        key: "question-0".into(),
        selected: vec!["option-1".into()],
        text: None,
    }];
    let output = ClaudeQuestion::from_hook(&raw())
        .unwrap()
        .answer_output(&answers)
        .unwrap();
    let (env, worker) = respond(output.to_string(), "200 OK");
    assert_eq!(
        forward_question(HookSource::Claude, raw(), env, "run".into()).unwrap(),
        Some(output.clone()),
        "只返回当前题的回答"
    );
    worker.join().unwrap();
    let mut tampered = output;
    tampered["hookSpecificOutput"]["updatedInput"]["questions"][0]["question"] =
        "Changed question".into();
    let (env, worker) = respond(tampered.to_string(), "200 OK");
    assert!(
        forward_question(HookSource::Claude, raw(), env, "run".into()).is_err(),
        "不能通过答案响应替换原题"
    );
    worker.join().unwrap();
    let (env, worker) = respond(String::new(), "204 No Content");
    assert_eq!(
        forward_question(HookSource::Claude, raw(), env, "run".into()).unwrap(),
        None,
        "人工回退不能输出allow"
    );
    worker.join().unwrap();
}

/// 场景：重复标签的问题不能结构化代答，但仍须通知宿主保留原题阻塞。
/// 构造：不支持的真实问答形态与独立loopback HTTP接收端。
/// 执行：调用生产forward_question，接收普通PreToolUse事件，不返回任何答案。
/// 预期：bridge仍回退原TUI，宿主得到绑定run/session/tool的待答事实。
#[test]
fn unsupported_question_notifies_host_before_returning_to_native_tui() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let worker = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let (mut socket, _) = loop {
            match listener.accept() {
                Ok(pair) => break pair,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(5))
                }
                _ => return None,
            }
        };
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut reader = BufReader::new(&socket);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(
            line, "POST /agent-hooks HTTP/1.1\r\n",
            "不可映射问题只通知待答，不请求权限放行"
        );
        let mut length = 0;
        loop {
            line.clear();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length: ") {
                length = value.trim().parse().unwrap();
            }
        }
        let mut bytes = vec![0; length];
        reader.read_exact(&mut bytes).unwrap();
        socket
            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .unwrap();
        Some(serde_json::from_slice::<Value>(&bytes).unwrap())
    });
    let mut raw = raw();
    raw["tool_input"]["questions"][0]["options"][1]["label"] = "Blue".into();
    let result = forward_question(
        HookSource::Claude,
        raw,
        HookForwardEnv {
            node_id: 7,
            endpoint: format!("http://{address}/agent-hooks"),
            token: "test".into(),
        },
        "run".into(),
    );
    assert!(result.is_err(), "不支持的题型仍回退原TUI，不构造不确定答案");
    let event = worker
        .join()
        .unwrap()
        .expect("回退前必须通知宿主原题仍在等待");
    assert_eq!(
        event["control_run_id"], "run",
        "原生待答事实必须绑定当前进程"
    );
    assert_eq!(event["provider_event"], "PreToolUse", "保留等待开始事件");
    assert_eq!(event["tool_name"], "AskUserQuestion", "不借用权限审批语义");
    assert!(
        event["raw_json"]["tool_input"]["answers"].is_null(),
        "不支持映射不能编造答案"
    );
}
