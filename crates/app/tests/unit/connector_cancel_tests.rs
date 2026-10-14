use super::test_support::Fixture;
use norma::agent_hook::{forward_event, parse_hook_event, HookSource};
use serde_json::json;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

struct CodexPty {
    output: crossbeam_channel::Sender<terminal::PtyEvent>,
    stopped: Arc<AtomicBool>,
    writes: Arc<std::sync::Mutex<Vec<u8>>>,
}
impl terminal::PtyBackend for CodexPty {
    fn write(&mut self, bytes: &[u8]) -> Result<(), terminal::PtyError> {
        self.writes.lock().expect("输入记录").extend(bytes);
        let _ = self.output.send(terminal::PtyEvent::Output(bytes.to_vec()));
        if bytes == [3] {
            self.stopped.store(true, Ordering::SeqCst);
        }
        Ok(())
    }
    fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), terminal::PtyError> {
        Ok(())
    }
    fn kill(&mut self) {}
}
fn frame(f: &mut Fixture) {
    f.test
        .background_executor
        .advance_clock(Duration::from_millis(120));
    f.test.run_until_parked();
}
fn hook(f: &mut Fixture, node: u64, event: &str) {
    let config = f.test.update(|cx| {
        f.sessions[0]
            .read(cx)
            .agent_hook_server
            .runtime_config()
            .expect("Hook入口")
    });
    let event = parse_hook_event(
        HookSource::Codex,
        node,
        json!({"hook_event_name":event,"session_id":"session-a","turn_id":"turn-a"}),
    )
    .expect("Codex Hook");
    forward_event(&config.endpoint, &config.token, &event).expect("Hook投递");
    frame(f);
}

