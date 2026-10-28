use norma::agent_hook::NativeSendStatus;
use norma::opencode::client::OpenCodeQuestionClient;
use norma::opencode::native_questions::{OpenCodeNativeEvent, OpenCodeQuestionHost};
use norma::protocol::{QuestionAnswer, QuestionState};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

fn original() -> Value {
    serde_json::from_str(include_str!("fixtures/questions/opencode-request.json")).unwrap()
}
fn answer() -> Vec<QuestionAnswer> {
    vec![
        QuestionAnswer {
            key: "question-0".into(),
            selected: vec!["option-1".into()],
            text: None,
        },
        QuestionAnswer {
            key: "question-1".into(),
            selected: vec!["option-0".into(), "option-2".into()],
            text: None,
        },
        QuestionAnswer {
            key: "question-2".into(),
            selected: vec![],
            text: Some("NATIVE_OPENCODE_7F2".into()),
        },
    ]
}

struct Server {
    endpoint: String,
    running: Arc<AtomicBool>,
    posts: Arc<AtomicUsize>,
    consumed: Arc<AtomicBool>,
    rejected: Arc<AtomicBool>,
    skipped: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    fn new(drop_reply: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let running = Arc::new(AtomicBool::new(true));
        let posts = Arc::new(AtomicUsize::new(0));
        let consumed = Arc::new(AtomicBool::new(false));
        let rejected = Arc::new(AtomicBool::new(false));
        let skipped = Arc::new(AtomicBool::new(false));
        let (run, count, done, reject, skip) = (
            running.clone(),
            posts.clone(),
            consumed.clone(),
            rejected.clone(),
            skipped.clone(),
        );
        let worker = std::thread::spawn(move || {
            'connections: while run.load(Ordering::Acquire) {
                let mut socket = match listener.accept() {
                    Ok((socket, _)) => socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("本机server失败:{error}"),
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_millis(200)))
                    .unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut first = String::new();
                // Clients may abandon a preconnected socket without a request.
                if reader.read_line(&mut first).is_err() || first.is_empty() {
                    continue;
                }
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() || line.is_empty() {
                        continue 'connections;
                    }
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                assert!(length < 8192, "请求体有界");
                let mut body = vec![0; length];
                if reader.read_exact(&mut body).is_err() {
                    continue;
                }
                let value = if first.starts_with("GET /question?") {
                    if done.load(Ordering::Acquire) {
                        json!([])
                    } else {
                        json!([original()])
                    }
                } else if first.starts_with("POST /question/") {
                    let body: Value = serde_json::from_slice(&body).unwrap();
                    assert_eq!(
                        body,
                        json!({"answers":[["Green"],["macOS","Linux"],["NATIVE_OPENCODE_7F2"]]}),
                        "唯一POST必须回答原题全部子题"
                    );
                    count.fetch_add(1, Ordering::AcqRel);
                    done.store(true, Ordering::Release);
                    if drop_reply {
                        continue;
                    }
                    json!(true)
                } else if first.starts_with("GET /session/") {
                    assert!(
                        first.contains(&format!(
                            "/message/{}?",
                            original()["tool"]["messageID"].as_str().unwrap()
                        )),
                        "只点查原工具所在message"
                    );
                    let mut part: Value = serde_json::from_str(include_str!(
                        "fixtures/questions/opencode-consumed.json"
                    ))
                    .unwrap();
                    if skip.load(Ordering::Acquire) {
                        part["state"]["metadata"]["answers"][0] = json!([]);
                    }
                    if reject.load(Ordering::Acquire) {
                        part["state"]["status"] = "error".into();
                        part["state"]["error"] = "The user dismissed this question".into();
                        part["state"].as_object_mut().unwrap().remove("metadata");
                    }
                    json!({"parts":[part]})
                } else {
                    panic!("不得发新任务或访问其他路径:{first}")
                };
                let body = value.to_string();
                let _=write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
            }
        });
        Self {
            endpoint,
            running,
            posts,
            consumed,
            rejected,
            skipped,
            worker: Some(worker),
        }
    }
    fn host(&self) -> OpenCodeQuestionHost {
        let raw = original();
        let client = OpenCodeQuestionClient::new(
            &self.endpoint,
            "/tmp/original",
            raw["sessionID"].as_str().unwrap(),
            "opencode",
            "test-secret",
        )
        .unwrap();
        OpenCodeQuestionHost::start(client, 7, "owned-run".into()).unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        let result = self.worker.take().unwrap().join();
        if !std::thread::panicking() {
            result.expect("本机 HTTP fixture 不得 panic");
        }
    }
}

