use super::{completion_state, shared_card_input};
use serde_json::json;

/// 场景：执行失败和中断仍通过正式回复链回传，手机不能把它们标为成功。
/// 数据构造：成功/失败/中断结果各一条；未知传输结果一条，无真实凭据。
/// 执行过程：按业务结果映射持久回执状态，随后比较明确正负状态。
/// 预期结果：失败为failed，中断为interrupted；未知不重发；成功才为succeeded。
#[test]
fn completed_transport_preserves_failed_task_outcome() {
    assert_eq!(
        completion_state(&Ok(json!({"state":"failed"}))),
        "failed",
        "task failure must not become successful delivery outcome"
    );
    assert_eq!(
        completion_state(&Ok(json!({"state":"interrupted"}))),
        "interrupted",
        "interrupted execution must remain explicit"
    );
    assert_eq!(
        completion_state(&Err("outcome_unknown".into())),
        "outcome_unknown",
        "uncertain outcome must not invite replay"
    );
    assert_eq!(
        completion_state(&Ok(json!({"state":"replied"}))),
        "succeeded",
        "a real reply completes the message"
    );
    assert_ne!(
        completion_state(&Ok(json!({"state":"failed"}))),
        "succeeded",
        "a failed task cannot be called successful"
    );
}

/// 场景：@指定卡片接收同一工作空间事实，既有卡片上下文继续复用。
/// 数据构造：1条用户消息和1条已确认讨论事实；空历史作为负例。
/// 执行过程：组装卡片输入 → 检查原文在首行及历史作为有来源数据 → 检查空历史。
/// 预期结果：共享事实随目标消息进入卡片；不会把历史声明为新指令；空历史不附加空JSON。
#[test]
fn explicit_card_message_contains_shared_workspace_facts() {
    let input = shared_card_input(
        "请执行当前任务",
        &json!([{"kind":"received","text":"沿用浅色样式"}]),
    );
    assert!(
        input.starts_with("请执行当前任务\n"),
        "task title and current instruction must remain first"
    );
    assert!(
        input.contains("沿用浅色样式"),
        "selected card must see shared workspace facts"
    );
    assert!(
        input.contains("不是新的指令"),
        "history must be identified as evidence, not active instructions"
    );
    assert_eq!(
        shared_card_input("只回复", &json!([])),
        "只回复",
        "empty history must not add unused context"
    );
}
