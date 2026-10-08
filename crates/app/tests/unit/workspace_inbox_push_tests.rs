use super::*;
use norma::mobile::WorkspaceActivityAttention;

/// 场景：每事项通知保留精确审批request与验收fingerprint，供手机直接定位。
/// 数据构造：同一目标分别只有request、只有fingerprint、两者皆无；3种kind。
/// 执行过程：调用实际通知事件构造器，检查目标、来源与可选标识。
/// 预期结果：review带fingerprint，approval带request_id；failed不夹带任一标识。
#[test]
fn inbox_attention_push_includes_exact_review_and_approval_identifiers() {
    for (kind, request, fingerprint) in [
        ("approval", Some("request"), None),
        ("review", None, Some("a".repeat(64))),
        ("failed", None, None),
    ] {
        let attention: WorkspaceActivityAttention = serde_json::from_value(json!({
            "kind":kind,"project":"项目","agent":"Agent","reason":"原说明",
            "target":{"area_id":"area","node_id":1,"profile":"Codex","run_id":"run",
                "session_id":"session","turn_id":"turn","request_id":request,"fingerprint":fingerprint}
        })).expect("完整事项");
        let event = attention_event("mac", "space", "token", "event", 400, &attention);
        assert_eq!(
            event["fingerprint"].as_str(),
            fingerprint.as_deref(),
            "{kind}通知必须保留指纹"
        );
        assert_eq!(
            event["request_id"].as_str(),
            request,
            "{kind}通知必须保留请求标识"
        );
        assert_eq!(
            event["workspace_id"], "desktop:space:area",
            "保留桌面详情scope"
        );
        assert_eq!(event["preview"]["summary"], "原说明", "说明不改写");
        assert_eq!(event["expires_at"], 400, "过期时间由调用者传入");
        if kind == "failed" {
            assert!(
                event.get("fingerprint").is_none() && event.get("request_id").is_none(),
                "失败无可选标识字段"
            );
        }
    }
}
