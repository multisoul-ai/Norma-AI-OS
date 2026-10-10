use norma::workspace_progress::*;
#[path = "support/workspace_progress.rs"]
pub mod support;
use support::*;

/// 场景：全部旧未结束和已知异常一直展示，只有近期终态按默认24小时或指定窗口过滤。
/// 数据构造：固定Q=1_800_000_000_000ms，S=Q-86_400_000；完成在S-1/S/S+1/Q/Q+1各1项，旧待提交和失败各1项。
/// 执行过程：分别用默认窗口及S-1到S-1的显式窗口查询同一组完整事实。
/// 预期结果：默认3个近期成果；待提交与旧失败不因早创建过滤；显式窗口仅改变成果，不改变当前任务统计。
#[test]
fn recent_window_has_exact_boundaries_without_hiding_old_pending_or_abnormal_tasks() {
    let q = query();
    let start = q.queried_at_unix_ms - 86_400_000;
    let mut data = facts();
    for (reference, time) in [
        ("before", start - 1),
        ("edge", start),
        ("after", start + 1),
        ("end", q.queried_at_unix_ms),
        ("future", q.queried_at_unix_ms + 1),
    ] {
        data.tasks.push(task(reference, TaskPhase::Completed, time));
    }
    data.tasks
        .push(task("old-pending", TaskPhase::WaitingToSubmit, 1));
    data.tasks.push(task("old-failed", TaskPhase::Failed, 2));
    data.activities.push(activity(
        "old-direct-success",
        TaskPhase::Completed,
        start - 1,
    ));
    data.activities
        .push(activity("old-direct-running", TaskPhase::Running, 1));
    let default = aggregate_workspace_progress(q.clone(), data.clone()).expect("默认窗口应该有效");
    assert_eq!(
        default.totals.all_known_direct_activity_count, 2,
        "历史直接活动覆盖统计应明确命名all_known"
    );
    assert_eq!(
        default.totals.default_report_activity_count, 1,
        "旧完成直接活动不应冒充当前工作概况，旧运行活动仍显示"
    );
    assert_eq!(
        default.totals.all_known_logical_task_count, 7,
        "历史覆盖总数明确为全部已知逻辑任务"
    );
    assert_eq!(
        default.totals.default_report_task_count, 5,
        "当前概况仅包含3个近期成果加旧待提交/旧异常"
    );
    assert_eq!(
        default
            .totals
            .default_report_task_phases
            .get(&TaskPhase::Completed),
        Some(&3),
        "十年前及未来完成不能当成当前概况"
    );
    assert_eq!(
        default.totals.unfinished_task_count, 1,
        "创建很早的待提交任务仍为当前未结束任务"
    );
    assert_eq!(
        default.totals.recent_task_outcome_count, 3,
        "闭区间[S,Q]只包含edge/after/end三个终态"
    );
    assert_eq!(
        default.projects[0].recent_outcomes.len(),
        3,
        "成果详情与独立成果计数一致"
    );
    let pending = default.projects[0]
        .tasks
        .iter()
        .find(|t| t.logical_task_ref == "old-pending")
        .expect("旧待提交任务仍存在");
    assert!(
        pending.include_in_default_report,
        "未结束任务不按创建时间过滤"
    );
    let failed = default.projects[0]
        .tasks
        .iter()
        .find(|t| t.logical_task_ref == "old-failed")
        .expect("旧异常任务仍存在");
    assert!(
        failed.include_in_default_report && failed.requires_attention,
        "旧失败必须保持待处理，不能隐式清除"
    );
    let outside = default.projects[0]
        .tasks
        .iter()
        .find(|t| t.logical_task_ref == "before")
        .expect("历史任务仍可追溯");
    assert!(
        !outside.include_in_default_report,
        "窗口外正常完成任务不混入默认当前工作详情"
    );
    let explicit = aggregate_workspace_progress(
        ProgressQuery {
            recent_window: Some(ProgressWindow {
                start_unix_ms: start - 1,
                end_unix_ms: start - 1,
            }),
            ..q
        },
        data,
    )
    .expect("显式窗口应该有效");
    assert_eq!(
        explicit.totals.recent_task_outcome_count, 1,
        "显式窗口覆盖默认，仅包含before"
    );
    assert_eq!(
        explicit.totals.all_known_logical_task_count, 7,
        "指定成果窗口不改变逻辑任务总数"
    );
    assert_eq!(
        explicit.totals.unfinished_task_count, 1,
        "指定成果窗口不隐藏未结束任务"
    );
    assert_eq!(
        explicit.recent_window.start_unix_ms,
        start - 1,
        "必须回显实际采用的指定窗口"
    );
}