/// 场景：OpenCode原instance宿主后台发现题目、提交一次并用原message/call的工具结果确认。
/// 构造：本机鉴权HTTP模拟真实三题，并保留一条未发送请求的预连接；另一分支由人工先回答。
/// 执行：取题，宿主提交或让人工先消费，等待精确终态并检查实际POST次数。
/// 预期：本次发送才有delivery身份；人工同答案仍为Elsewhere且零POST；完成后解除输入占有。
#[test]
fn native_host_answers_once_and_reconciles_original_tool_consumption() {
    for manual in [false, true] {
        let server = Server::new(false);
        let _idle = std::net::TcpStream::connect(server.endpoint.trim_start_matches("http://"))
            .expect("构造尚未发送请求的预连接");
        let host = server.host();
        let ticket = loop {
            if let OpenCodeNativeEvent::Question(ticket) =
                host.events().recv_timeout(Duration::from_secs(5)).unwrap()
            {
                break ticket;
            }
        };
        assert!(host.input_pending(), "原题占有普通任务输入");
        if manual {
            server.consumed.store(true, Ordering::Release);
        } else {
            assert!(
                ticket
                    .submit_delivery(&answer(), "delivery", Instant::now())
                    .unwrap(),
                "首次提交成功"
            );
        }
        let mut observed = Vec::new();
        loop {
            let event = host.events().recv_timeout(Duration::from_secs(5)).unwrap_or_else(|error| {
                panic!("原调用未完成：manual={manual} posts={} send={:?} events={observed:?} error={error}", server.posts.load(Ordering::Acquire), ticket.native_send_status())
            });
            observed.push(format!("{event:?}"));
            if let OpenCodeNativeEvent::Changed {
                state,
                delivery_id,
                evidence,
                ..
            } = event
            {
                if matches!(
                    state,
                    QuestionState::Answered | QuestionState::AnsweredElsewhere
                ) {
                    assert_eq!(
                        state,
                        if manual {
                            QuestionState::AnsweredElsewhere
                        } else {
                            QuestionState::Answered
                        },
                        "相同答案不能冒充本次发送"
                    );
                    assert_eq!(
                        delivery_id.as_deref(),
                        if manual { None } else { Some("delivery") },
                        "原消费保留真实发送身份"
                    );
                    assert!(evidence.is_some(), "必须有原工具消费证据");
                    break;
                }
            }
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while host.input_pending() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!host.input_pending(), "消费后解除原问题输入占有");
        assert_eq!(
            server.posts.load(Ordering::Acquire),
            usize::from(!manual),
            "只允许本次唯一POST"
        );
        assert_eq!(
            ticket.native_send_status(),
            if manual {
                NativeSendStatus::Unsent
            } else {
                NativeSendStatus::Sent
            },
            "HTTP接纳与人工消费分开"
        );
        drop(host);
    }
}

/// 场景：HTTP答案可能到达但200回包丢失，后来同工具有相同消费，不能猜是Norma或人工所答。
/// 构造：本机server收到唯一POST后断开，下一次点查返回同call同答案的真实消费。
/// 执行：提交、等待消费对账，检查发送归属和输入锁。
/// 预期：原题已消费但delivery来源未知；不标Answered/Elsewhere，不重发，释放已经结束的输入。
#[test]
fn lost_http_ack_consumes_question_without_inventing_delivery_attribution() {
    let server = Server::new(true);
    let host = server.host();
    let ticket = loop {
        if let OpenCodeNativeEvent::Question(ticket) =
            host.events().recv_timeout(Duration::from_secs(5)).unwrap()
        {
            break ticket;
        }
    };
    ticket
        .submit_delivery(&answer(), "lost-ack", Instant::now())
        .unwrap();
    loop {
        if let OpenCodeNativeEvent::Changed {
            state,
            evidence,
            delivery_id,
            ..
        } = host.events().recv_timeout(Duration::from_secs(5)).unwrap()
        {
            assert!(
                !matches!(
                    state,
                    QuestionState::Answered | QuestionState::AnsweredElsewhere
                ),
                "丢失接纳回包不能推断来源"
            );
            if state == QuestionState::ConsumedUnattributed {
                assert!(evidence.is_some(), "保留原题真实消费证据");
                assert!(delivery_id.is_none(), "不能把相同答案当成本次delivery成功");
                break;
            }
        }
    }
    assert_eq!(server.posts.load(Ordering::Acquire), 1, "未知结果永不重发");
    assert_eq!(
        ticket.native_send_status(),
        NativeSendStatus::Unknown,
        "发送归属仍未知"
    );
    assert!(!host.input_pending(), "原题确已消费，不继续卡住新任务");
    drop(host);
}

