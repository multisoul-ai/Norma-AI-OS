use super::{completion_node, IslandEvent as E, IslandMode as M, IslandState};

// 场景：悬停展开并在离开后收回，截止时间不能提前触发。
// 构造：100ms 时移入；执行：259/260ms tick，再在 300ms 移出并在 719/720ms tick。
// 预期：100+160=260ms 展开，300+420=720ms 收回；每个边界前保持原形态。
#[test]
fn hover_and_leave_obey_exact_deadlines() {
    let mut island = IslandState::default();
    assert_eq!(island.mode(), M::Compact, "默认收起");
    assert_eq!(island.deadline_ms(), None, "空闲时不需定时刷新");
    island.apply(E::PointerEntered, 100);
    assert_eq!(island.deadline_ms(), Some(260), "悬停截止为 160ms 后");
    island.apply(E::Tick, 259);
    assert_eq!(island.mode(), M::Compact, "不能提前展开");
    island.apply(E::Tick, 260);
    assert_eq!(island.mode(), M::Expanded, "到截止时展开");
    island.apply(E::PointerLeft, 300);
    assert_eq!(island.deadline_ms(), Some(720), "离开截止为 420ms 后");
    island.apply(E::Tick, 719);
    assert_eq!(island.mode(), M::Expanded, "不能提前收回");
    island.apply(E::Tick, 720);
    assert_eq!(island.mode(), M::Compact, "截止后收起");
    assert_eq!(island.deadline_ms(), None, "收起后没有无用定时器");
}

// 场景：完成通知停留三秒，不被鼠标进出延长或提前清除。
// 构造：100ms 收到完成；执行：指针进出后在 3099/3100ms tick。
// 预期：100+3000=3100ms 前保持 Activity，到时恢复 Compact，无剩余截止时间。
#[test]
fn completion_returns_after_three_seconds_even_with_pointer_events() {
    let mut island = IslandState::default();
    island.apply(E::Completed("done-1".into()), 100);
    assert_eq!(island.mode(), M::Activity, "完成应短暂展示");
    assert_eq!(island.deadline_ms(), Some(3100), "完成停留 3000ms");
    island.apply(E::PointerEntered, 200);
    island.apply(E::PointerLeft, 300);
    island.apply(E::Tick, 3099);
    assert_eq!(island.mode(), M::Activity, "指针进出不能清除通知期限");
    island.apply(E::Tick, 3100);
    assert_eq!(island.mode(), M::Compact, "完成通知到时收回");
    assert_eq!(island.deadline_ms(), None, "结束后不轮询");
}

// 场景：原生 tracking area 重复发入/出事件，不能不断延后截止时间。
// 构造：100ms 入岛，120ms 重复；执行：260ms 展开，300/320ms 连续离开。
// 预期：首次悬停仍在 260ms 到期，首次离开仍在 720ms 到期；重复事件不产生状态变更。
#[test]
fn repeated_pointer_events_do_not_postpone_deadlines() {
    let mut island = IslandState::default();
    island.apply(E::PointerEntered, 100);
    assert!(!island.apply(E::PointerEntered, 120), "重复移入是 no-op");
    assert_eq!(island.deadline_ms(), Some(260), "不延后悬停截止");
    island.apply(E::Tick, 260);
    island.apply(E::PointerLeft, 300);
    assert!(!island.apply(E::PointerLeft, 320), "重复移出是 no-op");
    assert_eq!(island.deadline_ms(), Some(720), "不延后收回截止");
}

// 场景：指针在展开延时内离开，旧 timer 即使迟到也不可重新展开。
// 构造：10ms 进入，100ms 离开；执行：旧 170ms timer 回调。
// 预期：截止清除，迟到 Tick 不改变 Compact；u64 上界计算不溢出。
#[test]
fn stale_tick_after_cancel_is_harmless() {
    let mut island = IslandState::default();
    island.apply(E::PointerEntered, 10);
    island.apply(E::PointerLeft, 100);
    assert_eq!(island.deadline_ms(), None, "离开取消悬停");
    assert!(!island.apply(E::Tick, 170), "迟到 timer 无副作用");
    assert_eq!(island.mode(), M::Compact, "保持收起");
    island.apply(E::PointerEntered, u64::MAX - 10);
    assert_eq!(island.deadline_ms(), Some(u64::MAX), "截止时间饱和而不溢出");
}

