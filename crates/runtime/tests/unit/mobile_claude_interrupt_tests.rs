use super::*;
use norma::agent_hook::{parse_hook_event, HookSource};
use serde_json::json;
use std::io::Write;
use std::time::Duration;

fn fixture(dir: &Path) -> (Runtime, NodeId) {
    let (mut runtime, id) =
        claude_audit_tests::fixture(storage::ApprovalAuditStore::new(dir.join("audit.db")));
    std::fs::write(dir.join("session-a.jsonl"), "").expect("空历史");
    hook(&mut runtime, id, dir, "PermissionRequest", "turn-a");
    (runtime, id)
}

fn hook(runtime: &mut Runtime, id: NodeId, dir: &Path, name: &str, turn: &str) {
    let event = parse_hook_event(
        HookSource::Claude,
        id,
        json!({
            "hook_event_name":name, "session_id":"session-a", "prompt_id":turn,
            "_norma_control_run_id":"run-a", "transcript_path":dir.join("session-a.jsonl")
        }),
    )
    .expect("Hook");
    assert!(
        runtime.observe_mobile_hook(&event),
        "精确运行的 Hook 应被接受"
    );
}

fn marker(dir: &Path, turn: &str) {
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(dir.join("session-a.jsonl"))
        .expect("追加");
    writeln!(file, "{}", json!({"type":"user","uuid":turn,"sessionId":"session-a","cwd":"/tmp",
        "promptId":turn,"message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user for tool use]"}]}})).expect("中断记录");
}

/// 正式中断必须先锁定回合、捕获末尾并发送 Escape，最后等待新增 Provider 证据。
///
/// 数据构造：run-a/turn-a echo PTY，旧中断 1 条、另一回合新增标记 1 条、准确回合新增标记 1 条。
/// 执行过程：登记审批 → 请求中断 → 准备 → 发按键 → 扫描旧/错误回合 → 新增准确标记 → 完成。
/// 预期结果：旧运行、重复请求及迟到批准均拒绝；准确新增记录才结束并持久化interrupted；系统Escape不污染下一条消息。
#[test]
fn claude_interrupt_waits_for_new_matching_evidence() {
    let dir = tempfile::tempdir().expect("临时目录");
    let (mut runtime, id) = fixture(dir.path());
    let approval = claude_audit_tests::request(id);
    assert!(
        runtime.accept_mobile_approval(approval.clone()),
        "中断前正常接受审批"
    );
    assert!(
        runtime.decide_mobile_approval(id, "run-a", approval.id(), ApprovalDecision::Allow),
        "先暂存选择，尚未向 Hook 提交"
    );
    marker(dir.path(), "turn-a");
    assert!(
        runtime
            .interrupt_mobile_terminal(id, "old-run", "turn-a")
            .is_err(),
        "旧运行不能中断"
    );
    assert!(
        runtime
            .interrupt_mobile_terminal(id, "run-a", "old-turn")
            .is_err(),
        "旧回合不能中断"
    );
    let (mut request, receipt) = runtime
        .interrupt_mobile_terminal(id, "run-a", "turn-a")
        .expect("预约");
    assert!(
        runtime
            .interrupt_mobile_terminal(id, "run-a", "turn-a")
            .is_err(),
        "禁止重复请求"
    );
    assert!(
        runtime
            .send_mobile_terminal_interrupt(&mut request)
            .is_err(),
        "未捕获末尾不能发送"
    );
    assert_eq!(
        approval.status(Instant::now()),
        ApprovalStatus::Cancelled,
        "中断撤销未发审批"
    );
    assert!(
        !runtime.decide_mobile_approval(id, "run-a", approval.id(), ApprovalDecision::Allow),
        "迟到选择失败"
    );
    assert!(
        !runtime.accept_mobile_approval(claude_audit_tests::request(id)),
        "等待中断时不能接受新审批"
    );
    let terminal_interrupt::InterruptHistory::Claude { reader, .. } = &mut request.history else {
        panic!("Claude fixture reader")
    };
    reader.projects_root = dir.path().into();
    request.prepare().expect("后台捕获");
    assert!(request.poll().is_err(), "未发送前不得收集完成证据");
    runtime
        .send_mobile_terminal_interrupt(&mut request)
        .expect("发送 Escape");
    assert!(
        runtime
            .send_mobile_terminal_interrupt(&mut request)
            .is_err(),
        "按键不能重复发送"
    );
    assert!(receipt.try_recv().is_err(), "按键写入不代表任务已中断");
    assert!(!request.poll().expect("读取"), "旧标记不可使用");
    marker(dir.path(), "other-turn");
    assert!(!request.poll().expect("另一个回合"), "不能采用其他回合证据");
    marker(dir.path(), "turn-a");
    assert!(
        request.poll().expect("准确证据"),
        "新增准确标记才能证明中断"
    );
    assert!(
        runtime.finish_mobile_terminal_interrupt(&request, None),
        "前台确认终态"
    );
    assert_eq!(
        receipt.try_recv().expect("回执").expect("成功")["status"],
        "interrupted",
        "返回实际中断"
    );
    let outcomes = storage::MobileTurnStore::new(dir.path().join("audit.db"));
    let key = storage::MobileTurnKey {
        node_id: id,
        profile: "Claude".into(),
        run_id: "run-a".into(),
        session_id: "session-a".into(),
        turn_id: "turn-a".into(),
    };
    // 2s = 200 × 10ms后台commit等待；新增准确中断证据必须落盘，不能只保留瞬时UI状态。
    let deadline = Instant::now() + Duration::from_secs(2);
    while outcomes.get(&key).expect("查询中断终态").is_none() {
        assert!(Instant::now() < deadline, "准确中断必须后台持久化");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        outcomes.get(&key).expect("读取中断").map(|r| r.state),
        Some(storage::MobileTurnState::Interrupted),
        "不能把用户停止记为完成或模型失败"
    );
    assert_eq!(
        runtime.validate_mobile_message(&runtime.mobile_card_snapshots().remove(0), "next message"),
        Ok(()),
        "系统中断按键不能伪装成未提交的电脑草稿，导致下一条手机消息永久被拒"
    );
    assert_eq!(
        runtime.agent_task_state(id),
        Some(crate::AgentTaskState::Ended),
        "等待态必须结束"
    );
    assert_eq!(runtime.mobile_runs[&id].prompt_id, None, "已结束回合解绑");
    assert_eq!(
        runtime.live[&id].terminal_status(),
        Some(TerminalStatus::Running),
        "保留原生终端"
    );
    assert!(
        !runtime.finish_mobile_terminal_interrupt(&request, None),
        "不能重复完成"
    );
}

