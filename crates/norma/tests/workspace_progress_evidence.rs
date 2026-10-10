use norma::workspace_progress::*;
#[path = "support/workspace_progress.rs"]
pub mod support;
use support::*;

/// 场景：多事实来源、显式重试与续聊只形成一个逻辑任务，正文相同的新任务独立计数。
/// 数据构造：root 三来源，retry 显式指向 root 并使用第1次尝试；一个内部步骤及一个独立同正文任务。
/// 执行过程：输入未加载停用角色和 linked retry turn，通过公开聚合入口查询。
/// 预期结果：两个互斥阶段任务，角色保留为1，步骤1/2不增加任务，关联turn不变成直接活动。
#[test]
fn explicit_linkage_deduplicates_sources_retries_turns_and_steps() {
    let mut data = facts();
    let root = task("root", TaskPhase::Accepted, 10);
    data.tasks.push(root.clone());
    let mut project = root.clone();
    project.phase = TaskPhase::Submitted;
    project.evidence = evidence(EvidenceSource::ProjectLedger, 11);
    data.tasks.push(project);
    let mut runtime = root.clone();
    runtime.phase = TaskPhase::Running;
    runtime.evidence = evidence(EvidenceSource::Runtime, 12);
    data.tasks.push(runtime);
    let mut retry = task("retry", TaskPhase::Running, 13);
    retry.logical_task_ref = Some("root".into());
    retry.attempt_sequence = 1;
    retry.steps = Some(ProgressSteps {
        completed: 1,
        total: 2,
        complete_plan: true,
        current_step: Some("验证登录".into()),
    });
    data.tasks.push(retry.clone());
    let mut step = task("step", TaskPhase::Running, 14);
    step.kind = TaskFactKind::InternalStep;
    step.logical_task_ref = Some("root".into());
    data.tasks.push(step);
    data.tasks
        .push(task("independent", TaskPhase::Accepted, 14));
    data.activities.push(CardActivityFact {
        card: card(),
        name_at_observation: "Alan".into(),
        execution: retry.execution.clone().expect("fixture包含确切turn"),
        linked_task_ref: Some("retry".into()),
        linked_attempt_sequence: Some(1),
        phase: TaskPhase::Running,
        title: "修复登录".into(),
        summary: Some("正在验证".into()),
        waiting_reason: None,
        evidence: evidence(EvidenceSource::ProviderHook, 15),
        steps: retry.steps,
        needs_review: false,
        terminal_at_unix_ms: None,
        result_ref: None,
    });
    let result = aggregate_workspace_progress(query(), data).expect("完整已授权事实应该聚合成功");
    assert_eq!(
        result.totals.all_known_logical_task_count, 2,
        "只有root及同正文独立任务两个逻辑任务"
    );
    assert_eq!(result.totals.role_count, 1, "未加载且停用的保留角色仍计数");
    assert_eq!(
        result.totals.all_known_direct_activity_count, 0,
        "已证明关联的turn不重复列为直接活动"
    );
    assert_eq!(
        result.totals.all_known_task_phases.get(&TaskPhase::Running),
        Some(&1),
        "retry归root且仅root正在运行"
    );
    assert_eq!(
        result
            .totals
            .all_known_task_phases
            .get(&TaskPhase::Accepted),
        Some(&1),
        "独立任务保持已接收而非随卡片升级运行"
    );
    assert_eq!(
        result.totals.all_known_task_phases.values().sum::<usize>(),
        2,
        "每个逻辑任务只属于一个阶段"
    );
    let root = result.projects[0]
        .tasks
        .iter()
        .find(|t| t.logical_task_ref == "root")
        .expect("root应可查看");
    assert_eq!(
        root.steps.as_ref().map(|s| (s.completed, s.total)),
        Some((1, 2)),
        "内部步骤只展示其可信分母"
    );
    assert_eq!(
        root.current_role_name.as_deref(),
        Some("Betty"),
        "改名仅更新原持久身份的显示名称"
    );
    assert_eq!(
        root.task_refs,
        vec!["retry", "root"],
        "来源任务与尝试引用应可追溯"
    );
}

