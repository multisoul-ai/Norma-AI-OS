use norma::workspace_progress::*;
#[path = "support/workspace_progress.rs"]
pub mod support;
use support::*;

/// 场景：直接卡片活动按完整turn身份去重；未证明关联保持单列；逻辑任务的历史成果与当前阶段分开。
/// 数据构造：root先完成后显式retry运行；直接turn completed有双来源及迟到快照；另一turn指向不存在的task_ref。
/// 执行过程：一次全量只读聚合，同正文与当前卡片状态都不参与关联推断。
/// 预期结果：逻辑任务1、直接活动2，当前root运行但近期原成果仍1；直接完成成果1且不回退。
#[test]
fn direct_turns_and_prior_attempt_outcomes_remain_separate_from_current_task_phase() {
    let mut data = facts();
    let time = query().queried_at_unix_ms - 10;
    data.tasks.push(task("root", TaskPhase::Completed, time));
    let mut retry = task("retry", TaskPhase::Running, time + 1);
    retry.logical_task_ref = Some("root".into());
    retry.attempt_sequence = 1;
    data.tasks.push(retry);
    let mut direct = activity("direct-turn", TaskPhase::Completed, time);
    direct.needs_review = true;
    direct.result_ref = Some("direct-result".into());
    data.activities.push(direct.clone());
    let mut duplicate = direct.clone();
    duplicate.evidence.source = EvidenceSource::ProviderHook;
    data.activities.push(duplicate);
    let mut late = direct;
    late.phase = TaskPhase::Running;
    late.evidence.kind = EvidenceKind::Snapshot;
    late.evidence.observed_at_unix_ms = time + 2;
    data.activities.push(late);
    let mut unlinked = activity("unlinked-turn", TaskPhase::Running, time);
    unlinked.linked_task_ref = Some("missing-task".into());
    unlinked.linked_attempt_sequence = Some(0);
    data.activities.push(unlinked);
    let result = aggregate_workspace_progress(query(), data).expect("直接活动应与任务同一次聚合");
    assert_eq!(
        result.totals.all_known_logical_task_count, 1,
        "显式retry仍属一个逻辑任务"
    );
    assert_eq!(
        result.totals.all_known_task_phases.get(&TaskPhase::Running),
        Some(&1),
        "当前尝试已运行而不是原完成阶段"
    );
    assert_eq!(
        result.totals.all_known_direct_activity_count, 2,
        "三来源direct只1项，未证明链接另1项"
    );
    assert_eq!(
        result.totals.recent_task_outcome_count, 1,
        "当前retry不应抹掉窗口内原逻辑任务的可信成果"
    );
    assert_eq!(
        result.totals.recent_activity_outcome_count, 1,
        "直接卡片可信成果独立计数且多来源不重复"
    );
    assert_eq!(
        result.projects[0].recent_outcomes.len(),
        2,
        "逻辑任务成果与直接卡片成果分列两项"
    );
    let direct = result.projects[0]
        .activities
        .iter()
        .find(|a| a.execution.turn_id == "direct-turn")
        .expect("直接活动应可查看");
    assert_eq!(
        direct.phase,
        TaskPhase::Completed,
        "turn可信完成不能被其后来快照覆盖"
    );
    assert_eq!(
        direct.target.result_ref.as_deref(),
        Some("direct-result"),
        "直接成果保留稳定结果入口"
    );
    let unlinked = result.projects[0]
        .activities
        .iter()
        .find(|a| a.execution.turn_id == "unlinked-turn")
        .expect("未关联turn应单列");
    assert!(
        unlinked.quality.contains(&ProgressQuality::UnresolvedLink),
        "无匹配task证据时说明关联待核对"
    );
}

/// 场景：同turn一个来源已证明任务关联，另一重复来源未带关联字段时，不能重复变成直接卡片活动。
/// 数据构造：1个accepted任务、1个linked running hook和同完整turn的未linked重复Runtime观察。
/// 执行过程：按两种来源到达顺序聚合同一组事实。
/// 预期结果：仅1个运行逻辑任务、零直接活动；状态与计数不依赖输入顺序。
#[test]
fn proven_turn_link_applies_to_all_duplicate_sources_independent_of_arrival_order() {
    let mut data = facts();
    data.tasks.push(task("root", TaskPhase::Accepted, 100));
    let mut linked = activity("direct-turn", TaskPhase::Running, 101);
    linked.linked_task_ref = Some("root".into());
    linked.linked_attempt_sequence = Some(0);
    linked.evidence.source = EvidenceSource::ProviderHook;
    let mut duplicate = linked.clone();
    duplicate.linked_task_ref = None;
    duplicate.linked_attempt_sequence = None;
    duplicate.evidence.source = EvidenceSource::Runtime;
    data.activities = vec![linked.clone(), duplicate.clone()];
    let first = aggregate_workspace_progress(query(), data.clone())
        .expect("已证明turn关联应该合并所有来源");
    data.activities = vec![duplicate, linked];
    let reversed = aggregate_workspace_progress(query(), data).expect("来源顺序不改变事实含义");
    assert_eq!(
        first.totals.all_known_logical_task_count, 1,
        "重复来源不能增加逻辑任务"
    );
    assert_eq!(
        first.totals.all_known_direct_activity_count, 0,
        "已经关联到任务的同turn来源不应再单列活动"
    );
    assert_eq!(
        first.projects[0].tasks[0].phase,
        TaskPhase::Running,
        "匹配turn的真实执行证据应更新原任务"
    );
    assert_eq!(first, reversed, "聚合结果必须独立于来源到达顺序");
}
