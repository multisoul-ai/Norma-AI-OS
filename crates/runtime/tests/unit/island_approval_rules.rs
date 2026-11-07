use super::*;
use norma::{
    agent_hook::ApprovalDecision,
    mobile::{ApprovalSnapshot, ApprovalTarget},
};

/// 场景：“总是允许”仅对同空间、同项目根目录、同卡片与完全相同工具参数生效。
/// 数据：B/a下Claude卡片7的Bash调用，所有动态请求身份在下一回合变化。
/// 执行：计算原授权键，再变更请求、命令、卡片、空间、项目和根目录。
/// 预期：动态请求ID不影响授权；任何权限范围/输入变更都会失配；键不泄露命令正文。
#[test]
fn island_remembered_approval_is_exact_and_scoped() {
    let mut approval = ApprovalSnapshot {
        target: ApprovalTarget {
            node_id: 7,
            profile: "Claude".into(),
            run_id: "r".into(),
            session_id: "s".into(),
            turn_id: "t".into(),
            request_id: "request".into(),
        },
        card_title: "Iris".into(),
        tool: "Bash".into(),
        details: serde_json::json!({"command":"printf private-marker"}),
        decisions: vec![ApprovalDecision::Allow, ApprovalDecision::Deny],
    };
    let original = approval_rule_key("B", "a", "/project", &approval).expect("允许保存精确授权");
    approval.target.request_id = "next".into();
    approval.target.turn_id = "next-turn".into();
    assert_eq!(
        approval_rule_key("B", "a", "/project", &approval),
        Some(original.clone()),
        "后续同一操作应复用显式授权"
    );
    for (workspace, area, root) in [
        ("A", "a", "/project"),
        ("B", "b", "/project"),
        ("B", "a", "/other"),
    ] {
        assert_ne!(
            approval_rule_key(workspace, area, root, &approval),
            Some(original.clone()),
            "范围变化必须重新询问"
        );
    }
    approval.target.node_id = 8;
    assert_ne!(
        approval_rule_key("B", "a", "/project", &approval),
        Some(original.clone()),
        "其他卡片不能继承授权"
    );
    approval.target.node_id = 7;
    approval.details["command"] = "printf changed".into();
    assert_ne!(
        approval_rule_key("B", "a", "/project", &approval),
        Some(original.clone()),
        "命令参数变化不能自动放行"
    );
    assert!(
        !original.contains("private-marker"),
        "持久化授权键不得保存命令正文"
    );
    approval.decisions = vec![ApprovalDecision::Deny];
    assert_eq!(
        approval_rule_key("B", "a", "/project", &approval),
        None,
        "provider不支持允许时不能生成规则"
    );
}

/// 场景：Codex下一次调用沿用相同命令和cwd，但协议item/turn/thread身份发生变化。
/// 数据：真实commandExecution/requestApproval参数结构及accept/decline能力。
/// 执行：仅替换协议身份，再改变cwd、网络权限，最后改为未知请求类型。
/// 预期：身份变化仍匹配；执行权限与路径变化不匹配；未知结构不支持记忆。
#[test]
fn island_codex_grant_ignores_protocol_ids_but_keeps_execution_scope() {
    let mut approval = ApprovalSnapshot {
        target: ApprovalTarget {
            node_id: 7,
            profile: "Codex".into(),
            run_id: "r".into(),
            session_id: "s".into(),
            turn_id: "t".into(),
            request_id: "r:1".into(),
        },
        card_title: "Mia".into(),
        tool: "item/commandExecution/requestApproval".into(),
        details: serde_json::json!({"threadId":"s","turnId":"t","itemId":"item-a","command":"echo hello","cwd":"/project","availableDecisions":["accept","decline"]}),
        decisions: vec![ApprovalDecision::Allow, ApprovalDecision::Deny],
    };
    let original = approval_rule_key("w", "a", "/project", &approval).expect("已识别协议");
    for field in ["threadId", "turnId", "itemId"] {
        approval.details[field] = format!("new-{field}").into();
    }
    assert_eq!(
        approval_rule_key("w", "a", "/project", &approval),
        Some(original.clone()),
        "动态协议身份不得成为工具权限"
    );
    approval.details["cwd"] = "/other".into();
    assert_ne!(
        approval_rule_key("w", "a", "/project", &approval),
        Some(original.clone()),
        "cwd属于执行范围"
    );
    approval.details["cwd"] = "/project".into();
    approval.details["networkAccess"] = true.into();
    assert_ne!(
        approval_rule_key("w", "a", "/project", &approval),
        Some(original),
        "额外权限必须重新审批"
    );
    approval.tool = "unknown/requestApproval".into();
    assert_eq!(
        approval_rule_key("w", "a", "/project", &approval),
        None,
        "未知结构不得猜测哪些字段是权限"
    );
}
