use norma::agent_hook::ApprovalDecision;
use norma::mobile::ApprovalTarget;
use serde_json::json;

/// 审批回执区分本地选择、传输提交和请求关闭，并核验完整来源及具体决定。
///
/// 数据构造：node7/run-a/session-a/turn-a；Codex用run-a:1，Claude使用独立request-a；审计阶段按真实格式编码。
/// 执行过程：pending/chosen → 等待；submitted/responded → 本地已提交；resolved/cancelled → 关闭；改错会话/决定 → 拒绝。
/// 预期结果：不能以chosen或Provider resolved推断工具执行；跨会话与相反决定不得确认成功。
#[test]
fn approval_delivery_requires_exact_identity_and_transport_stage() {
    let codex: ApprovalTarget = serde_json::from_value(json!({"node_id":7,"profile":"Codex","run_id":"run-a","session_id":"session-a","turn_id":"turn-a","request_id":"run-a:1"})).expect("Codex目标");
    assert!(codex.valid(), "带冒号的唯一Codex调用合法");
    let mut row = json!({"approval":{"id":"run-a:1","run_id":"run-a","thread_id":"session-a","turn_id":"turn-a"},"state":"pending"});
    assert_eq!(
        codex.delivery_state(&row, ApprovalDecision::Allow),
        Ok("waiting"),
        "pending不是已提交"
    );
    row["state"] = json!({"chosen":"allow"});
    assert_eq!(
        codex.delivery_state(&row, ApprovalDecision::Allow),
        Ok("waiting"),
        "已持久化选择不能冒充提交"
    );
    row["state"] = json!({"submitted":"allow"});
    assert_eq!(
        codex.delivery_state(&row, ApprovalDecision::Allow),
        Ok("submitted"),
        "准确传输记录确认本地提交"
    );
    assert!(
        codex.delivery_state(&row, ApprovalDecision::Deny).is_err(),
        "不能拿批准回执确认拒绝"
    );
    row["approval"]["thread_id"] = json!("wrong-session");
    assert!(
        codex.delivery_state(&row, ApprovalDecision::Allow).is_err(),
        "同请求ID另一会话也不可信"
    );
    row["approval"]["thread_id"] = json!("session-a");
    row["state"] = json!("resolved");
    assert_eq!(
        codex.delivery_state(&row, ApprovalDecision::Allow),
        Ok("request_closed"),
        "resolved不揭示获胜者，不能显示决定已采用"
    );
    let mut claude = codex.clone();
    claude.profile = "Claude".into();
    claude.request_id = "request-a".into();
    let mut hook = json!({"id":"request-a","request":{"run_id":"run-a","event":{"node_id":7,"session_id":"session-a","turn_id":"turn-a"}},"state":{"decided":"deny"}});
    assert_eq!(
        claude.delivery_state(&hook, ApprovalDecision::Deny),
        Ok("waiting"),
        "尚未被Hook消费时继续等待"
    );
    hook["state"] = json!({"responded":"deny"});
    assert_eq!(
        claude.delivery_state(&hook, ApprovalDecision::Deny),
        Ok("submitted"),
        "Hook实际消费后可确认本地提交"
    );
    hook["state"] = json!("cancelled");
    assert_eq!(
        claude.delivery_state(&hook, ApprovalDecision::Allow),
        Ok("request_closed"),
        "取消不能被推断为批准或拒绝"
    );
    hook["request"]["event"]["node_id"] = json!(8);
    assert!(
        claude
            .delivery_state(&hook, ApprovalDecision::Deny)
            .is_err(),
        "不能读取另一张卡片的回执"
    );
}