/// 场景：只有真实任务完成可主动弹岛，失败、退出和重复Done都不能冒充完成。
/// 数据：同一卡片分别从Thinking进入Done/Failed/Ended，以及Done到Done。
/// 执行：通过运行时事件映射提取完成节点。
/// 预期：仅Thinking到Done返回节点7，其余全部无通知。
#[test]
fn island_completion_only_accepts_a_new_successful_transition() {
    use crate::{AgentTaskState, RuntimeEvent};
    let event = |from, to| RuntimeEvent::AgentTaskStateChanged {
        node_id: 7,
        from,
        to,
        source_event: "fixture".into(),
    };
    assert_eq!(
        completion_node(&event(AgentTaskState::Thinking, AgentTaskState::Done)),
        Some(7),
        "真实完成应显示一次"
    );
    assert_eq!(
        completion_node(&event(AgentTaskState::Done, AgentTaskState::Done)),
        None,
        "重复终态不能再次弹出"
    );
    assert_eq!(
        completion_node(&event(AgentTaskState::Thinking, AgentTaskState::Ended)),
        None,
        "退出不等于完成"
    );
    assert_eq!(
        completion_node(&event(
            AgentTaskState::Thinking,
            AgentTaskState::Failed {
                message: "failed".into()
            }
        )),
        None,
        "失败只留在列表，不主动弹出"
    );
}

/// 场景：在提醒内回答后显示2200ms确认；下一个待办优先，不被确认提示遮住。
/// 数据：完成通知开关关闭，问题q已弹出；随后再构造q与q2同时待处理。
/// 执行：确认q、在2199/2200ms前后Tick；另一状态确认q并检查q2。
/// 预期：用户操作反馈不受完成主动弹出开关影响；自动收回且不假称任务完成；队列仍可处理。
#[test]
fn island_action_confirmation_uses_short_feedback_without_hiding_next_alert() {
    use super::{IslandEvent, IslandMode};
    let question = |id: &str| super::IslandAlert {
        id: id.into(),
        kind: super::IslandAlertKind::Question,
    };
    let mut state = IslandState::default();
    state.apply(
        IslandEvent::PreferencesChanged(super::IslandPreferences {
            completions: false,
            ..Default::default()
        }),
        0,
    );
    state.apply(
        IslandEvent::OverviewUpdated(super::IslandOverview {
            alerts: vec![question("q")],
            ..Default::default()
        }),
        0,
    );
    state.apply(IslandEvent::ActionConfirmed("q".into()), 100);
    assert_eq!(
        state.mode(),
        IslandMode::Activity,
        "手动回答必须有短确认反馈"
    );
    assert_eq!(
        state.activity_id(),
        None,
        "操作成功不能伪装成智能体任务完成"
    );
    assert_eq!(state.deadline_ms(), Some(2300), "反馈持续2200ms");
    state.apply(
        IslandEvent::OverviewUpdated(super::IslandOverview {
            alerts: vec![question("q")],
            ..Default::default()
        }),
        200,
    );
    assert_eq!(
        state.mode(),
        IslandMode::Activity,
        "迟到投影不能复活刚确认的请求"
    );
    state.apply(IslandEvent::Tick, 2299);
    assert_eq!(state.mode(), IslandMode::Activity, "截止前保留反馈");
    state.apply(IslandEvent::Tick, 2300);
    assert_eq!(state.mode(), IslandMode::Compact, "反馈到期收回");
    state.apply(
        IslandEvent::OverviewUpdated(super::IslandOverview {
            alerts: vec![question("q"), question("q2")],
            ..Default::default()
        }),
        2400,
    );
    state.apply(IslandEvent::ActionConfirmed("q".into()), 2500);
    assert_eq!(state.mode(), IslandMode::Alert, "不能遮住下一个待办");
    assert_eq!(state.alert_id(), Some("q2"), "队列转到下一题");
    assert_eq!(state.deadline_ms(), None, "下一题不会被确认计时器收回");
    state.apply(IslandEvent::ActionConfirmed("q2".into()), 2600);
    assert_eq!(
        state.mode(),
        IslandMode::Activity,
        "旧快照中的q已处理，不能在q2之后重新提醒"
    );
}
