use brain::{BrainEvent, Lane, Router, RouterConfig, TriggerKind};
use std::time::Instant;

/// 场景：前台转交后台的长用户请求，不能按播报摘要长度截掉末尾背景和约束。
/// 构造：中英文各一条请求；背景超过旧上限 600 × 3 = 1,800 字，末尾追加否定要求。
/// 执行：通过公开 Router 推入 UserRequest，检查送往主会话的实际 Action。
/// 预期：完整原话逐字出现，保留换行和末尾要求；不变为审批，不以省略号替代尾部。
#[test]
fn long_user_request_reaches_brain_without_losing_tail_constraints() {
    for (background, tail) in [
        (
            "客户离线时必须保留草稿。\n",
            "末尾要求：不要修改 API，必须验证重复点击。",
        ),
        (
            "Offline customers need their drafts.\n",
            "Final constraint: keep the API and test duplicate clicks.",
        ),
    ] {
        let original = format!("{}{tail}", background.repeat(200));
        assert!(original.chars().count() > 1_800, "夹具必须超过旧摘要上限");
        let now = Instant::now();
        let mut router = Router::new(RouterConfig::default(), now);
        let actions = router.push(
            BrainEvent::UserRequest {
                text: original.clone(),
            },
            now,
        );
        assert_eq!(actions.len(), 1, "用户请求只产生一次交接");
        let action = &actions[0];
        assert_eq!(action.lane, Lane::Main, "用户请求仍进入主会话");
        assert_eq!(
            action.trigger.kind,
            TriggerKind::UserRequest,
            "保留用户请求来源"
        );
        assert_eq!(action.approval_id, None, "不应变为审批动作");
        assert!(action.text.contains(&original), "后台必须收到逐字完整原话");
        assert!(action.text.contains(tail), "背景之后的末尾约束不能丢失");
        assert!(!action.text.contains('…'), "不得用摘要省略号替代用户内容");
    }
}