/// 准备期间收到新回合或自然结束时，旧按键和旧结果都不能覆盖新状态。
///
/// 数据构造：一个 echo 会话先后 turn-a/b/c，分别在准备中换回合、自然 Stop、发送后 StopFailure。
/// 执行过程：捕获末尾 → 新回合 → 尝试旧发送；新请求 → Stop；第三次请求发送 → StopFailure。
/// 预期结果：换回合返回错误且不发送；自然 Stop 返回 completed，失败返回 failed；旧请求不能终结新回合。
#[test]
fn claude_interrupt_revalidates_turn_and_reports_natural_completion() {
    let dir = tempfile::tempdir().expect("目录");
    let (mut runtime, id) = fixture(dir.path());
    let (mut old, receipt) = runtime
        .interrupt_mobile_terminal(id, "run-a", "turn-a")
        .expect("请求");
    let terminal_interrupt::InterruptHistory::Claude { reader, .. } = &mut old.history else {
        panic!("Claude fixture reader")
    };
    reader.projects_root = dir.path().into();
    old.prepare().expect("准备");
    hook(&mut runtime, id, dir.path(), "UserPromptSubmit", "turn-b");
    assert!(
        receipt.try_recv().expect("换回合回执").is_err(),
        "换回合不是成功中断"
    );
    assert!(
        runtime.send_mobile_terminal_interrupt(&mut old).is_err(),
        "不得向新回合发送旧 Escape"
    );
    assert!(
        !runtime.finish_mobile_terminal_interrupt(&old, None),
        "旧证据不能修改新回合"
    );
    assert_eq!(
        runtime.mobile_runs[&id].prompt_id.as_deref(),
        Some("turn-b"),
        "新回合保留"
    );
    let (_request, receipt) = runtime
        .interrupt_mobile_terminal(id, "run-a", "turn-b")
        .expect("第二次");
    hook(&mut runtime, id, dir.path(), "Stop", "turn-b");
    assert_eq!(
        receipt.try_recv().expect("结束回执").expect("成功")["status"],
        "completed",
        "自然完成不是 interrupted"
    );
    hook(&mut runtime, id, dir.path(), "UserPromptSubmit", "turn-c");
    let (mut request, receipt) = runtime
        .interrupt_mobile_terminal(id, "run-a", "turn-c")
        .expect("第三次");
    let terminal_interrupt::InterruptHistory::Claude { reader, .. } = &mut request.history else {
        panic!("Claude fixture reader")
    };
    reader.projects_root = dir.path().into();
    request.prepare().expect("捕获");
    runtime
        .send_mobile_terminal_interrupt(&mut request)
        .expect("发出");
    hook(&mut runtime, id, dir.path(), "StopFailure", "turn-c");
    assert_eq!(
        receipt
            .try_recv()
            .expect("失败回执")
            .expect("Provider 终态")["status"],
        "failed",
        "必须如实返回失败"
    );
    assert!(request.poll().is_err(), "结束后的后台读取撤销");
}

