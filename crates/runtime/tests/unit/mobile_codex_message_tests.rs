use super::*;

/// 新Codex回合已经接收、历史文件尚为空时，原生终端附着应等待刷新，不能杀掉已接收任务。
///
/// 数据构造：真实错误形状含rollout空文件及当前thread-a；对照另一个thread-b、既有会话与永久权限错误。
/// 执行过程：分类四条启动错误；重试实际仍受既有10秒附着期限约束。
/// 预期结果：仅当前新线程的短暂空文件或尚未发现历史允许等待；旧会话/其他线程/权限错误不得被吞掉。
#[test]
fn new_empty_rollout_is_retryable_only_for_the_new_thread() {
    let error = json!({"code":-32603,"message":"failed to read thread: rollout at /sessions/rollout-thread-a.jsonl is empty"}).to_string();
    assert!(
        CodexHost::native_history_pending(&error, "thread-a", true),
        "当前新历史尚未flush应等待而不是取消真实任务"
    );
    assert!(
        !CodexHost::native_history_pending(&error, "thread-a", false),
        "旧空会话不能冒充启动中的新历史"
    );
    assert!(
        !CodexHost::native_history_pending(&error, "thread-b", true),
        "另一线程的错误不能被吞掉"
    );
    assert!(
        !CodexHost::native_history_pending("permission denied", "thread-a", true),
        "永久错误立即失败"
    );
}

/// Codex普通消息不能覆盖运行回合，只有准确RPC回执确认发送，超时禁止另起重复提交。
///
/// 数据构造：正式私有WS fixture，turn-a完成后发送turn-b；第二条no-confirm故意不给RPC结果。
/// 执行过程：忙碌发送拒绝 → 完成a → 发送b/尝试重复 → 取得准确回执 → 完成b → 无回执提交 → 超时。
/// 预期结果：线上只收到两次turn/start；结果绑定run/thread/turn；超时结果未知，不能把第三条自动发送给Provider。
#[test]
fn codex_messages_wait_for_rpc_and_quarantine_unknown_outcomes() {
    let (mut runtime, node, _directory, server) = super::tests::setup();
    runtime
        .events
        .update_agent_task_state(node, AgentTaskState::Thinking, "test".into());
    let busy = runtime.mobile_card_snapshots().remove(0);
    assert!(
        runtime
            .send_mobile_message(&busy, "must-not-steer".into())
            .is_err(),
        "普通消息不得steer正在运行回合"
    );
    super::tests::advance(
        &runtime,
        node,
        vec![
            json!({"method":"turn/completed","params":{"threadId":"thread-a","turn":{"id":"turn-a","status":"completed"}}}),
        ],
    );
    runtime.drain_all();
    let ready = runtime.mobile_card_snapshots().remove(0);
    let receipt = runtime
        .send_mobile_message(&ready, "next task".into())
        .expect("空闲发送");
    assert!(
        runtime
            .send_mobile_message(&ready, "duplicate".into())
            .is_err(),
        "并发提交不得再派发"
    );
    // 5s is the fixture local transport bound; no model or real sleeps beyond bounded polling.
    let deadline = Instant::now() + Duration::from_secs(5);
    let result = loop {
        runtime.drain_all();
        if let Ok(result) = receipt.try_recv() {
            break result.expect("真实RPC确认");
        }
        assert!(Instant::now() < deadline, "本地RPC必须及时结束");
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(result["turn_id"], "turn-b", "不能使用旧turn-a");
    assert_eq!(result["session_id"], "thread-a", "准确会话");
    assert_eq!(result["run_id"], "run-a", "准确运行");
    super::tests::advance(
        &runtime,
        node,
        vec![
            json!({"method":"turn/completed","params":{"threadId":"thread-a","turn":{"id":"turn-b","status":"completed"}}}),
        ],
    );
    runtime.drain_all();
    let ready = runtime.mobile_card_snapshots().remove(0);
    let unknown = runtime
        .send_mobile_message(&ready, "no-confirm".into())
        .expect("不确定提交");
    let requests = super::tests::advance(&runtime, node, vec![]);
    // 31 = 30s deadline + 1s to cross the boundary without waiting.
    runtime.advance_mobile_messages(Instant::now() + Duration::from_secs(31));
    assert!(
        unknown.try_recv().expect("明确超时").is_err(),
        "无RPC结果不能成功"
    );
    assert!(
        runtime
            .send_mobile_message(
                &runtime.mobile_card_snapshots().remove(0),
                "must-not-replay".into()
            )
            .is_err(),
        "未知的Provider状态不能立刻重发"
    );
    assert_eq!(
        requests.as_array().expect("记录").len(),
        2,
        "线上仅两个实际提交，不含busy/duplicate/replay"
    );
    assert_eq!(
        requests[0]["params"]["input"][0]["text"], "next task",
        "原文保持"
    );
    assert_eq!(
        requests[1]["params"]["input"][0]["text"], "no-confirm",
        "第二条明确输入"
    );
    runtime.kill_all();
    assert_eq!(
        server.join().expect("回收Provider").len(),
        2,
        "关闭不能重放未确认消息"
    );
}

/// 同一次Codex事件批次的两个终态按实际turn保存，不能使用drain后的当前卡片推断。
///
/// 数据构造：turn-a完成、turn-b开始后失败、另一thread的turn-c完成事件是负例。
/// 执行过程：正式私有WS推送一批事件 → runtime drain → 关闭 → 从项目SQLite读取两个独立回合。
/// 预期结果：a完成、b失败，错线程c与没有结束事件的回合均没有成功记录。
#[test]
fn codex_turn_outcomes_capture_each_provider_turn_before_state_changes() {
    let (mut runtime, node, directory, server) = super::tests::setup();
    super::tests::advance(
        &runtime,
        node,
        vec![
            json!({"method":"turn/completed","params":{"threadId":"thread-a","turn":{"id":"turn-a","status":"completed"}}}),
            json!({"method":"turn/started","params":{"threadId":"thread-a","turn":{"id":"turn-b"}}}),
            json!({"method":"turn/completed","params":{"threadId":"thread-a","turn":{"id":"turn-b","status":"failed"}}}),
            json!({"method":"turn/completed","params":{"threadId":"thread-other","turn":{"id":"turn-c","status":"completed"}}}),
        ],
    );
    runtime.drain_all();
    runtime.kill_all();
    server.join().expect("回收WS");
    let store = storage::MobileTurnStore::new(directory.path().join("audit.sqlite"));
    let key = |turn: &str| storage::MobileTurnKey {
        node_id: node,
        profile: "Codex".into(),
        run_id: "run-a".into(),
        session_id: "thread-a".into(),
        turn_id: turn.into(),
    };
    // 2s = 本地200次 × 10ms写入等待，无真实Provider超时。
    let deadline = Instant::now() + Duration::from_secs(2);
    while store.get(&key("turn-b")).expect("读取b").is_none() {
        assert!(Instant::now() < deadline, "精确终态必须进入后台持久化");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        store.get(&key("turn-a")).expect("读取a").map(|r| r.state),
        Some(storage::MobileTurnState::Completed),
        "a不被后续失败覆盖"
    );
    assert_eq!(
        store.get(&key("turn-b")).expect("读取b").map(|r| r.state),
        Some(storage::MobileTurnState::Failed),
        "保存b准确失败"
    );
    assert!(
        store.get(&key("turn-c")).expect("错线程回合").is_none(),
        "不能接受其他thread的终态"
    );
    assert!(
        store.get(&key("unobserved")).expect("无证据回合").is_none(),
        "不能根据卡片Done推断未知回合"
    );
}
