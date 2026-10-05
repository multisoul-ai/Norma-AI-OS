use super::decode;
use serde_json::{json, Value};

fn request(kind: &str) -> String {
    json!({"version":1,"request_id":"old-operation","machine_id":"mac-test",
        "action":{"type":kind,"device_id":"phone-test","token":"fixture-only"}})
    .to_string()
}

fn rejected(kind: &str) -> Value {
    decode(&request(kind))
        .err()
        .expect("legacy mutation must be rejected before dispatch")
}

/// 场景：旧线程写入口在局域网与中继共用解码边界退役，不能复活旧待发送任务。
/// 数据构造：旧消息、派工、追问、验收、终端控制5种写动作，各使用相同虚拟设备且不带真实凭据。
/// 执行过程：解码旧协议信封 → 在任何授权、写库或派工前返回只读错误 → 对比仍有效的认证动作。
/// 预期结果：5种写动作逐一被拒；稳定request_id保留；普通认证不能被旧协议屏障拒绝。
#[test]
fn legacy_thread_mutations_are_read_only_on_both_transports() {
    assert_eq!(
        rejected("thread_message")["error"],
        "legacy_thread_read_only",
        "old messages must not create new history"
    );
    assert_eq!(
        rejected("thread_task")["error"],
        "legacy_thread_read_only",
        "old tasks must not be dispatched"
    );
    assert_eq!(
        rejected("thread_task_followup")["error"],
        "legacy_thread_read_only",
        "old followups must not write to a session"
    );
    assert_eq!(
        rejected("thread_task_accept")["error"],
        "legacy_thread_read_only",
        "old acceptance must not mutate task state"
    );
    assert_eq!(
        rejected("thread_task_terminal_control")["error"],
        "legacy_thread_read_only",
        "old terminal control must not claim input"
    );
    assert_eq!(
        rejected("thread_task")["request_id"],
        "old-operation",
        "rejection must remain correlated to the original operation"
    );
    assert!(
        decode(&request("authenticate")).is_ok(),
        "existing pairing/authentication must remain available"
    );
    assert!(
        decode(&request("workspace_catalog")).is_ok(),
        "new workspace directory must not be retired"
    );
}

/// 场景：退役旧写协议不能影响旧消息、任务回执的只读核查。
/// 数据构造：2个历史查询，沿用旧collaboration_workspace_id/thread_id，不按名称映射新空间。
/// 执行过程：解码两个查询 → 检查被正式只读Action接收。
/// 预期结果：旧消息和旧任务查询均存在；查询不会成为写动作。
#[test]
fn legacy_receipt_queries_remain_available() {
    let receipt = |kind| {
        json!({"version":1,"request_id":"lookup","machine_id":"mac-test",
        "action":{"type":kind,"device_id":"phone-test","token":"fixture-only",
        "collaboration_workspace_id":"old-space","thread_id":"old-thread","operation_id":"old-operation"}}).to_string()
    };
    assert!(
        decode(&receipt("thread_receipt")).is_ok(),
        "historical message receipt lookup must remain"
    );
    assert!(
        decode(&receipt("thread_task_receipt")).is_ok(),
        "historical task receipt lookup must remain"
    );
    assert!(
        decode(&request("not_an_action")).is_err(),
        "unknown actions must not bypass decoding"
    );
}