/// 读失败、超时和关窗不能被误报为成功，后台请求丢失也不能永久锁住审批。
///
/// 数据构造：独立 JSONL 与一个活动回合；超时预算 30s = 既有启动等待上限。
/// 执行过程：发送后替换文件 → 读取失败 → 明确失败回执；新请求推进期限 → drain；第三次 kill_all。
/// 预期结果：前两次不清回合身份或伪造 Ended，允许再次请求；关闭后请求与历史均失效。
#[test]
fn claude_interrupt_failure_timeout_and_close_never_claim_success() {
    let dir = tempfile::tempdir().expect("目录");
    let (mut runtime, id) = fixture(dir.path());
    let (mut request, receipt) = runtime
        .interrupt_mobile_terminal(id, "run-a", "turn-a")
        .expect("请求");
    let terminal_interrupt::InterruptHistory::Claude { reader, .. } = &mut request.history else {
        panic!("Claude fixture reader")
    };
    reader.projects_root = dir.path().into();
    request.prepare().expect("捕获");
    runtime
        .send_mobile_terminal_interrupt(&mut request)
        .expect("发送");
    std::fs::rename(
        dir.path().join("session-a.jsonl"),
        dir.path().join("old.jsonl"),
    )
    .expect("替换");
    std::fs::write(dir.path().join("session-a.jsonl"), "").expect("新文件");
    let error = request.poll().expect_err("文件替换应失败");
    assert!(
        runtime.finish_mobile_terminal_interrupt(&request, Some(error)),
        "读取失败应有回执"
    );
    assert!(
        receipt.try_recv().expect("错误回执").is_err(),
        "不能报告成功"
    );
    assert_eq!(
        runtime.mobile_runs[&id].prompt_id.as_deref(),
        Some("turn-a"),
        "未知状态保留回合"
    );
    let (_request, receipt) = runtime
        .interrupt_mobile_terminal(id, "run-a", "turn-a")
        .expect("重试");
    // 将 30s 期限推进为 now - 1ms，无真实等待；drain 负责未被后台消费的超时。
    runtime
        .mobile_runs
        .get_mut(&id)
        .expect("run")
        .interrupt
        .as_mut()
        .expect("中断")
        .deadline = Instant::now() - Duration::from_millis(1);
    runtime.drain_all();
    assert!(
        receipt.try_recv().expect("超时回执").is_err(),
        "超时不等于成功"
    );
    let (abandoned, abandoned_receipt) = runtime
        .interrupt_mobile_terminal(id, "run-a", "turn-a")
        .expect("丢弃请求");
    drop(abandoned);
    runtime.drain_all();
    assert!(
        abandoned_receipt.try_recv().expect("丢弃回执").is_err(),
        "丢失后台工作不能永久锁住请求"
    );
    let (mut request, receipt) = runtime
        .interrupt_mobile_terminal(id, "run-a", "turn-a")
        .expect("关闭前请求");
    runtime.kill_all();
    assert!(
        receipt.try_recv().expect("关闭回执").is_err(),
        "关窗不是准确回合中断证据"
    );
    assert!(request.prepare().is_err(), "关闭后不得读取和发送");
    assert!(
        runtime
            .send_mobile_terminal_interrupt(&mut request)
            .is_err(),
        "关闭后不能发按键"
    );
}

