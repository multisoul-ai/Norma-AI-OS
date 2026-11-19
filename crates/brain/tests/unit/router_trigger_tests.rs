use super::*;

/// 每条动作都带触发源，后台持久化与活动面板据此显示“为什么有这一轮”。
///
/// 数据构造：审批、需要告知的完成、静默完成、用户请求、用户回来、定时摘要各一次。
/// 预期结果：
///   - 正断言：种类与相关 Agent 名依次为 ApprovalRequest/Claude、AgentFinished/Alan、
///     AgentFinished/Bella、UserRequest、UserReturned、Scheduled
///   - 负断言：非 Agent 触发的 `who` 为空
#[test]
fn every_action_carries_its_trigger() {
    let now = t0();
    let mut router = Router::new(RouterConfig::default(), now);
    let approval_actions = router.push(approval("a1", "cargo test"), now);
    assert_eq!(
        approval_actions[0].trigger,
        Trigger::new(TriggerKind::ApprovalRequest, "Claude")
    );
    router.push(needs_you("Alan"), now);
    router.push(finished("Bella"), now);
    let urgent = router.poll(now + Duration::from_secs(1));
    assert_eq!(
        urgent[0].trigger,
        Trigger::new(TriggerKind::AgentFinished, "Alan")
    );
    let quiet = router.poll(now + Duration::from_secs(30));
    assert_eq!(
        quiet[0].trigger,
        Trigger::new(TriggerKind::AgentFinished, "Bella")
    );
    let request = router.push(
        BrainEventKind::UserRequest {
            text: "看看进度".into(),
        },
        now,
    );
    assert_eq!(
        request[0].trigger,
        Trigger::new(TriggerKind::UserRequest, "")
    );
    let returned = router.push(
        BrainEventKind::UserReturned {
            away: Duration::from_secs(0),
        },
        now + Duration::from_secs(31),
    );
    assert!(returned.is_empty(), "刚离开就回来没有新进展，不叫醒大脑");
    router.push(
        BrainEventKind::AgentProgress {
            agent: "Alan".into(),
            text: "跑测试".into(),
        },
        now,
    );
    let summary = router.poll(now + Duration::from_secs(400));
    assert_eq!(summary[0].trigger, Trigger::new(TriggerKind::Scheduled, ""));
}

/// 回来摘要的触发源是 UserReturned。
///
/// 数据构造：Alan 完成后，用户离开 10 秒回来。
#[test]
fn return_digest_is_triggered_by_the_user_returning() {
    let now = t0();
    let mut router = Router::new(RouterConfig::default(), now);
    router.push(finished("Alan"), now);
    let actions = router.push(
        BrainEventKind::UserReturned {
            away: Duration::from_secs(10),
        },
        now + Duration::from_secs(5),
    );
    assert_eq!(
        actions[0].trigger,
        Trigger::new(TriggerKind::UserReturned, "")
    );
}

/// 一批里有多个 Agent 时，`who` 按首次出现的顺序去重后用顿号连接。
#[test]
fn a_batch_names_every_agent_once() {
    let now = t0();
    let mut router = Router::new(RouterConfig::default(), now);
    router.push(needs_you("Alan"), now);
    router.push(needs_you("Bella"), now);
    router.push(needs_you("Alan"), now);
    let actions = router.poll(now + Duration::from_secs(1));
    assert_eq!(actions[0].trigger.who, "Alan、Bella");
}