/// 场景：用户在原OpenCode TUI拒绝问题后pending消失，工具以原call的error结束。
/// 构造：原问题ticket，原message/input/call一致的终止错误；没有任何Norma提交。
/// 执行：人工拒绝后让宿主点查结果并观察Cancelled事件。
/// 预期：精确失败证据才关闭原题，撤销ticket并解除输入，不发送POST也不虚构答案。
#[test]
fn native_rejection_releases_only_the_original_question() {
    let server = Server::new(false);
    let host = server.host();
    let ticket = loop {
        if let OpenCodeNativeEvent::Question(ticket) =
            host.events().recv_timeout(Duration::from_secs(5)).unwrap()
        {
            break ticket;
        }
    };
    server.rejected.store(true, Ordering::Release);
    server.consumed.store(true, Ordering::Release);
    loop {
        if let OpenCodeNativeEvent::Changed {
            state, evidence, ..
        } = host
            .events()
            .recv_timeout(Duration::from_secs(5))
            .expect("原工具拒绝必须结束问题")
        {
            assert!(
                !matches!(
                    state,
                    QuestionState::Answered | QuestionState::AnsweredElsewhere
                ),
                "拒绝不能伪造答案"
            );
            if state == QuestionState::Cancelled {
                assert!(evidence.is_some(), "保留同call的失败证据");
                break;
            }
        }
    }
    assert!(!host.input_pending(), "拒绝后不永久锁住输入");
    assert!(
        !ticket
            .submit_delivery(&answer(), "late", Instant::now())
            .unwrap(),
        "原题已关闭，迟到采用无效"
    );
    assert_eq!(server.posts.load(Ordering::Acquire), 0, "拒绝不发送答案");
    drop(host);
}

/// 场景：原生 TUI 允许跳过子题后提交，原工具正常完成但某项答案为空。
/// 构造：原三题消费夹具中第一题为空数组；不发出 Norma 回答。
/// 执行：人工跳过提交后等待宿主对账，再尝试迟到提交。
/// 预期：记录人工完成并解锁，不把空答案认作我方成功，不发送 POST。
#[test]
fn native_skipped_item_is_consumed_elsewhere() {
    let server = Server::new(false);
    let host = server.host();
    let ticket = loop {
        if let OpenCodeNativeEvent::Question(ticket) =
            host.events().recv_timeout(Duration::from_secs(5)).unwrap()
        {
            break ticket;
        }
    };
    server.skipped.store(true, Ordering::Release);
    server.consumed.store(true, Ordering::Release);
    loop {
        if let OpenCodeNativeEvent::Changed {
            state,
            delivery_id,
            evidence,
            ..
        } = host
            .events()
            .recv_timeout(Duration::from_secs(5))
            .expect("跳过子题也必须识别原工具完成")
        {
            assert_eq!(
                state,
                QuestionState::AnsweredElsewhere,
                "人工跳过不是我方完整回答"
            );
            assert!(delivery_id.is_none(), "人工完成不带我方归属");
            assert!(evidence.is_some(), "保留原工具结果");
            break;
        }
    }
    assert!(!host.input_pending(), "跳过子题不永久占用输入");
    assert!(
        !ticket
            .submit_delivery(&answer(), "late", Instant::now())
            .unwrap(),
        "人工完成后不能再答"
    );
    assert_eq!(
        server.posts.load(Ordering::Acquire),
        0,
        "人工跳过不发送 POST"
    );
}
