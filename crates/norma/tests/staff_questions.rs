use norma::protocol::{Task, TaskDisplayStatus};
use serde_json::json;

/// 场景：旧提议仍待决定，忽略的提议持久化后不再要求用户采用。
/// 数据构造：同一旧任务 JSON 的提议缺 ignored 字段；再将唯一提议设为 ignored=true。
/// 执行过程：分别反序列化并读取展示状态，再序列化忽略后的任务。
/// 预期结果：旧数据是 NeedsYou；忽略后是 Waiting 而不是 NeedsYou，且 ignored 保持 true。
#[test]
fn old_proposals_remain_pending_but_ignored_proposals_do_not_need_you() {
    let mut value = serde_json::to_value(Task::new("t", "w", "p", "Ship")).expect("task");
    value["proposals"] = json!([{"id":1,"agent":"Simon","title":"Ship","prompt":"Build","why":"Ready","approved":false,"dispatch_id":null}]);
    let old: Task = serde_json::from_value(value.clone()).expect("old task must remain readable");
    assert_eq!(
        old.display_status(),
        TaskDisplayStatus::NeedsYou,
        "旧提议仍然待决定"
    );
    value["proposals"][0]["ignored"] = json!(true);
    let ignored: Task = serde_json::from_value(value).expect("ignored task");
    assert_eq!(
        ignored.display_status(),
        TaskDisplayStatus::Waiting,
        "忽略后不再需要用户采用"
    );
    assert_ne!(
        ignored.display_status(),
        TaskDisplayStatus::NeedsYou,
        "忽略不应继续点亮需要你"
    );
    assert_eq!(
        serde_json::to_value(ignored).expect("roundtrip")["proposals"][0]["ignored"],
        true,
        "忽略决定必须存盘"
    );
}

/// 场景：问答是有状态的持久消息，待回答才点亮“需要你”。
/// 数据构造：一个任务含一条两选一提问；不传可选来源和计时字段，验证默认值兼容。
/// 执行过程：反序列化 Pending 问答 → 改为 Answered → 再往返 JSON。
/// 预期结果：待回答为 NeedsYou；回答后为 Waiting，题目及终态完整保留。
#[test]
fn question_messages_roundtrip_and_only_pending_questions_need_you() {
    let mut value = serde_json::to_value(Task::new("t", "w", "p", "Ship")).expect("task");
    value["messages"] = json!([{"Question":{"question":{"id":"q", "state":"Pending", "items":[{
        "key":"theme", "prompt":"Theme?", "options":[{"key":"light","label":"Light"},{"key":"dark","label":"Dark"}],
        "multiple":false, "allow_custom":true, "secret":false
    }]}}}]);
    let pending: Task =
        serde_json::from_value(value.clone()).expect("question message must deserialize");
    assert_eq!(
        pending.display_status(),
        TaskDisplayStatus::NeedsYou,
        "待答需要用户"
    );
    value["messages"][0]["Question"]["question"]["state"] = json!("Answered");
    let answered: Task = serde_json::from_value(value).expect("answered question");
    assert_eq!(
        answered.display_status(),
        TaskDisplayStatus::Waiting,
        "回答后不再需要用户"
    );
    assert_ne!(
        answered.display_status(),
        TaskDisplayStatus::NeedsYou,
        "终态不阻塞输入"
    );
    let roundtrip = serde_json::to_value(answered).expect("serialize");
    assert_eq!(
        roundtrip["messages"][0]["Question"]["question"]["state"], "Answered",
        "回答状态不能丢失"
    );
    assert_eq!(
        roundtrip["messages"][0]["Question"]["question"]["items"][0]["prompt"], "Theme?",
        "题目是用户内容，原文保留"
    );
}
