use norma::workspace_progress::*;
#[path = "support/workspace_progress.rs"]
pub mod support;
use support::*;

/// 场景：只有旧称呼、缺少稳定卡片身份的历史记录保持待核对，不绑定同名现存卡片。
/// 数据构造：现存Betty卡片1张；同旧称呼的record-1/record-2各1条，record-1另有重复输入；
/// 另加入未授权区域和其他空间记录，原证据时间/序号均未知；标题/摘要/原因分别超过120/600/240字符。
/// 执行过程：只读聚合后取得每页1条的历史待核对记录，并序列化输出检查身份与授权范围。
/// 预期结果：2条独立历史记录持续可见，重复原引用不增量；角色仍为1、任务/活动/成果为0；
/// 不编造卡片导航身份或时间，未授权事实消失；详情有界且分页保留完整计数。
#[test]
fn name_only_history_is_retained_without_inventing_card_identity_or_progress() {
    let mut data = facts();
    let first = UnresolvedHistoricalFact {
        workspace_id: "space-a".into(),
        area_id: "area-a".into(),
        record_ref: "record-1".into(),
        name_at_observation: Some("Betty".into()),
        title: "旧".repeat(130),
        summary: Some("摘".repeat(610)),
        reason: "因".repeat(250),
        evidence: UnresolvedHistoricalEvidence {
            source: EvidenceSource::ProjectLedger,
            kind: EvidenceKind::DisplayOnly,
            observed_at_unix_ms: None,
            source_sequence: None,
            freshness: FactFreshness::NeedsVerification,
            quality: vec![],
        },
    };
    let mut second = first.clone();
    second.record_ref = "record-2".into();
    let mut outside_area = first.clone();
    outside_area.area_id = "secret-area".into();
    outside_area.record_ref = "secret-history".into();
    let mut outside_workspace = first.clone();
    outside_workspace.workspace_id = "space-b".into();
    outside_workspace.record_ref = "other-workspace-history".into();
    data.unresolved_history = vec![
        first.clone(),
        second,
        first,
        outside_area,
        outside_workspace,
    ];
    let snapshot = aggregate_workspace_progress(query(), data).expect("旧记录不应阻断已知进度查询");
    assert_eq!(
        snapshot.totals.role_count, 1,
        "旧称呼不能创建或绑定额外角色"
    );
    assert_eq!(
        snapshot.totals.all_known_logical_task_count, 0,
        "无任务身份不能增加跟踪任务数"
    );
    assert_eq!(
        snapshot.totals.default_report_task_count, 0,
        "待核对旧记录不能伪造当前任务"
    );
    assert_eq!(
        snapshot.totals.all_known_direct_activity_count, 0,
        "无执行身份不能伪造卡片活动"
    );
    assert_eq!(
        snapshot.totals.recent_task_outcome_count, 0,
        "未知时间和结果不能冒充近期成果"
    );
    assert_eq!(
        snapshot.totals.recent_activity_outcome_count, 0,
        "旧摘要不能冒充活动成果"
    );
    assert!(
        snapshot.totals.all_known_task_phases.is_empty(),
        "无稳定任务身份不进入阶段分布"
    );
    assert_eq!(
        snapshot.totals.unresolved_historical_record_count, 2,
        "同名两原引用独立，重复同引用只计一次"
    );
    let page = snapshot.projects[0].unresolved_history_page(0, 1);
    assert_eq!(page.items.len(), 1, "待核对历史详情首屏有界");
    assert_eq!(page.total_items, 2, "分页不截断完整待核对计数");
    assert_eq!(page.next_offset, Some(1), "同名第二原记录可继续查看");
    assert_eq!(
        page.totals.unresolved_historical_record_count, 2,
        "页面保留项目完整总计"
    );
    let row = &page.items[0];
    assert_eq!(row.record_ref, "record-1", "保留原来源稳定记录引用");
    assert_eq!(
        row.name_at_observation.as_deref(),
        Some("Betty"),
        "旧称呼仅作历史展示"
    );
    assert_eq!(
        row.evidence.observed_at_unix_ms, None,
        "原时间未知不能用查询时间填造"
    );
    assert_eq!(row.evidence.source_sequence, None, "缺失原序号不能填造零");
    assert!(
        row.evidence
            .quality
            .contains(&ProgressQuality::UnresolvedLink),
        "无身份历史必须明确标待核对"
    );
    assert_eq!(row.title.chars().count(), 120, "历史标题限制120字符");
    assert_eq!(
        row.summary.as_ref().expect("保留原摘要").chars().count(),
        600,
        "历史摘要限制600字符"
    );
    assert_eq!(row.reason.chars().count(), 240, "待核对原因限制240字符");
    let encoded_row = serde_json::to_string(row).expect("历史待核对DTO可序列化");
    assert!(
        !encoded_row.contains("card_instance_id")
            && !encoded_row.contains("target")
            && !encoded_row.contains("node_id"),
        "待核对记录没有可执行卡片导航目标"
    );
    let encoded = serde_json::to_string(&snapshot).expect("完整查询快照可序列化");
    assert!(
        !encoded.contains("secret-history")
            && !encoded.contains("other-workspace-history")
            && !encoded.contains("secret-area"),
        "未授权记录及其名称数量缺口均不能泄漏"
    );
    let next = snapshot.projects[0].unresolved_history_page(1, 1);
    assert_eq!(
        next.items[0].record_ref, "record-2",
        "同名第二原记录不可按称呼归并"
    );
}