/// 手机停止绑定运行身份，允许同回合的阶段和标题变化；系统按键保留真实电脑草稿。
///
/// 数据构造：run-a/session-a/turn-a；旧 run-b、session-b、turn-b；电脑草稿为 draft。
/// 执行过程：快照来自 thinking → 实际进入审批 → 校验准确/旧身份 → 输入草稿 → 预约并发送 → 检查快照及删除确认。
/// 预期结果：准确回合可停止，旧身份不可；等待态为 interrupting，不能重复停止或免确认删除；已有草稿不被清除。
#[test]
fn mobile_interrupt_identity_and_pending_state_preserve_desktop_draft() {
    let dir = tempfile::tempdir().expect("目录");
    let (mut runtime, id) = fixture(dir.path());
    runtime.events.update_agent_task_state(
        id,
        crate::AgentTaskState::WaitingApproval {
            tool_name: Some("permission".into()),
        },
        "test".into(),
    );
    let mut expected = runtime.mobile_card_snapshots().remove(0);
    expected.state = "thinking".into();
    expected.title = "previous title".into();
    assert_eq!(
        runtime.validate_mobile_interrupt(&expected),
        Ok(()),
        "同回合阶段和标题变化不得阻止停止"
    );
    let mut stale = expected.clone();
    stale.run_id = Some("run-b".into());
    assert_eq!(
        runtime.validate_mobile_interrupt(&stale),
        Err("card_changed"),
        "旧运行不得中断"
    );
    stale = expected.clone();
    stale.session_id = Some("session-b".into());
    assert_eq!(
        runtime.validate_mobile_interrupt(&stale),
        Err("card_changed"),
        "旧会话不得中断"
    );
    stale = expected.clone();
    stale.turn_id = Some("turn-b".into());
    assert_eq!(
        runtime.validate_mobile_interrupt(&stale),
        Err("card_changed"),
        "旧回合不得中断"
    );
    runtime
        .send_node_input(id, crate::NodeInput::Text("draft".into()))
        .expect("电脑草稿");
    let (mut request, _receipt) = runtime
        .interrupt_mobile_terminal(id, "run-a", "turn-a")
        .expect("预约");
    assert_eq!(
        runtime.validate_mobile_interrupt(&expected),
        Err("state_not_ready"),
        "请求中不能重复停止"
    );
    let current = runtime.mobile_card_snapshots().remove(0);
    assert_eq!(
        current.state, "interrupting",
        "请求等待证据时必须显示正在停止"
    );
    assert_eq!(
        runtime.validate_mobile_card_deletion(&current, false),
        Err("confirmation_required"),
        "停止未确认仍需确认删除运行卡"
    );
    assert_eq!(
        runtime.validate_mobile_card_deletion(&current, true),
        Ok(()),
        "明确确认可走既有关闭流程"
    );
    let terminal_interrupt::InterruptHistory::Claude { reader, .. } = &mut request.history else {
        panic!("Claude fixture reader")
    };
    reader.projects_root = dir.path().into();
    request.prepare().expect("捕获历史");
    runtime
        .send_mobile_terminal_interrupt(&mut request)
        .expect("发送停止");
    marker(dir.path(), "turn-a");
    assert!(request.poll().expect("读取"), "准确回合中断证据应可见");
    assert!(
        runtime.finish_mobile_terminal_interrupt(&request, None),
        "完成中断"
    );
    assert_eq!(
        runtime.validate_mobile_message(&runtime.mobile_card_snapshots().remove(0), "next"),
        Err("desktop_input_pending"),
        "停止不能清除电脑已有草稿"
    );
}
