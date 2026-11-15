use super::*;

fn failed(agent: &str) -> BrainEventKind {
    BrainEventKind::AgentFinished {
        node_id: 0,
        dispatch_id: None,
        agent: agent.into(),
        outcome: Outcome::Failed,
        excerpt: "编译错误".into(),
        from_result: true,
    }
}

/// 幕僚任务派出的 Agent 正常完成，下一步归幕僚，主大脑不再被叫醒去提议。
///
/// 数据构造：Simon 的正常成功属于幕僚任务（owned=true），等过静默窗口。
/// 预期结果：没有任何动作；对照：同样的事件 owned=false 时窗口后产生一条动作。
#[test]
fn owned_quiet_finishes_do_not_wake_the_main_brain() {
    let now = t0();
    let later = now + Duration::from_secs(60);
    let mut owned = Router::new(RouterConfig::default(), now);
    owned.push_owned(finished("Simon"), now, true);
    assert!(owned.poll(later).is_empty(), "staff owns the next step");
    let mut plain = Router::new(RouterConfig::default(), now);
    plain.push_owned(finished("Simon"), now, false);
    assert_eq!(
        plain.poll(later).len(),
        1,
        "user-dispatched work still reaches the main brain"
    );
}

/// 幕僚任务里的失败仍要告诉用户，但不再要求主大脑提议下一步。
///
/// 数据构造：Simon 的失败属于幕僚任务；对照组不属于。
/// 预期结果：两者都立即产生一条动作；归幕僚的提示里没有“下一步”，对照组有。
#[test]
fn owned_failures_still_reach_the_user_without_next_step_duty() {
    let now = t0();
    let mut owned = Router::new(RouterConfig::default(), now);
    owned.push_owned(failed("Simon"), now, true);
    let actions = owned.poll(now);
    assert_eq!(actions.len(), 1, "check: actions.len()");
    assert!(actions[0].text.contains("失败"), "{}", actions[0].text);
    assert!(!actions[0].text.contains("下一步"), "{}", actions[0].text);
    let mut plain = Router::new(RouterConfig::default(), now);
    plain.push_owned(failed("Simon"), now, false);
    assert!(
        plain.poll(now)[0].text.contains("下一步"),
        "check: plain.poll(now)[0].text.contains('下一步')"
    );
}

/// 同一批里只要有一个不属于幕僚，就仍按完整规则处理。
///
/// 数据构造：Simon（幕僚任务）与 Kai（用户直接派的）在同一窗口内失败。
/// 预期结果：提示里仍含“下一步”。
#[test]
fn mixed_batches_keep_the_next_step_duty() {
    let now = t0();
    let mut router = Router::new(RouterConfig::default(), now);
    router.push_owned(failed("Simon"), now, true);
    router.push_owned(failed("Kai"), now, false);
    let actions = router.poll(now);
    assert_eq!(actions.len(), 1, "check: actions.len()");
    assert!(actions[0].text.contains("下一步"), "{}", actions[0].text);
}
