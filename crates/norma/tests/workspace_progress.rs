use norma::workspace_progress::*;
#[path = "support/workspace_progress.rs"]
pub mod support;
use support::*;

/// 场景：没有已知任务的已授权空空间仍返回明确的查询范围与时点。
/// 数据构造：固定查询时点 172_800_000ms（两天），默认成果窗口为此前 86_400_000ms。
/// 执行过程：通过公开纯聚合入口查询完整空空间。
/// 预期结果：所有计数为零，窗口回显一天，不暗示任何项目已经完成。
#[test]
fn empty_workspace_preserves_scope_time_and_zero_known_work() {
    let facts = AuthorizedProgressFacts::empty(
        ProgressScope {
            workspace_id: "space-a".into(),
            authorized_area_ids: vec![],
            authorization_revision: 4,
            whole_workspace: true,
        },
        7,
    );
    let result = aggregate_workspace_progress(
        ProgressQuery {
            queried_at_unix_ms: 172_800_000,
            timezone: "Asia/Shanghai".into(),
            recent_window: None,
            project_id: None,
        },
        facts,
    )
    .expect("完整空空间应该可以只读汇总");
    assert_eq!(
        result.scope.workspace_id, "space-a",
        "回复必须归发送时的空间"
    );
    assert_eq!(
        result.queried_at_unix_ms, 172_800_000,
        "查询时间必须原样保留"
    );
    assert_eq!(
        result.recent_window.start_unix_ms, 86_400_000,
        "默认窗口起点为查询时点减24小时"
    );
    assert_eq!(
        result.recent_window.end_unix_ms, 172_800_000,
        "默认窗口止点为查询时点"
    );
    assert_eq!(
        result.timezone, "Asia/Shanghai",
        "必须回显应用解释自然语言日期所用时区"
    );
    assert_eq!(
        result.totals.all_known_logical_task_count, 0,
        "空空间没有已知任务"
    );
    assert_eq!(result.totals.role_count, 0, "空空间没有已知角色");
    assert_eq!(
        result.totals.all_known_direct_activity_count, 0,
        "空空间没有直接卡片活动"
    );
    assert!(result.projects.is_empty(), "空空间不应创建占位项目或任务");
}

/// 场景：有界进度详情不会因为历史来源传入长文本而变成全文输出。
/// 数据构造：标题/项目名/角色名各2000汉字、summary2000汉字、等待原因2000汉字。
/// 执行过程：公开入口汇总旧等待输入任务，并通过任务和角色详情检查展示字段。
/// 预期结果：标题≤120字符、摘要≤600、原因≤240、角色名≤80；原任务身份和证据时间完整保留。
#[test]
fn display_fields_are_bounded_without_changing_stable_identity_or_evidence_time() {
    let mut data = facts();
    data.areas[0].project_name = "项目".repeat(1000);
    data.cards[0].name = "角色".repeat(1000);
    let mut row = task("task-ref", TaskPhase::WaitingInput, 123);
    row.title = "任务".repeat(1000);
    row.summary = Some("结果".repeat(1000));
    row.waiting_reason = Some("原因".repeat(1000));
    data.tasks.push(row);
    let result = aggregate_workspace_progress(query(), data).expect("长展示来源仍应可有界汇总");
    let row = &result.projects[0].tasks[0];
    assert!(
        row.title.chars().count() <= 120,
        "任务标题必须有界，不能输出正文全文"
    );
    assert!(
        row.summary.as_ref().expect("摘要保留").chars().count() <= 600,
        "工作摘要必须有界"
    );
    assert!(
        row.waiting_reason
            .as_ref()
            .expect("已有等待原因保留")
            .chars()
            .count()
            <= 240,
        "等待依据必须有界"
    );
    assert!(
        result.projects[0].name.chars().count() <= 120,
        "项目展示名必须有界"
    );
    assert!(
        result.projects[0].roles[0].name.chars().count() <= 80,
        "历史角色展示名必须有界"
    );
    assert_eq!(
        row.target.task_ref.as_deref(),
        Some("task-ref"),
        "截断展示不能改变稳定任务引用"
    );
    assert_eq!(
        row.evidence.observed_at_unix_ms, 123,
        "截断展示不能替换事实观察时间"
    );
}
