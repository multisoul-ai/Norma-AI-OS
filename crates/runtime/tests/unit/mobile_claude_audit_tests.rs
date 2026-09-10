use super::*;
use norma::agent_hook::{parse_hook_event, HookApprovalRequest, HookSource};
use serde_json::json;
use std::time::Duration;
use storage::ApprovalAuditStore;

pub(super) fn fixture(store: ApprovalAuditStore) -> (Runtime, NodeId) {
    let (mut runtime, id) = launch_tests::runtime_with_card();
    // 80×24 = 标准测试终端，不启动真实 CLI；审批和 SQLite 均走正式入口。
    let launch = runtime
        .mobile_agent_launch(id, Path::new("unused"), 80, 24)
        .expect("预约")
        .with_approval_audit(store.clone());
    launch
        .record_session("session-a", "run-a")
        .expect("模拟后台准备，先登记准确身份");
    let factory: crate::PtyFactory =
        Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx))));
    let live = TermLive::spawn_with_factory(
        launch.generation,
        PtyCommand::new("unused"),
        80,
        24,
        &factory,
    )
    .expect("测试终端");
    runtime
        .attach_mobile_live(launch, "session-a".into(), "run-a".into(), live)
        .expect("附着");
    runtime.mobile_runs.get_mut(&id).expect("run").claude =
        Some(claude::ClaudeApprovals::new(store).expect("审计准备"));
    let event = parse_hook_event(
        HookSource::Claude,
        id,
        json!({
            "hook_event_name":"UserPromptSubmit", "session_id":"session-a", "prompt_id":"turn-a",
            "_norma_control_run_id":"run-a"
        }),
    )
    .expect("首条消息");
    assert!(
        runtime.observe_mobile_hook(&event),
        "真实消息应开始当前回合"
    );
    (runtime, id)
}

pub(super) fn request(id: NodeId) -> HookApproval {
    HookApproval::new(HookApprovalRequest {
        run_id:"run-a".into(),
        event:parse_hook_event(HookSource::Claude, id, json!({
            "hook_event_name":"PermissionRequest", "session_id":"session-a", "prompt_id":"turn-a",
            "tool_name":"Bash", "tool_input":{"command":"printf exact"}
        })).expect("权限事件")
    }, Instant::now()).expect("审批调用")
}