/// 场景：原卡片的可信完成不能被晚写的旧run快照覆盖，也不能转给同名新卡片。
/// 数据构造：原卡片已删除，另一个同node/同名新卡片保留；完成证据时间100，旧run快照时间1000。
/// 执行过程：聚合同一task_ref的完成和迟到running观察。
/// 预期结果：只计1个当前角色，历史任务仍完成，导航保留原持久卡片身份，来源时间不变为查询时间。
#[test]
fn terminal_evidence_survives_late_old_run_snapshot_and_name_reuse() {
    let mut data = facts();
    data.cards[0].retained = false;
    let mut replacement = data.cards[0].clone();
    replacement.retained = true;
    replacement.card.card_instance_id = "card-replacement".into();
    replacement.name = "Alan".into();
    data.cards.push(replacement);
    let mut completed = task("root", TaskPhase::Completed, 100);
    completed.evidence.source = EvidenceSource::TurnOutcome;
    completed.result_ref = Some("historical-result".into());
    data.tasks.push(completed.clone());
    let mut late = completed;
    late.phase = TaskPhase::Running;
    late.evidence = evidence(EvidenceSource::PersistedSnapshot, 1000);
    late.evidence.kind = EvidenceKind::Snapshot;
    late.execution.as_mut().expect("fixture应包含run").run_id = "old-run".into();
    data.tasks.push(late);
    let result = aggregate_workspace_progress(query(), data).expect("历史事实应该可聚合");
    let row = &result.projects[0].tasks[0];
    assert_eq!(
        row.phase,
        TaskPhase::Completed,
        "可信终态必须优先于旧run的后来快照"
    );
    assert_eq!(
        result.totals.all_known_task_phases.get(&TaskPhase::Running),
        None,
        "不得将完成任务退回运行"
    );
    assert_eq!(
        result.totals.role_count, 1,
        "删除原卡不计当前角色，只有新卡计数"
    );
    assert_eq!(row.current_role_name, None, "同名新卡不能充当原历史角色");
    assert_eq!(
        row.target.card.card_instance_id, "card-original",
        "历史结果导航必须保留原身份"
    );
    assert_eq!(
        row.target.result_ref.as_deref(),
        Some("historical-result"),
        "可信历史结果入口必须保留"
    );
    assert_eq!(
        row.evidence.observed_at_unix_ms, 100,
        "原证据时间不能改为快照写入或查询时间"
    );
    assert_eq!(row.observations.len(), 2, "被终态压过的观察仍应可追溯");
}

/// 场景：同一尝试的冲突终态与仅有摘要/旧快照的任务诚实标记未知，质量标记不加任务数。
/// 数据构造：conflict含Completed和Failed transition；snapshot仅有未经终态证明的Completed旧快照；plan运行但分母不完整。
/// 执行过程：在公开入口汇总三个逻辑任务并读取互斥阶段及额外质量计数。
/// 预期结果：未知2、运行1；冲突/跟踪不足/过时独立标记，缺完整分母时不展示比例。
#[test]
fn conflicting_or_unproven_outcomes_remain_unknown_without_inflating_counts() {
    let mut data = facts();
    data.tasks.push(task("conflict", TaskPhase::Completed, 100));
    data.tasks.push(task("conflict", TaskPhase::Failed, 101));
    let mut snapshot = task("snapshot", TaskPhase::Completed, 102);
    snapshot.evidence.kind = EvidenceKind::Snapshot;
    snapshot.evidence.freshness = FactFreshness::NeedsVerification;
    snapshot.evidence.quality = vec![ProgressQuality::Untracked];
    data.tasks.push(snapshot);
    let mut plan = task("plan", TaskPhase::Running, 103);
    plan.steps = Some(ProgressSteps {
        completed: 1,
        total: 2,
        complete_plan: false,
        current_step: None,
    });
    data.tasks.push(plan);
    let result = aggregate_workspace_progress(query(), data).expect("不完整事实可返回待核对快照");
    assert_eq!(
        result.totals.all_known_logical_task_count, 3,
        "质量标记不应额外增加任务数"
    );
    assert_eq!(
        result.totals.all_known_task_phases.get(&TaskPhase::Unknown),
        Some(&2),
        "两个缺可靠终态的任务应进入未知槽"
    );
    assert_eq!(
        result
            .totals
            .all_known_task_phases
            .get(&TaskPhase::Completed),
        None,
        "冲突和快照都不能证明完成"
    );
    assert_eq!(
        result
            .totals
            .quality_task_counts
            .get(&ProgressQuality::ConflictingEvidence),
        Some(&1),
        "只给确实矛盾的逻辑任务标冲突"
    );
    assert_eq!(
        result
            .totals
            .quality_task_counts
            .get(&ProgressQuality::Stale),
        Some(&1),
        "旧快照质量独立于未知阶段"
    );
    assert_eq!(
        result
            .totals
            .quality_task_counts
            .get(&ProgressQuality::Untracked),
        Some(&1),
        "跟踪不足只计其逻辑任务一次"
    );
    let conflict = result.projects[0]
        .tasks
        .iter()
        .find(|t| t.logical_task_ref == "conflict")
        .expect("冲突任务仍保留");
    assert!(
        conflict.quality.contains(&ProgressQuality::OutcomeUnknown),
        "终态冲突必须提示结果待核对"
    );
    assert_eq!(
        conflict.target.result_ref, None,
        "无法解释的冲突不能宣称存在确证结果入口"
    );
    let plan = result.projects[0]
        .tasks
        .iter()
        .find(|t| t.logical_task_ref == "plan")
        .expect("缺分母任务仍保留");
    assert_eq!(
        plan.phase,
        TaskPhase::Running,
        "缺步骤分母不丢失可信任务阶段"
    );
    assert_eq!(plan.steps, None, "计划分母不完整时不能输出1/2比例");
}

