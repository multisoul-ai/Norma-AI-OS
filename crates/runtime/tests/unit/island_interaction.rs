use super::{
    IslandAlert, IslandAlertKind, IslandEvent as E, IslandMode as M, IslandOverview, IslandState,
    IslandTab,
};

fn approval(id: &str) -> IslandAlert {
    IslandAlert {
        id: id.to_owned(),
        kind: IslandAlertKind::Approval,
    }
}

// 场景：审批卡下方的队列点击切换，处理完自动轮到下一个。
// 数据：待处理 a b c。先弹出 a，再点 c，然后 c 被处理掉。
// 执行：OverviewUpdated、SelectAlert、再 OverviewUpdated。
// 预期：点选后当前卡是 c；c 消失后轮到 b，而不是跳回 a。
#[test]
fn selecting_a_queued_alert_then_resolving_it_advances_forward() {
    let mut island = IslandState::default();
    island.apply(
        E::OverviewUpdated(IslandOverview {
            alerts: vec![approval("a"), approval("b"), approval("c")],
            review_count: 0,
        }),
        0,
    );
    assert_eq!(island.mode(), M::Alert, "新审批弹出提醒");
    assert_eq!(island.alert_id(), Some("a"), "默认第一张");
    assert!(
        island.apply(E::SelectAlert("c".into()), 10),
        "点队列头像应改变状态"
    );
    assert_eq!(island.alert_id(), Some("c"), "当前卡换成 c");
    assert_eq!(island.mode(), M::Alert, "切换仍停在审批形态");
    island.apply(
        E::OverviewUpdated(IslandOverview {
            alerts: vec![approval("a"), approval("b")],
            review_count: 0,
        }),
        20,
    );
    assert_eq!(island.alert_id(), Some("b"), "c 处理完轮到它前面的 b");
    assert!(
        !island.apply(E::SelectAlert("missing".into()), 30),
        "不存在的卡不能切换"
    );
    assert_eq!(island.alert_id(), Some("b"), "无效点击保持当前卡");
}

/// 场景：审批处理完，下一个是成果。成果不进提醒种类，否则会自动弹岛。
/// 数据：审批 a 正在显示。先按住成果，再把审批从概览里拿走。
/// 执行：ReviewHold，然后 OverviewUpdated 成空列表。
/// 预期：岛停在提醒形态，当前审批清空；没有按住时，空列表会收回。
#[test]
fn a_review_keeps_the_alert_open_after_the_last_approval_leaves() {
    let mut island = IslandState::default();
    island.apply(
        E::OverviewUpdated(IslandOverview {
            alerts: vec![approval("a")],
            review_count: 1,
        }),
        0,
    );
    assert_eq!(island.mode(), M::Alert, "审批先把岛打开");
    assert!(island.apply(E::ReviewHold(true), 10), "按住成果应记入状态");
    island.apply(E::OverviewUpdated(IslandOverview::default()), 20);
    assert_eq!(island.mode(), M::Alert, "成果还在时不能收回");
    assert_eq!(island.alert_id(), None, "失效审批不再指向卡片");
    let mut plain = IslandState::default();
    plain.apply(
        E::OverviewUpdated(IslandOverview {
            alerts: vec![approval("a")],
            review_count: 0,
        }),
        0,
    );
    plain.apply(E::OverviewUpdated(IslandOverview::default()), 20);
    assert_eq!(plain.mode(), M::Compact, "没有成果按住时，空队列收回");
}

// 场景：用户固定面板或正在输入时不能被离开计时器打断。
// 构造：快捷键展开并固定；执行：离开、取消固定、输入得失焦，最后 Esc。
// 预期：固定和输入时无收回截止；取消后以当前时刻+420ms 计算，Esc 无条件收回。
#[test]
fn pin_and_input_focus_suspend_collapse() {
    let mut island = IslandState::default();
    island.apply(E::ToggleExpanded, 0);
    assert_eq!(island.mode(), M::Expanded, "全局快捷键展开");
    island.apply(E::TogglePin, 10);
    island.apply(E::PointerLeft, 20);
    assert!(island.pinned(), "图钉保存固定状态");
    assert_eq!(island.deadline_ms(), None, "固定时不安排自动收回");
    island.apply(E::TogglePin, 100);
    assert!(!island.pinned(), "再次点击解除固定");
    assert_eq!(
        island.deadline_ms(),
        Some(520),
        "外部解除固定后给予完整离开宽限"
    );
    island.apply(E::InputFocusChanged(true), 200);
    assert_eq!(island.deadline_ms(), None, "输入得焦撤销收回");
    island.apply(E::Tick, 1000);
    assert_eq!(island.mode(), M::Expanded, "持续输入不能因旧计时器收回");
    island.apply(E::InputFocusChanged(false), 1100);
    assert_eq!(island.deadline_ms(), Some(1520), "失焦后重新计时");
    island.apply(E::TogglePin, 1200);
    island.apply(E::Escape, 1300);
    assert_eq!(island.mode(), M::Compact, "Esc 即使固定也收回");
    assert!(!island.pinned(), "Esc 清除固定状态");
    assert_eq!(island.deadline_ms(), None, "Esc 取消所有截止时间");
}

// 场景：打开时按待办选择默认页签，已展开时保留用户选择。
// 构造：一个成果待查看；执行：展开、选幕僚、更新概览、成功定位，再快捷键开合。
// 预期：首次 Pending，刷新不抢页签，定位收起；空概览再开默认 Running。
#[test]
fn tab_selection_and_explicit_close_follow_user_intent() {
    let mut island = IslandState::default();
    island.apply(
        E::OverviewUpdated(IslandOverview {
            alerts: vec![],
            review_count: 1,
        }),
        0,
    );
    island.apply(E::Expand, 1);
    assert_eq!(island.tab(), IslandTab::Pending, "成果也是需要处理的待办");
    island.apply(E::SelectTab(IslandTab::Staff), 2);
    island.apply(E::OverviewUpdated(IslandOverview::default()), 3);
    assert_eq!(island.tab(), IslandTab::Staff, "刷新不能切走当前对话页");
    island.apply(E::Located, 4);
    assert_eq!(island.mode(), M::Compact, "定位成功后收起");
    island.apply(E::ToggleExpanded, 5);
    assert_eq!(island.tab(), IslandTab::Running, "无待办时默认运行中");
    island.apply(E::ToggleExpanded, 6);
    assert_eq!(island.mode(), M::Compact, "再次快捷键收起");
}
