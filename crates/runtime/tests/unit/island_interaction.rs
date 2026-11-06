use super::{IslandEvent as E, IslandMode as M, IslandOverview, IslandState, IslandTab};

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