/// 场景：新尝试成功只改变最新阶段，旧失败/中断/待核对事项不能被隐式清除。
/// 数据构造：root第0次Failed、第1次Interrupted、第2次Unknown、第3次Completed；全部时间在默认窗口之前。
/// 执行过程：同一稳定root下汇总4次明确关联尝试，检查当前阶段及原尝试异常历史。
/// 预期结果：逻辑任务只1且最新完成；旧3异常仍待处理并保留各自证据/原target，不把旧终态当新尝试状态。
#[test]
fn later_success_preserves_old_attempt_attention_without_duplicate_task_counts() {
    let mut data = facts();
    for (attempt, phase) in [
        (0, TaskPhase::Failed),
        (1, TaskPhase::Interrupted),
        (2, TaskPhase::Unknown),
        (3, TaskPhase::Completed),
    ] {
        let mut row = task(&format!("attempt-{attempt}"), phase, 100 + attempt);
        row.logical_task_ref = Some("root".into());
        row.attempt_sequence = attempt;
        row.execution
            .as_mut()
            .expect("fixture包含execution")
            .turn_id = format!("turn-{attempt}");
        data.tasks.push(row);
    }
    let result = aggregate_workspace_progress(query(), data).expect("关联尝试应保留历史异常");
    let row = &result.projects[0].tasks[0];
    assert_eq!(
        result.totals.all_known_logical_task_count, 1,
        "4次尝试仍只有1个逻辑任务"
    );
    assert_eq!(
        result
            .totals
            .all_known_task_phases
            .get(&TaskPhase::Completed),
        Some(&1),
        "最新尝试成功只占一个完成阶段"
    );
    assert_eq!(
        result.totals.all_known_task_phases.get(&TaskPhase::Failed),
        None,
        "旧失败不能额外占互斥阶段"
    );
    assert!(
        row.requires_attention && row.include_in_default_report,
        "窗口外最新完成不能隐式清除旧异常"
    );
    assert_eq!(
        result.totals.attention_task_count, 1,
        "待处理按逻辑任务计数，不能按旧尝试膨胀为3"
    );
    assert_eq!(
        row.attention_history.len(),
        3,
        "旧失败、中断、待核对都应保留，不创造解决状态"
    );
    assert_eq!(
        row.attention_history[0].phase,
        TaskPhase::Failed,
        "失败原阶段应可核对"
    );
    assert_eq!(
        row.attention_history[1].phase,
        TaskPhase::Interrupted,
        "中断原阶段应可核对"
    );
    assert_eq!(
        row.attention_history[2].phase,
        TaskPhase::Unknown,
        "未知原阶段应可核对"
    );
    assert_eq!(
        row.attention_history[0]
            .target
            .execution
            .as_ref()
            .map(|e| e.turn_id.as_str()),
        Some("turn-0"),
        "旧异常入口冻结原尝试turn，不指向新成功turn"
    );
    assert_eq!(
        result.totals.recent_task_outcome_count, 0,
        "旧异常持续展示不改变指定成果窗口"
    );
}

/// 场景：单来源单execution的单调证据序号优先于回拨时钟，跨来源选择不受输入排列影响。
/// 数据构造：Runtime seq2/时间100为等待审批，旧seq1/时间300为运行；ProjectLedger时间200为运行。
/// 执行过程：正序与逆序聚合；先剔除各来源旧版本，再比较保留的观察时间。
/// 预期结果：同来源旧seq1不能复活；两次结果一致，并由仍保留的ProjectLedger时间200证明运行。
#[test]
fn monotonic_source_versions_are_selected_before_cross_source_time_comparison() {
    let mut newer = task("root", TaskPhase::WaitingApproval, 100);
    newer.evidence.source = EvidenceSource::Runtime;
    newer.evidence.source_sequence = 2;
    let mut older = task("root", TaskPhase::Running, 300);
    older.evidence.source = EvidenceSource::Runtime;
    older.evidence.source_sequence = 1;
    let mut ledger = task("root", TaskPhase::Running, 200);
    ledger.evidence.source = EvidenceSource::ProjectLedger;
    let mut data = facts();
    data.tasks = vec![newer, older, ledger];
    let first = aggregate_workspace_progress(query(), data.clone()).expect("来源版本序列应该有效");
    data.tasks.reverse();
    let reversed = aggregate_workspace_progress(query(), data).expect("输入顺序不应改变已知版本");
    assert_eq!(
        first, reversed,
        "不能用非传递的跨来源比较让输入顺序决定阶段"
    );
    assert_eq!(
        first.projects[0].tasks[0].phase,
        TaskPhase::Running,
        "各来源最新版本中时间200的Ledger观察最新"
    );
    assert_eq!(
        first.projects[0].tasks[0].evidence.source,
        EvidenceSource::ProjectLedger,
        "时间300的旧Runtime版本必须先剔除"
    );
    assert_eq!(
        first.projects[0].tasks[0].observations.len(),
        3,
        "旧版本仍作为来源历史保留，而不是覆盖当前状态"
    );
}
