#![cfg(unix)]
use norma::agent_hook::ApprovalDecision;
use norma::codex::{CodexApprovalState, CodexApprovals};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

fn request(id: i64, thread: &str, turn: &str) -> Value {
    json!({"id":id,"method":"item/commandExecution/requestApproval","params":{
        "threadId":thread,"turnId":turn,"itemId":format!("item-{id}"),
        "command":"printf hello","availableDecisions":["accept","decline"]
    }})
}

/// 具体审批绑定运行/线程/回合，桌面先处理、重复响应、ID 复用和过期均不能再次批准。
///
/// 数据构造：run-a/thread-a/turn-a；请求 7、9，另一个线程的请求 8；120s 为审批有效期，121s 验证超过边界。
/// 执行过程：收请求 → 忽略错误线程 → 桌面先解决 7 → 拒绝 9 → 重复批准失败 → 新回合复用 7 → 超时。
/// 预期结果：只传一次精确 decline；旧本地身份不再生效；审计保留提交与 Provider 关闭两个不同事实。
#[test]
fn decisions_are_once_per_live_request_and_keep_resolution_distinct() {
    let now = Instant::now();
    let mut approvals =
        CodexApprovals::new("run-a".into(), "thread-a".into(), Some("turn-a".into()));
    approvals
        .observe(&request(7, "thread-a", "turn-a"), now)
        .expect("应接收当前请求");
    approvals
        .observe(&request(8, "thread-b", "turn-a"), now)
        .expect("其他线程应被忽略");
    let first = approvals.pending(now).expect("应可列举");
    assert_eq!(first.len(), 1, "其他电脑/线程的请求不能混入");
    assert_eq!(
        first[0].provider_id,
        json!(7),
        "必须保存真实 Provider request ID"
    );
    assert_eq!(first[0].run_id, "run-a", "请求必须绑定运行实例");
    let first_id = first[0].id.clone();
    approvals.observe(&json!({"method":"serverRequest/resolved","params":{"threadId":"thread-a","requestId":7}}),now).expect("应关闭桌面已处理请求");
    let mut sent = Vec::new();
    assert!(
        approvals
            .submit(&first_id, ApprovalDecision::Allow, now, |id, result| {
                sent.push((id, result));
                Ok(())
            })
            .is_err(),
        "桌面先处理后不能迟到批准"
    );
    approvals
        .observe(&request(9, "thread-a", "turn-a"), now)
        .expect("应接收另一独立请求");
    let second = approvals
        .pending(now)
        .expect("应列举")
        .pop()
        .expect("应有请求 9");
    approvals
        .choose(&second.id, ApprovalDecision::Deny, now)
        .expect("应保留待落盘决定");
    approvals
        .submit(&second.id, ApprovalDecision::Deny, now, |id, result| {
            sent.push((id, result));
            Ok(())
        })
        .expect("应提交拒绝");
    assert!(
        approvals
            .submit(&second.id, ApprovalDecision::Allow, now, |id, result| {
                sent.push((id, result));
                Ok(())
            })
            .is_err(),
        "一个请求只能提交一次决定"
    );
    assert_eq!(
        sent,
        vec![(json!(9), json!({"decision":"decline"}))],
        "不得错用客户端 ID、发送第二次或扩大为会话授权"
    );
    approvals.observe(&json!({"method":"serverRequest/resolved","params":{"threadId":"thread-a","requestId":9}}),now).expect("应记录 Provider 关闭");
    let changes = approvals.take_changes();
    assert!(
        changes.iter().any(|change| change.approval.id == second.id
            && change.state == CodexApprovalState::Submitted(ApprovalDecision::Deny)),
        "必须保留本地提交事实"
    );
    assert!(
        changes.iter().any(|change| change.approval.id == second.id
            && change.state == CodexApprovalState::Resolved),
        "Provider 关闭必须是另一条事实，不能冒称工具已执行"
    );
    approvals.observe(&json!({"method":"turn/started","params":{"threadId":"thread-a","turn":{"id":"turn-b"}}}),now).expect("应进入新回合");
    approvals
        .observe(&request(7, "thread-a", "turn-b"), now)
        .expect("Provider 可在新回合复用数值 ID");
    let reused = approvals
        .pending(now)
        .expect("应列举新请求")
        .pop()
        .expect("应有新请求");
    assert_ne!(reused.id, first_id, "相同 Provider ID 也必须有新本地身份");
    assert!(
        approvals
            .submit(&first_id, ApprovalDecision::Allow, now, |_, _| Ok(()))
            .is_err(),
        "旧卡片不能命中新请求"
    );
    // expired = creation(0s) + TTL(120s) + 1s beyond boundary.
    let expired = now + Duration::from_secs(121);
    assert!(
        approvals
            .submit(&reused.id, ApprovalDecision::Allow, expired, |_, _| Ok(()))
            .is_err(),
        "过期审批不能批准"
    );
    assert!(
        approvals
            .pending(expired)
            .expect("应可查看空列表")
            .is_empty(),
        "过期请求不得继续显示可操作"
    );
    approvals
        .observe(&request(7, "thread-a", "turn-b"), expired)
        .expect("重复帧应被忽略");
    assert!(
        approvals.pending(expired).expect("应可查看").is_empty(),
        "重复帧不能延长旧请求有效期"
    );
    approvals.observe(&json!({"method":"serverRequest/resolved","params":{"threadId":"thread-a","requestId":7}}),expired).expect("晚到的 Provider 关闭仍需记录");
    assert!(
        approvals
            .take_changes()
            .iter()
            .any(|change| change.approval.id == reused.id
                && change.state == CodexApprovalState::Resolved),
        "过期后仍须将 Provider 回执关联到原请求"
    );
}

