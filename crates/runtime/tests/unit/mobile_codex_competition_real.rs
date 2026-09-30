use super::real_tests::{approval, completed, exact_command, task};
use super::*;

// Called by the existing real five-turn check, sharing its owned host, native TUI and cleanup.
pub(super) fn verify_competition(
    runtime: &mut Runtime,
    id: NodeId,
    directory: &Path,
    socket: &Path,
    session: &str,
    run: &str,
    store: &storage::ApprovalAuditStore,
) {
    let peer = CodexConnection::connect(socket).expect("独立桌面协议连接");
    CodexHost::rpc(&peer, "thread/resume", json!({"threadId":session})).expect("附着同一真实会话");
    for phase in ["desktop-first", "commit-race"] {
        let target = directory.join(format!("{phase}.txt"));
        let command = format!("printf hello-from-norma > {}", target.display());
        let host = runtime.mobile_runs[&id].codex.as_ref().expect("host");
        let turn = CodexHost::rpc(
            &host.connection,
            "turn/start",
            json!({"threadId":session,
            "input":[{"type":"text","text":task(&command)}]}),
        )
        .expect("竞争回合");
        let pending = approval(runtime, id, turn["turn"]["id"].as_str().expect("回合 ID"));
        exact_command(&pending, &command);
        let remote = wait_peer(&peer, |event| {
            event["method"] == pending.method && event["params"]["turnId"] == pending.turn_id
        });
        assert_eq!(
            remote["id"], pending.provider_id,
            "两端必须看到同一个 Provider 请求"
        );
        assert_eq!(
            remote["params"]["itemId"], pending.item_id,
            "工具项不能串线"
        );
        let lock = if phase == "commit-race" {
            let connection =
                rusqlite::Connection::open(directory.join("audit.sqlite")).expect("审计锁连接");
            // 2s = SQLite 默认短事务锁等待余量；只在已取得请求后持锁，不等待模型生成。
            connection
                .busy_timeout(Duration::from_secs(2))
                .expect("锁等待上限");
            connection
                .execute_batch("BEGIN IMMEDIATE")
                .expect("延迟写入提交");
            runtime
                .decide_mobile_codex_approval(id, run, &pending.id, ApprovalDecision::Allow)
                .expect("手机先暂存批准");
            Some(connection)
        } else {
            None
        };
        let denial = pending
            .response(ApprovalDecision::Deny)
            .expect("实际提供的拒绝选项");
        peer.respond(remote["id"].clone(), denial.clone())
            .expect("桌面独立连接先拒绝");
        wait_peer(&peer, |event| {
            event["method"] == "serverRequest/resolved"
                && event["params"]["requestId"] == pending.provider_id
        });
        // 5s = 两个 WS 消费线程和 SQLite 回执的本地观察预算，不等待新的模型调用。
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            runtime.drain_all();
            if !runtime
                .mobile_codex_approvals(id)
                .expect("读取同步后请求")
                .iter()
                .any(|p| p.id == pending.id)
            {
                break;
            }
            assert!(Instant::now() < deadline, "Runtime 应收到另一端的 resolved");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            runtime
                .decide_mobile_codex_approval(id, run, &pending.id, ApprovalDecision::Allow)
                .is_err(),
            "桌面解决后不得再接收手机批准"
        );
        if let Some(lock) = lock {
            lock.execute_batch("COMMIT").expect("放开审计提交");
        }
        completed(
            runtime,
            id,
            if denial["decision"] == "cancel" {
                AgentTaskState::Ended
            } else {
                AgentTaskState::Done
            },
        );
        assert!(
            !target.exists(),
            "{phase}：迟到批准或迟到提交绝不能执行目标命令"
        );
        let rows = loop {
            runtime.drain_all();
            let rows = store.list_run(run, 0, 256).expect("审计读取");
            if rows.iter().any(|r| {
                r.request_id == pending.id
                    && matches!(r.payload["state"].as_str(), Some("resolved" | "cancelled"))
            }) {
                break rows;
            }
            assert!(Instant::now() < deadline, "竞争终态应写入审计");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(
            !rows.iter().any(
                |r| r.request_id == pending.id && r.payload["state"].get("submitted").is_some()
            ),
            "另一端已解决的请求不能向 Provider 补发批准"
        );
        if phase == "commit-race" {
            assert!(
                rows.iter().any(|r| r.request_id == pending.id
                    && r.payload["state"] == json!({"chosen":"allow"})),
                "审计仍应保留先前接受的手机意图"
            );
        }
        std::fs::write(format!("/tmp/norma-ios-codex-{phase}-v12.json"),serde_json::to_vec_pretty(&json!({"turn":pending.turn_id,"request":pending.provider_id,"target_exists":target.exists(),"audit":rows.iter().filter(|r|r.request_id==pending.id).collect::<Vec<_>>()})).expect("证据JSON")).expect("保存测试证据");
        eprintln!("Codex {phase}: peer resolved; Runtime rejected late approval; target absent; no submitted audit");
    }
}

fn wait_peer(peer: &CodexConnection, matches: impl Fn(&Value) -> bool) -> Value {
    // 5s = 两端本地 WS 调度上限；模型请求已由原 helper 等待完成。
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let event = peer
            .events()
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("独立客户端应收到目标事件");
        if matches(&event) {
            return event;
        }
    }
}
