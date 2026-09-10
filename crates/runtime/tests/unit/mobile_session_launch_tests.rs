use super::*;
use std::os::unix::net::UnixListener;
use tungstenite::Message;

fn prepare_attempt(conflict: bool) -> (Vec<String>, bool, storage::MobileSession) {
    // /tmp keeps the Unix socket below macOS sockaddr_un's 104-byte ceiling.
    let dir = tempfile::tempdir_in("/tmp").expect("隔离目录");
    let socket = dir.path().join("control.sock");
    let listener = UnixListener::bind(&socket).expect("私有socket");
    let (mut runtime, id) = super::super::launch_tests::runtime_with_card();
    let CanvasNodeKind::Agent(spec) = &mut runtime.document.node_mut(id).expect("卡片").kind
    else {
        panic!("Agent")
    };
    spec.profile = AgentProfile::Codex;
    let store = storage::MobileTurnStore::new(dir.path().join("project.sqlite"));
    let audit_store = storage::ApprovalAuditStore::new(dir.path().join("project.sqlite"));
    let launch = runtime
        .mobile_agent_launch(id, Path::new("unused"), 80, 24)
        .expect("预约")
        .with_approval_audit(audit_store.clone());
    let expected = storage::MobileSession {
        key: storage::MobileSessionKey {
            node_id: id,
            profile: "Codex".into(),
            run_id: "run-a".into(),
            session_id: "thread-a".into(),
        },
        workspace_root: PathBuf::from("/tmp"),
        title: "Codex".into(),
    };
    if conflict {
        let mut original = expected.clone();
        original.key.session_id = "original-thread".into();
        store.record_session(&original).expect("已有准确身份");
    }
    let audit = audit::CodexAudit::new(id, audit_store).expect("既有writer");
    let key = expected.key.clone();
    let reader = store.clone();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("本地连接");
        // 5s = 既有本地RPC测试预算；没有模型生成，不应等待默认30s。
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("防挂预算");
        let mut ws = tungstenite::accept(stream).expect("WebSocket握手");
        let mut methods = Vec::new();
        let mut committed_before_turn = false;
        while let Ok(Message::Text(text)) = ws.read() {
            let call: Value = serde_json::from_str(&text).expect("RPC");
            let method = call["method"].as_str().expect("请求方法");
            methods.push(method.to_owned());
            let reply = match method {
                "initialize" => json!({"id":call["id"],"result":{}}),
                "initialized" => continue,
                "thread/start" => json!({"id":call["id"],"result":{"thread":{"id":"thread-a"}}}),
                "turn/start" => {
                    committed_before_turn = reader
                        .session(&key)
                        .expect("首次任务前重开数据库")
                        .is_some();
                    // Stop at the precise boundary, without running a model or native terminal.
                    json!({"id":call["id"],"error":{"code":-32000,"message":"probe stops after first turn request"}})
                }
                _ => panic!("不应发送其他请求：{method}"),
            };
            ws.send(Message::Text(reply.to_string().into()))
                .expect("RPC响应");
        }
        (methods, committed_before_turn)
    });
    let result = CodexHost::connect_and_start(&launch, &socket, Some("real task"), "run-a", audit);
    assert!(
        result.is_err(),
        "fixture在首次turn边界主动结束，不应启动TUI"
    );
    let (methods, committed_before_turn) = server.join().expect("结束测试服务");
    let stored = store.sessions(None).expect("索引");
    assert_eq!(stored.len(), 1, "每次尝试只能有原始的一份准确身份");
    (
        methods,
        committed_before_turn,
        stored.into_iter().next().expect("身份").session,
    )
}

/// 第一条模型任务发出前须提交准确身份；保存冲突时既不发任务也不覆盖旧身份。
///
/// 数据构造：两个隔离Unix RPC服务；成功分支无已有记录，冲突分支同run-a已绑定original-thread。
/// 执行过程：正式connect_and_start → thread/start返回thread-a → 保存身份 → 服务收到turn/start时重开SQLite检查。
/// 预期结果：成功分支收到turn/start前已落盘；冲突分支从未收到turn/start，原会话身份保留。
#[test]
fn codex_commits_identity_before_first_task_and_stops_on_conflict() {
    let (calls, committed, stored) = prepare_attempt(false);
    assert!(
        committed,
        "不能只在附着后补记，否则首条任务先执行会丢失身份"
    );
    assert!(
        calls.iter().any(|method| method == "turn/start"),
        "正常登记后应继续正式派发"
    );
    assert_eq!(
        stored.key.session_id, "thread-a",
        "必须记录Provider准确返回的线程"
    );
    let (calls, committed, stored) = prepare_attempt(true);
    assert!(
        !calls.iter().any(|method| method == "turn/start"),
        "身份保存失败不能开始模型任务"
    );
    assert!(!committed, "冲突不能伪装成新会话已登记");
    assert_eq!(
        stored.key.session_id, "original-thread",
        "失败必须保留原会话身份"
    );
}