/// 决定落盘前只锁定意图，落盘等待期间桌面解决或 TTL 到期时不能发送响应。
///
/// 数据构造：请求 1/2 在同一回合，120s 是原始请求 TTL；Chosen 不延长期限。
/// 执行过程：Pending 直接 submit 被拒绝 → choose Allow → 再选 Deny 失败 → Provider resolved → submit 失败；请求 2 等待超过 TTL。
/// 预期结果：Chosen 与 Submitted 分开；过期和已解决请求均没有触发发送回调。
#[test]
fn choice_needs_a_durable_receipt_and_revalidation_before_send() {
    let now = Instant::now();
    let mut approvals =
        CodexApprovals::new("run-a".into(), "thread-a".into(), Some("turn-a".into()));
    approvals
        .observe(&request(1, "thread-a", "turn-a"), now)
        .expect("应接收请求");
    let id = approvals.pending(now).expect("应可读")[0].id.clone();
    let mut sends = 0;
    assert!(
        approvals
            .submit(&id, ApprovalDecision::Allow, now, |_, _| {
                sends += 1;
                Ok(())
            })
            .is_err(),
        "没有先锁定并持久化决定不可直接发送"
    );
    approvals
        .choose(&id, ApprovalDecision::Allow, now)
        .expect("应锁定一次选择");
    assert!(
        approvals.choose(&id, ApprovalDecision::Deny, now).is_err(),
        "等待持久化期间不能改为另一决定"
    );
    assert!(
        approvals
            .take_changes()
            .iter()
            .any(|c| c.state == CodexApprovalState::Chosen(ApprovalDecision::Allow)),
        "必须记录尚未发送的意图"
    );
    approvals.observe(&json!({"method":"serverRequest/resolved","params":{"threadId":"thread-a","requestId":1}}),now).expect("桌面已处理");
    assert!(
        approvals
            .submit(&id, ApprovalDecision::Allow, now, |_, _| {
                sends += 1;
                Ok(())
            })
            .is_err(),
        "落盘期间已解决的请求不能发送"
    );
    approvals
        .observe(&request(2, "thread-a", "turn-a"), now)
        .expect("应接收第二条");
    let id = approvals.pending(now).expect("应可读")[0].id.clone();
    approvals
        .choose(&id, ApprovalDecision::Deny, now)
        .expect("应锁定拒绝");
    assert!(
        approvals
            .submit(
                &id,
                ApprovalDecision::Deny,
                now + Duration::from_secs(120),
                |_, _| {
                    sends += 1;
                    Ok(())
                }
            )
            .is_err(),
        "在原始 120s 边界到期的选择不得发送"
    );
    assert_eq!(sends, 0, "任何失效决定都不得触发发送回调");
}

/// Provider 只提供 cancel 时拒绝应结束回合；不得为了兼容而扩大为会话或持久授权。
///
/// 数据构造：0.153.4 实际选项 accept、execpolicy amendment、cancel；另构造只含 acceptForSession 的列表。
/// 执行过程：接收原始选项 → choose Deny → 提交 → 请求重复被拒绝；读取仅提供扩权的 Allow 响应。
/// 预期结果：准确发送 cancel；没有 accept/acceptForSession，不能凭空添加 Provider 未提供的选项。
#[test]
fn deny_uses_cancel_when_provider_does_not_offer_decline() {
    let now = Instant::now();
    let mut approvals =
        CodexApprovals::new("run-a".into(), "thread-a".into(), Some("turn-a".into()));
    let mut event = request(1, "thread-a", "turn-a");
    event["params"]["availableDecisions"] = json!(["accept",{"acceptWithExecpolicyAmendment":{"execpolicy_amendment":["echo"]}},"cancel"]);
    approvals.observe(&event, now).expect("应接收新版本请求");
    let approval = approvals.pending(now).expect("应可读").remove(0);
    assert_eq!(
        approval.response(ApprovalDecision::Deny).expect("应可拒绝"),
        json!({"decision":"cancel"}),
        "拒绝时须使用 Provider 支持的结束回合选项"
    );
    approvals
        .choose(&approval.id, ApprovalDecision::Deny, now)
        .expect("应锁定拒绝");
    let mut response = Value::Null;
    approvals
        .submit(&approval.id, ApprovalDecision::Deny, now, |_, value| {
            response = value;
            Ok(())
        })
        .expect("应提交单次拒绝");
    assert_eq!(
        response,
        json!({"decision":"cancel"}),
        "不得以扩大权限替代不受支持的 decline"
    );
    assert!(
        approvals
            .choose(&approval.id, ApprovalDecision::Allow, now)
            .is_err(),
        "拒绝后不可改为批准"
    );
    let mut broader = approval.clone();
    broader.params["availableDecisions"] = json!(["acceptForSession"]);
    assert!(
        broader.response(ApprovalDecision::Allow).is_err(),
        "只提供会话授权时不得冒充单次批准"
    );
    assert!(
        broader.response(ApprovalDecision::Deny).is_err(),
        "不能发送缺失的拒绝选项"
    );
}
