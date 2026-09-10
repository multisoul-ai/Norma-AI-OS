use norma::agent_hook::{
    parse_hook_event, ApprovalDecision, ApprovalStatus, HookApproval, HookApprovalRequest,
    HookSource,
};
use serde_json::json;
use std::time::{Duration, Instant};

fn request() -> HookApprovalRequest {
    HookApprovalRequest {
        run_id: "run-a".into(),
        event: parse_hook_event(
            HookSource::Claude,
            7,
            json!({
                "hook_event_name": "PermissionRequest", "session_id": "session-a",
                "prompt_id": "turn-a", "tool_name": "Bash", "tool_input": {"command": "pwd"}
            }),
        )
        .expect("审批事件应可解析"),
    }
}

/// 一次 Hook 调用只接受一个决定，重复、迟到、取消后的批准均不得生效。
///
/// 数据构造：同内容请求两次，代表两个不同 Hook 调用；时限 = 120 秒，取产品审批等待上限。
/// 执行过程：创建两请求 → 第一条拒绝并重复批准 → 第二条取消并迟到批准 → 检查状态与输出。
/// 预期结果：身份不同，第一条仅输出拒绝，第二条无授权输出；Claude prompt_id 保留为回合身份。
#[test]
fn invocation_identity_and_single_decision_are_preserved() {
    let now = Instant::now();
    let first = HookApproval::new(request(), now).expect("有效审批应可创建");
    let second = HookApproval::new(request(), now).expect("相同内容的另一调用应可创建");
    assert_ne!(first.id(), second.id(), "不能按命令内容合并不同调用");
    assert_eq!(
        first.request().event.turn_id.as_deref(),
        Some("turn-a"),
        "Claude prompt_id 必须保留"
    );
    assert!(
        first.decide(ApprovalDecision::Deny, now),
        "首次拒绝应被接受"
    );
    assert!(
        !first.decide(ApprovalDecision::Allow, now),
        "重复批准不得覆盖拒绝"
    );
    let output = first.take_response(now).expect("拒绝应形成一次响应");
    assert_eq!(
        output["hookSpecificOutput"]["decision"]["behavior"], "deny",
        "输出必须为拒绝"
    );
    assert!(first.take_response(now).is_none(), "响应不得重复消费");
    second.cancel();
    assert!(
        !second.decide(ApprovalDecision::Allow, now),
        "取消后不得接受批准"
    );
    assert_eq!(
        second.status(now),
        ApprovalStatus::Cancelled,
        "取消应为终态"
    );
    assert!(second.take_response(now).is_none(), "取消不得产生授权 JSON");
    let expired = HookApproval::new(request(), now).expect("应可创建待超时请求");
    assert!(
        !expired.decide(ApprovalDecision::Allow, now + Duration::from_secs(120)),
        "到达 120 秒上限必须拒绝迟到批准"
    );
    assert_eq!(
        expired.status(now + Duration::from_secs(120)),
        ApprovalStatus::Cancelled,
        "过期请求不能显示待处理"
    );
}

/// 审批入口只接受绑定运行实例、Claude 会话与工具的 PermissionRequest。
///
/// 数据构造：有效模板衍生 3 个非法请求，分别缺会话、错误 Provider 和普通 Stop 事件。
/// 执行过程：逐项构造 → 创建审批。
/// 预期结果：有效模板通过；缺失身份或不支持的协议不能获得可批准请求。
#[test]
fn approval_requires_precise_supported_identity() {
    let now = Instant::now();
    assert!(
        HookApproval::new(request(), now).is_ok(),
        "完整 Claude 请求应可接入"
    );
    let mut missing = request();
    missing.event.session_id = None;
    assert!(
        HookApproval::new(missing, now).is_err(),
        "缺会话身份不得创建审批"
    );
    let mut wrong = request();
    wrong.event.profile = norma::protocol::AgentProfile::Codex;
    assert!(
        HookApproval::new(wrong, now).is_err(),
        "Codex 不使用 Claude Hook 审批 JSON"
    );
    let mut stop = request();
    stop.event.provider_event = "Stop".into();
    assert!(
        HookApproval::new(stop, now).is_err(),
        "普通事件不得进入可批准队列"
    );
}
