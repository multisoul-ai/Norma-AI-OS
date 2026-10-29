use super::*;
use norma::agent_hook::{forward_question, HookForwardEnv, HookSource, QuestionHookStatus};
use norma::protocol::QuestionAnswer;
use std::time::Instant;

/// 场景：原生问答的长等待不能占满普通HTTP worker，工作区撤销要回退原TUI。
/// 构造：5个并发问题（比4个worker多1），同鉴权工作区，真实PreToolUse消息。
/// 执行：接收全部ticket，回答第一题Green，撤销路由，回收server与全部客户端。
/// 预期：问题单独入队，审批/普通事件通道为空；恰好一个答案，其余204无答案。
#[test]
fn native_questions_wait_off_workers_and_revoke_without_answer() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/agent-hooks", listener.local_addr().unwrap());
    let registry = Arc::new(Mutex::new(AgentHookRegistry::default()));
    let (token, events, _, approvals, questions) = registry.lock().unwrap().register().unwrap();
    let shutdown = Arc::new(AtomicBool::new(false));
    let server_registry = registry.clone();
    let server_shutdown = shutdown.clone();
    let server = thread::spawn(move || accept_loop(listener, server_registry, server_shutdown));
    let clients: Vec<_> = (0..5)
        .map(|node_id| {
            let env = HookForwardEnv {
                node_id,
                endpoint: endpoint.clone(),
                token: token.clone(),
            };
            thread::spawn(move || {
                let mut raw: serde_json::Value = serde_json::from_str(include_str!(
                    "../../../norma/tests/fixtures/questions/claude-pre-tool-use.json"
                ))
                .unwrap();
                raw["tool_use_id"] = format!("tool-{node_id}").into();
                forward_question(HookSource::Claude, raw, env, "run".into())
            })
        })
        .collect();
    let received: Vec<_> = (0..5)
        .map(|_| {
            questions
                .recv_timeout(Duration::from_secs(5))
                .expect("全部问题应及时入队，不能占住worker")
        })
        .collect();
    assert!(approvals.is_empty(), "AskUserQuestion不得变成权限审批");
    assert!(events.is_empty(), "等待ticket不能当普通观察事件丢弃");
    let answer = vec![QuestionAnswer {
        key: "question-0".into(),
        selected: vec!["option-1".into()],
        text: None,
    }];
    assert!(
        received[0].submit_answer(&answer, Instant::now()).unwrap(),
        "首题可回答"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while received[0].status(Instant::now()) != QuestionHookStatus::Responded
        && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        received[0].status(Instant::now()),
        QuestionHookStatus::Responded,
        "首题应被传输取走"
    );
    registry.lock().unwrap().unregister(&token);
    let outputs: Vec<_> = clients
        .into_iter()
        .map(|client| client.join().unwrap().unwrap())
        .collect();
    shutdown.store(true, Ordering::Release);
    server.join().unwrap();
    assert_eq!(
        outputs.iter().filter(|value| value.is_some()).count(),
        1,
        "只能回答明确提交的一题"
    );
    assert_eq!(
        outputs.iter().filter(|value| value.is_none()).count(),
        4,
        "撤销工作区应释放其余原生问答"
    );
    assert!(
        received[1..]
            .iter()
            .all(|ticket| ticket.status(Instant::now()) == QuestionHookStatus::Cancelled),
        "剩余句柄必须永久失效"
    );
}
