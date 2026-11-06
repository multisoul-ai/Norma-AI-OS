use super::{
    IslandAlert, IslandAlertKind as K, IslandEvent as E, IslandMode as M, IslandOverview,
    IslandState,
};

fn overview(items: &[(&str, K)]) -> E {
    E::OverviewUpdated(IslandOverview {
        alerts: items
            .iter()
            .map(|(id, kind)| IslandAlert {
                id: (*id).into(),
                kind: *kind,
            })
            .collect(),
        review_count: 0,
    })
}

// 场景：审批和提问高于完成通知，多个提醒按首次展示顺序逐个处理。
// 构造：先收到完成，再收到审批 a 和提问 q；执行：完成事件、超长 tick、依次移除 a/q。
// 预期：审批覆盖完成，完成不能覆盖审批；提醒无截止时间，处理后显示下一条，队列清空才收起。
#[test]
fn alerts_preempt_completion_and_remain_until_resolved() {
    let mut island = IslandState::default();
    island.apply(E::Completed("done".into()), 0);
    island.apply(overview(&[("a", K::Approval), ("q", K::Question)]), 100);
    assert_eq!(island.mode(), M::Alert, "审批应覆盖完成");
    assert_eq!(island.alert_id(), Some("a"), "只展示第一个提醒");
    assert_eq!(island.activity_id(), None, "旧完成通知应移除");
    assert!(
        !island.apply(overview(&[("a", K::Approval), ("q", K::Question)]), 150),
        "相同概览不引发状态更新或重放提醒"
    );
    island.apply(overview(&[("q", K::Question), ("a", K::Approval)]), 160);
    assert_eq!(
        island.alert_id(),
        Some("a"),
        "投影排序变化不应换掉正在处理的审批"
    );
    island.apply(E::Completed("later".into()), 200);
    island.apply(E::PointerLeft, 300);
    island.apply(E::Tick, 100_000);
    assert_eq!(
        island.mode(),
        M::Alert,
        "未处理提醒不超时且不被完成事件替换"
    );
    assert_eq!(island.alert_id(), Some("a"), "保留当前审批");
    assert_eq!(island.deadline_ms(), None, "提醒不应安排收回");
    island.apply(overview(&[("q", K::Question)]), 100_100);
    assert_eq!(island.alert_id(), Some("q"), "审批结束后展示提问");
    island.apply(overview(&[]), 100_200);
    assert_eq!(island.mode(), M::Compact, "最后一条处理后收起");
    assert_eq!(island.alert_id(), None, "不保留失效提醒");
}
