use super::{
    IslandAlert, IslandAlertKind as K, IslandEvent as E, IslandMode as M, IslandOverview,
    IslandPreferences, IslandState,
};

fn pending() -> IslandOverview {
    IslandOverview {
        alerts: vec![
            IslandAlert {
                id: "approval".into(),
                kind: K::Approval,
            },
            IslandAlert {
                id: "question".into(),
                kind: K::Question,
            },
        ],
        review_count: 0,
    }
}

// 场景：主动弹出三类开关独立，关闭审批不应吞掉提问或隐藏待办数据。
// 构造：关闭审批和完成，保留提问；执行：更新两条待办，再关闭提问，重复同一概览。
// 预期：只弹提问，关闭后收回，完成不弹；再次开启不重播已有请求，手动展开仍可处理。
#[test]
fn popup_preferences_do_not_discard_pending_items() {
    let mut island = IslandState::default();
    let mut preferences = IslandPreferences {
        approvals: false,
        completions: false,
        ..Default::default()
    };
    island.apply(E::PreferencesChanged(preferences), 0);
    island.apply(E::OverviewUpdated(pending()), 1);
    assert_eq!(island.alert_id(), Some("question"), "审批关闭时仍能弹提问");
    preferences.questions = false;
    island.apply(E::PreferencesChanged(preferences), 2);
    assert_eq!(island.mode(), M::Compact, "关闭当前提醒类型后收起");
    island.apply(E::Completed("done".into()), 3);
    assert_eq!(island.mode(), M::Compact, "完成通知独立关闭");
    preferences.approvals = true;
    island.apply(E::PreferencesChanged(preferences), 4);
    assert!(
        !island.apply(E::OverviewUpdated(pending()), 5),
        "相同快照不重播也不触发变更"
    );
    island.apply(E::Expand, 6);
    assert_eq!(
        island.tab(),
        super::IslandTab::Pending,
        "禁用弹出不等于丢弃待办"
    );
}

// 场景：隐藏岛时取消展示与计时器，但继续接收概览；重新显示不能重放旧提醒。
// 构造：展开并固定；执行：隐藏、到达新待办、快捷键和悬停，再取消隐藏。
// 预期：隐藏期间始终 Compact、无截止、无 pin；重新显示后只在主动展开时展示待办。
#[test]
fn hiding_cancels_deadlines_without_replaying_attention() {
    let mut island = IslandState::default();
    island.apply(E::Expand, 0);
    island.apply(E::TogglePin, 1);
    island.apply(
        E::PreferencesChanged(IslandPreferences {
            hidden: true,
            ..Default::default()
        }),
        2,
    );
    island.apply(E::OverviewUpdated(pending()), 3);
    island.apply(E::ToggleExpanded, 4);
    island.apply(E::PointerEntered, 5);
    island.apply(E::Tick, 999);
    assert_eq!(island.mode(), M::Compact, "隐藏时不可被快捷键或提醒打开");
    assert!(!island.pinned(), "隐藏清除固定状态");
    assert_eq!(island.deadline_ms(), None, "隐藏后没有遗留计时器");
    island.apply(E::PreferencesChanged(IslandPreferences::default()), 1000);
    assert!(
        !island.apply(E::OverviewUpdated(pending()), 1001),
        "取消隐藏不重播旧提醒"
    );
    assert_eq!(island.mode(), M::Compact, "保持收起直到用户打开");
    island.apply(E::Expand, 1002);
    assert_eq!(
        island.tab(),
        super::IslandTab::Pending,
        "隐藏时仍保存待办状态"
    );
}

/// 场景：操作确认展示中重新应用关闭完成通知的设置。
/// 数据：100ms收到真实操作回执；200ms重放completions=false。
/// 执行：检查设置重放后、2200ms期限前后，再隐藏岛。
/// 预期：确认不被完成通知开关截断；hidden仍立即关闭并清除期限。
#[test]
fn island_confirmation_survives_preferences_replay_but_not_hidden() {
    let mut island = IslandState::default();
    island.apply(E::ActionConfirmed("answered".into()), 100);
    island.apply(
        E::PreferencesChanged(IslandPreferences {
            completions: false,
            ..Default::default()
        }),
        200,
    );
    assert_eq!(island.mode(), M::Activity, "完成开关不能关闭操作确认");
    assert_eq!(island.deadline_ms(), Some(2300), "不能重置2200ms确认期限");
    island.apply(E::Tick, 2299);
    assert_eq!(island.mode(), M::Activity, "期限前保留确认");
    island.apply(E::Tick, 2300);
    assert_eq!(island.mode(), M::Compact, "期限后自动收回");
    island.apply(E::ActionConfirmed("another".into()), 2400);
    island.apply(
        E::PreferencesChanged(IslandPreferences {
            hidden: true,
            ..Default::default()
        }),
        2500,
    );
    assert_eq!(island.mode(), M::Compact, "隐藏开关仍应关闭所有展示");
    assert_eq!(island.deadline_ms(), None, "隐藏取消残留计时");
}