/// Claude 的选择须先持久化，数据库阻塞时不能先向 Hook 发出 Allow。
///
/// 数据构造：独立项目数据库、一个 echo PTY 和一个准确绑定 run-a/session-a/turn-a 的 Bash 请求。
/// 执行过程：SQLite BEGIN IMMEDIATE 锁写入 → 接受与选择 → 读取 Hook 响应 → 解锁 → drain → 检查回执与审计。
/// 预期结果：锁定期间无响应，错误运行与重复选择失败；提交后只响应一次，审计保留 pending/chosen/submitted/responded。
#[test]
fn claude_choice_waits_for_durable_project_audit() {
    let directory = tempfile::tempdir().expect("临时目录");
    let path = directory.path().join("project.db");
    let store = ApprovalAuditStore::new(path.clone());
    let (mut runtime, id) = fixture(store.clone());
    let connection = rusqlite::Connection::open(path).expect("锁连接");
    connection.execute_batch("BEGIN IMMEDIATE").expect("锁定");
    let approval = request(id);
    assert!(
        runtime.accept_mobile_approval(approval.clone()),
        "当前请求应接受"
    );
    assert!(
        !runtime.decide_mobile_approval(id, "old-run", approval.id(), ApprovalDecision::Allow),
        "错误运行不能选择"
    );
    assert!(
        runtime.decide_mobile_approval(id, "run-a", approval.id(), ApprovalDecision::Allow),
        "首次选择应接受"
    );
    assert!(
        !runtime.decide_mobile_approval(id, "run-a", approval.id(), ApprovalDecision::Deny),
        "重复选择不得覆盖"
    );
    runtime.drain_all();
    assert!(
        approval.take_response(Instant::now()).is_none(),
        "提交成功之前，Hook 不能消费批准"
    );
    connection.execute_batch("COMMIT").expect("解锁");
    // 2s = 本地 writer 轮询 200 次 × 10ms，非 Provider 超时。
    let deadline = Instant::now() + Duration::from_secs(2);
    let output = loop {
        runtime.drain_all();
        if let Some(output) = approval.take_response(Instant::now()) {
            break output;
        }
        assert!(Instant::now() < deadline, "落盘后应响应 Hook");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(
        output["hookSpecificOutput"]["decision"]["behavior"], "allow",
        "精确单次批准"
    );
    assert!(
        approval.take_response(Instant::now()).is_none(),
        "不能重复响应"
    );
    runtime.drain_all();
    let rows = loop {
        let rows = store.list_run("run-a", 0, 20).expect("读取项目审计");
        if rows.len() >= 4 {
            break rows;
        }
        assert!(Instant::now() < deadline, "四阶段应落盘");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(rows[0].payload["state"], "pending", "记录真实请求");
    assert_eq!(
        rows[1].payload["state"],
        json!({"chosen":"allow"}),
        "先保存选择"
    );
    assert_eq!(
        rows[2].payload["state"],
        json!({"decided":"allow"}),
        "提交后才交给 Hook"
    );
    assert_eq!(
        rows[3].payload["state"],
        json!({"responded":"allow"}),
        "只记录本地 transport 消费，不推断工具执行"
    );
    assert!(
        store
            .list_run("old-run", 0, 20)
            .expect("查旧运行")
            .is_empty(),
        "不能串写旧运行"
    );
}

/// 桌面断开、超时和回合结束均撤销尚未落盘的选择，新回合仍可正常审批。
///
/// 数据构造：三次独立 Hook 调用，共用一个 run；每次锁定写入再选 Allow。
/// 执行过程：第一项被桌面撤销；第二项推进到 120s TTL；第三项收到 Stop → 解锁 → 验证审计 → 新回合 Deny。
/// 预期结果：三项均不发送 Allow，审计保留取消；旧回合不得重新批准，新回合仅输出 Deny。
#[test]
fn claude_late_commit_cannot_revive_desktop_timeout_or_ended_turn() {
    let directory = tempfile::tempdir().expect("临时目录");
    let path = directory.path().join("project.db");
    let store = ApprovalAuditStore::new(path.clone());
    let (mut runtime, id) = fixture(store.clone());
    let connection = rusqlite::Connection::open(path).expect("锁连接");
    let desktop = request(id);
    connection.execute_batch("BEGIN IMMEDIATE").expect("锁定");
    assert!(
        runtime.accept_mobile_approval(desktop.clone()),
        "接受桌面竞争项"
    );
    assert!(
        runtime.decide_mobile_approval(id, "run-a", desktop.id(), ApprovalDecision::Allow),
        "暂存选择"
    );
    desktop.cancel(); // Transport EOF/桌面先处理走同一原子撤销。
    connection.execute_batch("COMMIT").expect("解锁");
    wait_rows(&mut runtime, &store, 3);
    assert_eq!(
        desktop.status(Instant::now()),
        ApprovalStatus::Cancelled,
        "迟到提交不得恢复桌面已撤销请求"
    );
    assert!(
        desktop.take_response(Instant::now()).is_none(),
        "不得发送桌面旧批准"
    );

    let expired = request(id);
    connection.execute_batch("BEGIN IMMEDIATE").expect("锁定");
    assert!(
        runtime.accept_mobile_approval(expired.clone()),
        "接受时未过期"
    );
    assert!(
        runtime.decide_mobile_approval(id, "run-a", expired.id(), ApprovalDecision::Allow),
        "过期前选择"
    );
    // 用域时钟推进到 now + Hook TTL(120s)，不改变生产超时、不真实等待两分钟。
    assert_eq!(
        expired.status(Instant::now() + norma::agent_hook::APPROVAL_TIMEOUT),
        ApprovalStatus::Cancelled,
        "Chosen 也应在期限结束时取消"
    );
    connection.execute_batch("COMMIT").expect("解锁");
    wait_rows(&mut runtime, &store, 6);
    assert!(
        expired.take_response(Instant::now()).is_none(),
        "过期后晚到提交不能发出批准"
    );

    let ended = request(id);
    connection.execute_batch("BEGIN IMMEDIATE").expect("锁定");
    assert!(
        runtime.accept_mobile_approval(ended.clone()),
        "接受结束前的请求"
    );
    assert!(
        runtime.decide_mobile_approval(id, "run-a", ended.id(), ApprovalDecision::Allow),
        "结束前选择"
    );
    let mut stop = ended.request().event.clone();
    stop.provider_event = "Stop".into();
    stop.control_run_id = Some("run-a".into());
    assert!(
        runtime.observe_mobile_hook(&stop),
        "准确 Stop 应取消回合审批"
    );
    connection.execute_batch("COMMIT").expect("解锁");
    let rows = wait_rows(&mut runtime, &store, 9);
    assert_eq!(rows[2].payload["state"], "cancelled", "桌面取消应落盘");
    assert_eq!(rows[5].payload["state"], "cancelled", "超时取消应落盘");
    assert_eq!(rows[8].payload["state"], "cancelled", "结束取消应落盘");
    assert!(
        !rows
            .iter()
            .any(|r| r.payload["state"].get("decided").is_some()),
        "三个选择都不得提交到 Hook"
    );
    assert!(
        ended.take_response(Instant::now()).is_none(),
        "Stop 后不得响应旧请求"
    );
    assert!(
        !runtime.decide_mobile_approval(id, "run-a", ended.id(), ApprovalDecision::Allow),
        "旧请求不能重新批准"
    );
    stop.provider_event = "UserPromptSubmit".into();
    stop.turn_id = Some("turn-b".into());
    assert!(runtime.observe_mobile_hook(&stop), "新回合可启动");
    let mut next = request(id).request().clone();
    next.event.turn_id = Some("turn-b".into());
    let next = HookApproval::new(next, Instant::now()).expect("新调用");
    assert!(
        runtime.accept_mobile_approval(next.clone()),
        "新回合仍可请求"
    );
    assert!(
        runtime.decide_mobile_approval(id, "run-a", next.id(), ApprovalDecision::Deny),
        "新回合可以拒绝"
    );
    wait_rows(&mut runtime, &store, 12);
    assert_eq!(
        next.take_response(Instant::now()).expect("已提交")["hookSpecificOutput"]["decision"]
            ["behavior"],
        "deny",
        "只拒绝当前调用"
    );
    assert!(
        desktop.take_response(Instant::now()).is_none(),
        "新回合不能复活旧调用"
    );
}

fn wait_rows(
    runtime: &mut Runtime,
    store: &ApprovalAuditStore,
    count: usize,
) -> Vec<storage::ApprovalAuditEntry> {
    // 2s = 200 次本地 writer 轮询 × 10ms；读取最多 20 条，覆盖本用例四阶段 × 三项。
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        runtime.drain_all();
        let rows = store.list_run("run-a", 0, 20).expect("审计读取");
        if rows.len() >= count {
            return rows;
        }
        assert!(
            Instant::now() < deadline,
            "审计应在本地预算内写入 {count} 条"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// 数据库拒绝提交时必须结束受控运行，关闭窗口也不能让旧回执继续发送。
///
/// 数据构造：SQLite 触发器只拒绝 Chosen 审计，另一个独立 Runtime 用写锁延迟选择。
/// 执行过程：选择 Allow → 提交失败 → drain；第二个 Runtime 选择 Allow → kill_all → 解锁与读取审计。
/// 预期结果：第一项 Failed 且没有响应，第二项 Cancelled 且保留撤销审计，两个运行都不能再审批。
#[test]
fn claude_failed_commit_and_close_never_release_a_choice() {
    let directory = tempfile::tempdir().expect("临时目录");
    let path = directory.path().join("failure.db");
    let store = ApprovalAuditStore::new(path.clone());
    let (mut runtime, id) = fixture(store);
    let connection = rusqlite::Connection::open(path).expect("数据库");
    connection.execute_batch("CREATE TRIGGER reject_choice BEFORE INSERT ON approval_audit WHEN json_extract(NEW.entry_json,'$.payload.state.chosen') IS NOT NULL BEGIN SELECT RAISE(ABORT,'test audit rejection'); END").expect("失败注入");
    let approval = request(id);
    assert!(
        runtime.accept_mobile_approval(approval.clone()),
        "请求应接受"
    );
    assert!(
        runtime.decide_mobile_approval(id, "run-a", approval.id(), ApprovalDecision::Allow),
        "选择进入后台"
    );
    let deadline = Instant::now() + Duration::from_secs(2); // 同本地 writer 的 200 × 10ms 预算。
    while runtime.mobile_agent_run_id(id).is_some() {
        runtime.drain_all();
        assert!(Instant::now() < deadline, "提交错误必须终止受控运行");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        matches!(
            runtime.agent_task_state(id),
            Some(crate::AgentTaskState::Failed { .. })
        ),
        "不能把写失败当成任务完成"
    );
    assert!(
        approval.take_response(Instant::now()).is_none(),
        "失败选择不能被消费"
    );
    assert!(
        !runtime.decide_mobile_approval(id, "run-a", approval.id(), ApprovalDecision::Allow),
        "失败运行不能重试旧决定"
    );

    let path = directory.path().join("close.db");
    let store = ApprovalAuditStore::new(path.clone());
    let (mut runtime, id) = fixture(store.clone());
    let connection = rusqlite::Connection::open(path).expect("数据库");
    connection.execute_batch("BEGIN IMMEDIATE").expect("锁写入");
    let closed = request(id);
    assert!(
        runtime.accept_mobile_approval(closed.clone()),
        "接受关闭前请求"
    );
    assert!(
        runtime.decide_mobile_approval(id, "run-a", closed.id(), ApprovalDecision::Allow),
        "保存关闭前选择"
    );
    runtime.kill_all();
    connection.execute_batch("COMMIT").expect("释放锁");
    let rows = wait_rows(&mut runtime, &store, 3);
    assert_eq!(rows[2].payload["state"], "cancelled", "关闭审计应记录撤销");
    assert!(
        closed.take_response(Instant::now()).is_none(),
        "关闭后不得消费选择"
    );
    assert!(
        runtime.mobile_agent_run_id(id).is_none(),
        "关闭后旧运行不存在"
    );
}

/// Claude终态必须绑定当前prompt；已完成旧回合不会被后续失败或迟到Hook覆盖。
///
/// 数据构造：run-a/session-a的turn-a正常Stop，turn-b为StopFailure，stale为非当前回合。
/// 执行过程：注入准确Hook → 开始b → 注入旧a/错stale → b失败 → 关闭runtime → 等后台落盘。
/// 预期结果：a完成、b失败独立保存；stale无结果，关卡不改变已存回合。
#[test]
fn claude_turn_outcomes_keep_exact_prompt_identity() {
    let directory = tempfile::tempdir().expect("临时目录");
    let path = directory.path().join("project.db");
    let (mut runtime, node) = fixture(ApprovalAuditStore::new(path.clone()));
    let mut observe = |name: &str, turn: &str| {
        let event = parse_hook_event(HookSource::Claude, node, json!({
            "hook_event_name":name,"session_id":"session-a","prompt_id":turn,"_norma_control_run_id":"run-a"
        })).expect("Hook事件");
        runtime.observe_mobile_hook(&event)
    };
    assert!(observe("Stop", "turn-a"), "当前a应结束");
    assert!(observe("UserPromptSubmit", "turn-b"), "b应开始");
    assert!(!observe("StopFailure", "turn-a"), "旧a不能覆盖当前b");
    assert!(!observe("Stop", "stale"), "错误回合不能完成");
    assert!(observe("StopFailure", "turn-b"), "当前b应失败");
    runtime.kill_all();
    let store = storage::MobileTurnStore::new(path);
    let key = |turn: &str| storage::MobileTurnKey {
        node_id: node,
        profile: "Claude".into(),
        run_id: "run-a".into(),
        session_id: "session-a".into(),
        turn_id: turn.into(),
    };
    // 2s = 200 × 10ms后台提交等待；只有真实SQLite终态可退出。
    let deadline = Instant::now() + Duration::from_secs(2);
    while store.get(&key("turn-b")).expect("读取b").is_none() {
        assert!(Instant::now() < deadline, "Provider终态必须由后台持久化");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        store.get(&key("turn-a")).expect("读取a").map(|r| r.state),
        Some(storage::MobileTurnState::Completed),
        "a必须保留完成"
    );
    assert_eq!(
        store.get(&key("turn-b")).expect("读取b").map(|r| r.state),
        Some(storage::MobileTurnState::Failed),
        "b保存自身失败"
    );
    assert!(
        store.get(&key("stale")).expect("读取错回合").is_none(),
        "错误回合无完成记录"
    );
}