/// 场景：Codex 接受 Ctrl+C 并写入 canonical turnCompleted，但没有发送 Stop Hook。
/// 构造：临时区域、记录 PTY、真实 Unix WebSocket 只读 Provider 假实现；一条运行任务及一条队列任务。
/// 执行：提交任务 → UserPromptSubmit → 排队 → cancel → Provider 中断证据 → 生产帧和后台读者。
/// 预期：按键及 RPC ack 不能冒充取消，只有准确终态使第一任务 cancelled，第二任务提交且正文只写一次。
#[test]
fn connector_codex_cancel_without_stop_hook_advances_queue() {
    let mut f = Fixture::new();
    let stopped = Arc::new(AtomicBool::new(false));
    let writes = Arc::new(std::sync::Mutex::new(Vec::new()));
    let pty_stopped = stopped.clone();
    let pty_writes = writes.clone();
    f.test.update(|cx| {
        f.sessions[0].update(cx, |app, _| {
            app.runtime
                .set_pty_factory(Box::new(move |_, _, _, output| {
                    Ok(Box::new(CodexPty {
                        output,
                        stopped: pty_stopped.clone(),
                        writes: pty_writes.clone(),
                    }))
                }))
        })
    });
    let card = f.create("cancel-card", 0, Some("first task"));
    assert_eq!(card["state"], "loaded", "建卡派工成功：{card}");
    let node = f
        .repo
        .signer
        .verify(card["card_id"].as_str().expect("卡片"))
        .expect("身份")
        .node_id;
    hook(&mut f, node, "SessionStart");
    let submit_deadline = Instant::now() + Duration::from_secs(3);
    loop {
        f.test.update(|cx| {
            f.sessions[0].update(cx, |app, cx| {
                app.pending_agent_submissions.mark_ready(node);
                app.submit_pending_agent_task_for_ready_node(node, cx);
            })
        });
        frame(&mut f);
        if writes.lock().expect("首任务输入").ends_with(b"\r") {
            break;
        }
        assert!(
            Instant::now() < submit_deadline,
            "先确认首任务实际发送 Enter，输入为 {:?}",
            writes.lock().expect("输入")
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    hook(&mut f, node, "UserPromptSubmit");
    let directory = f.call("directory", json!({}));
    let current = &directory["workspaces"][0]["cards"][0];
    let queued=f.call("dispatch_task",json!({"operation_id":"queued","card_id":current["card_id"],"card_revision":current["card_revision"],"task":"next task"}));
    assert_eq!(queued["state"], "queued", "确实排在运行任务之后：{queued}");
    let root = f.test.update(|cx| {
        f.sessions[0]
            .read(cx)
            .runtime
            .workspace_root()
            .expect("区域根")
            .to_path_buf()
    });
    let (connection, provider) =
        archive_provider(&f._guard.path().join("codex.sock"), root, stopped);
    f.test.update(|cx| {
        f.sessions[0].update(cx, |app, _| {
            app.runtime.set_mobile_codex_archive_connection(connection)
        })
    });
    let cancelled = f.call(
        "cancel_task",
        json!({"operation_id":"cancel","task_id":card["task_id"]}),
    );
    assert_eq!(
        cancelled["state"], "cancel_requested",
        "等待真实停止证据：{cancelled}"
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        frame(&mut f);
        let state = f.test.update(|cx| {
            f.sessions[0]
                .read(cx)
                .agent_task_coordinator
                .as_ref()
                .expect("协调器")
                .task_by_ref(card["task_id"].as_str().expect("任务"))
                .expect("目标任务")
                .state
        });
        if state != "cancel_requested" || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let progress = f.call("get_task_progress", json!({"task_id":card["task_id"]}));
    assert_eq!(
        progress["state"], "cancelled",
        "没有 Stop Hook 也必须收口：{progress}"
    );
    assert_ne!(progress["state"], "cancel_requested", "不能无限挂起");
    let submit_deadline = Instant::now() + Duration::from_secs(5);
    loop {
        frame(&mut f);
        let submitted = f.test.update(|cx| {
            let app = f.sessions[0].read(cx);
            let task = app
                .agent_task_coordinator
                .as_ref()
                .expect("协调器")
                .task_by_ref(queued["task_id"].as_str().expect("下个任务"))
                .expect("队列目标");
            app.pending_agent_submissions
                .task_stage(task.task_id)
                .is_none()
        });
        if submitted {
            break;
        }
        assert!(
            Instant::now() < submit_deadline,
            "下一任务必须实际发送 Enter"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let next = f.call("get_task_progress", json!({"task_id":queued["task_id"]}));
    assert_eq!(
        next["state"], "running",
        "队列下一任务实际发送 Enter：{next}"
    );
    assert_eq!(
        String::from_utf8(writes.lock().expect("输入").clone())
            .expect("UTF8")
            .matches("next task")
            .count(),
        1,
        "下一条正文只发送一次"
    );
    drop(f);
    provider.join().expect("假 Provider 正常退出");
}

fn archive_provider(
    path: &std::path::Path,
    root: std::path::PathBuf,
    stopped: Arc<AtomicBool>,
) -> (norma::codex::CodexConnection, std::thread::JoinHandle<()>) {
    let listener = std::os::unix::net::UnixListener::bind(path).expect("临时 Provider socket");
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("归档连接");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("读取超时");
        let mut socket = tungstenite::accept(stream).expect("真实 WebSocket 握手");
        while let Ok(tungstenite::Message::Text(text)) = socket.read() {
            let request: serde_json::Value = serde_json::from_str(&text).expect("RPC JSON");
            let result = match request["method"].as_str() {
                Some("initialized") => continue,
                Some("initialize") => json!({}),
                Some("thread/read") => json!({"thread":{"id":"session-a","cwd":root}}),
                Some("thread/timeline/list") => {
                    let data = if stopped.load(Ordering::SeqCst) {
                        json!([{"type":"turnCompleted","turnId":"turn-a","position":1,"status":"interrupted"}])
                    } else {
                        json!([])
                    };
                    json!({"data":data})
                }
                other => panic!("只读假 Provider 不得收到有副作用 RPC：{other:?}"),
            };
            if socket
                .send(tungstenite::Message::Text(
                    json!({"id":request["id"],"result":result})
                        .to_string()
                        .into(),
                ))
                .is_err()
            {
                break;
            }
        }
    });
    (
        norma::codex::CodexConnection::connect(path).expect("连接隔离假 Provider"),
        server,
    )
}

/// 场景：旧卡片只有运行状态，没有可绑定的 Provider turn，Ctrl+C 后也没有 Stop。
/// 构造：临时 Codex 卡片、已提交协调器任务和一个排队任务；虚拟后台时钟。
/// 执行：cancel → 等待任务开始监控 → 时钟推进 31 秒（30 秒上限加 1 秒裕量）。
/// 预期：明确 failed/cancel_unconfirmed，卡片仍 busy，后续任务完整保留且没有被误提交。
#[test]
fn connector_cancel_without_turn_is_bounded_and_preserves_queue() {
    let mut f = Fixture::new();
    let card = f.create("unconfirmed", 0, Some("running task"));
    let task_ref = card["task_id"].as_str().expect("任务引用").to_owned();
    f.test.update(|cx| {
        f.sessions[0].update(cx, |app, _| {
            let task = app
                .agent_task_coordinator
                .as_ref()
                .expect("协调器")
                .task_by_ref(&task_ref)
                .expect("目标");
            app.pending_agent_submissions.remove_task(task.task_id);
            app.agent_task_coordinator
                .as_mut()
                .expect("协调器")
                .mark_task_submitted(task.task_id);
            app.runtime
                .apply_agent_task_update(runtime::AgentTaskUpdate {
                    node_id: task.target_node_id,
                    profile: canvas::AgentProfile::Codex,
                    provider_event: "UserPromptSubmit".into(),
                    to: runtime::AgentTaskState::Thinking,
                    task_title: None,
                    session_id: None,
                });
        })
    });
    let queued=f.call("dispatch_task",json!({"operation_id":"keep-queued","card_id":card["card_id"],"card_revision":card["card_revision"],"task":"retained task"}));
    assert_eq!(queued["state"], "queued", "后续工作确实在队列中");
    let cancelled = f.call(
        "cancel_task",
        json!({"operation_id":"unknown-stop","task_id":task_ref}),
    );
    assert_eq!(cancelled["state"], "cancel_requested", "按键不能冒充已停止");
    f.test.run_until_parked();
    f.test
        .background_executor
        .advance_clock(Duration::from_secs(31));
    f.test.run_until_parked();
    let writer = f.test.update(|cx| {
        f.sessions[0]
            .read(cx)
            .persistence
            .agent_dispatch_writer()
            .cloned()
            .expect("持久队列")
    });
    writer.flush().expect("失败状态已保存");
    let progress = f.call("get_task_progress", json!({"task_id":task_ref}));
    assert_eq!(
        progress["state"], "failed",
        "无确认不能永久 cancel_requested：{progress}"
    );
    assert!(
        progress["error"]
            .as_str()
            .expect("明确原因")
            .contains("cancel_unconfirmed"),
        "明确未确认原因"
    );
    assert_eq!(
        f.call("get_task_progress", json!({"task_id":queued["task_id"]}))["state"],
        "queued",
        "未知停止不能丢弃或提交下一任务"
    );
    assert_eq!(
        f.call("directory", json!({}))["workspaces"][0]["cards"][0]["state"],
        "busy",
        "没有停止证据时仍显示忙碌"
    );
}