/// 场景：授权区域目录缺失时，旧无身份历史仍保留于归属待核对分组，项目筛选不能猜归属。
/// 数据构造：已知project-a与无目录的area-missing都获授权；area-missing保留观察时间1毫秒的旧记录。
/// 执行过程：先查完整授权范围，再按project-a收窄；检查旧记录、已知项目数及目录覆盖缺口。
/// 预期结果：旧记录不因24小时成果窗口消失，不编造第二项目；收窄不将未知归属记录塞入project-a。
#[test]
fn unknown_project_history_survives_whole_scope_but_is_not_guessed_into_project_filter() {
    let mut data = facts();
    data.scope.authorized_area_ids.push("area-missing".into());
    let mut old = history("old-record", EvidenceSource::ProjectLedger);
    old.area_id = "area-missing".into();
    old.evidence.observed_at_unix_ms = Some(1);
    data.unresolved_history.push(old);
    let snapshot =
        aggregate_workspace_progress(query(), data.clone()).expect("目录缺失仍有部分结果");
    assert_eq!(
        snapshot.totals.unresolved_historical_record_count, 1,
        "窗口外历史待核对事项持续展示"
    );
    assert_eq!(snapshot.totals.project_count, 1, "未知目录不编造新项目");
    let unknown = snapshot
        .projects
        .iter()
        .find(|p| !p.project_identity_known)
        .expect("原授权区域归属待核对分组存在");
    assert_eq!(
        unknown.unresolved_history[0].area_id, "area-missing",
        "保留旧记录原授权区域身份"
    );
    assert_eq!(
        unknown.unresolved_history[0].evidence.observed_at_unix_ms,
        Some(1),
        "旧时间不得刷新为查询时间"
    );
    assert_eq!(unknown.totals.role_count, 0, "未知历史称呼不产生角色");
    assert_eq!(
        unknown.totals.all_known_logical_task_count, 0,
        "未知历史不产生跟踪任务"
    );
    assert!(!snapshot.coverage_complete, "目录缺口必须保留");
    let filtered = aggregate_workspace_progress(
        ProgressQuery {
            project_id: Some("project-a".into()),
            ..query()
        },
        data,
    )
    .expect("收窄查询可返回部分结果");
    assert_eq!(
        filtered.totals.unresolved_historical_record_count, 0,
        "无目录不得猜属于指定项目"
    );
    assert!(
        !filtered.coverage_complete,
        "收窄不能掩盖未知目录可能属于指定项目的缺口"
    );
}

/// 场景：同原来源记录的重复采样按单调原序号去重；不同来源的同编号不猜合并，输入顺序不影响结果。
/// 数据构造：ProjectLedger record-1有sequence=1的新写旧版本和sequence=2的旧观察最新版本；
/// WorkspaceLedger也有record-1但无明确跨来源关联，3输入只能形成2原记录。
/// 执行过程：正序与反序聚合相同事实，检查保留的观察时间、摘要和完整计数。
/// 预期结果：保留sequence=2原证据，不将后写快照冒充更新；异来源记录独立且全结果稳定。
#[test]
fn original_record_revisions_are_order_independent_and_source_scoped() {
    let mut data = facts();
    let mut stale = history("record-1", EvidenceSource::ProjectLedger);
    stale.evidence.source_sequence = Some(1);
    stale.evidence.observed_at_unix_ms = Some(query().queried_at_unix_ms);
    stale.summary = Some("后写的旧版本".into());
    let mut latest = stale.clone();
    latest.evidence.source_sequence = Some(2);
    latest.evidence.observed_at_unix_ms = Some(100);
    latest.summary = Some("原序号最新版本".into());
    data.unresolved_history = vec![
        stale,
        latest,
        history("record-1", EvidenceSource::WorkspaceLedger),
    ];
    let forward = aggregate_workspace_progress(query(), data.clone()).expect("重复采样可聚合");
    data.unresolved_history.reverse();
    let reversed = aggregate_workspace_progress(query(), data).expect("反序输入可聚合");
    assert_eq!(forward, reversed, "不能按输入先后猜历史证据顺序");
    assert_eq!(
        forward.totals.unresolved_historical_record_count, 2,
        "同来源去重，异来源无证明关联不合并"
    );
    let project_record = forward.projects[0]
        .unresolved_history
        .iter()
        .find(|r| r.evidence.source == EvidenceSource::ProjectLedger)
        .expect("项目原来源记录存在");
    assert_eq!(
        project_record.evidence.source_sequence,
        Some(2),
        "原单调序号优先后写快照时间"
    );
    assert_eq!(
        project_record.evidence.observed_at_unix_ms,
        Some(100),
        "保持最新原版本的旧观察时间"
    );
    assert_eq!(
        project_record.summary.as_deref(),
        Some("原序号最新版本"),
        "旧版本不得覆盖原来源最新版本"
    );
    assert_eq!(
        forward.totals.recent_task_outcome_count, 0,
        "重复旧历史仍不能创造近期任务成果"
    );
}

fn history(reference: &str, source: EvidenceSource) -> UnresolvedHistoricalFact {
    UnresolvedHistoricalFact {
        workspace_id: "space-a".into(),
        area_id: "area-a".into(),
        record_ref: reference.into(),
        name_at_observation: Some("Betty".into()),
        title: "仅旧称呼任务".into(),
        summary: None,
        reason: "原记录缺少稳定卡片/任务目标".into(),
        evidence: UnresolvedHistoricalEvidence {
            source,
            kind: EvidenceKind::DisplayOnly,
            observed_at_unix_ms: None,
            source_sequence: None,
            freshness: FactFreshness::NeedsVerification,
            quality: vec![],
        },
    }
}
