use super::*;

fn scope(area: &str) -> ContextScope {
    ContextScope {
        workspace_id: "restart-workspace".into(),
        area_id: area.into(),
        project: ContextProject::Personal,
        task_id: None,
        conversation_id: None,
    }
}

fn seed(access: &ContextAccess, id: &str, status: ReportDelivery) -> ContextRef {
    let reference = access.report(id, "重启前报告", vec![]).unwrap().reference;
    let steps = match status {
        ReportDelivery::Created => vec![],
        ReportDelivery::Acknowledged => vec![ReportDelivery::Sent, status],
        ReportDelivery::Completed | ReportDelivery::Interrupted => {
            vec![ReportDelivery::Playing, status]
        }
        _ => vec![status],
    };
    for status in steps {
        access
            .host_event(
                &format!("{id}:{status:?}"),
                ContextPayload::Delivery {
                    report: reference.clone(),
                    status,
                },
                vec![reference.clone()],
            )
            .unwrap();
    }
    reference
}

/// 场景：新宿主第一次打开留有未完成播报的数据库，不能将旧状态当成本进程正在播放。
/// 构造：直接用存储层留下五类活动状态和四类确定终态，另一个画布也留一条 Playing。
/// 执行：新建 SharedContext 并读取首个快照，再读取另一画布原始存储。
/// 预期：当前 scope 的活动报告变为 Unknown，终态和来源保留；不恢复其他画布，不重放音频。
#[tokio::test]
async fn report_restart_recovers_only_orphaned_reports_in_current_scope() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("context.sqlite");
    let access = ContextAccess::new(ContextStore::new(path.clone()), scope("a")).unwrap();
    let active = [
        ReportDelivery::Created,
        ReportDelivery::Queued,
        ReportDelivery::Sent,
        ReportDelivery::Acknowledged,
        ReportDelivery::Playing,
    ];
    let mut old = Vec::new();
    for (i, status) in active.into_iter().enumerate() {
        old.push(seed(&access, &format!("active-{i}"), status));
    }
    let mut settled = Vec::new();
    for status in [
        ReportDelivery::Completed,
        ReportDelivery::Skipped,
        ReportDelivery::Interrupted,
        ReportDelivery::Failed,
    ] {
        settled.push((
            seed(&access, &format!("settled-{status:?}"), status),
            status,
        ));
    }
    let foreign = ContextAccess::new(ContextStore::new(path.clone()), scope("b")).unwrap();
    let foreign_report = seed(&foreign, "foreign", ReportDelivery::Playing);
    let before = access.snapshot().unwrap();
    let service = SharedContext::new(path, scope("a")).unwrap();
    let snapshot = service.snapshot().await.unwrap();
    for reference in old {
        assert_eq!(
            snapshot.state.reports[&reference.event_id].delivery,
            ReportDelivery::Unknown,
            "重启前活动报告结果不可猜成完成、跳过或仍在播放"
        );
        assert_eq!(
            access.read(&reference).unwrap().reference,
            reference,
            "恢复必须保留原报告的不可变引用"
        );
    }
    for (reference, status) in settled {
        assert_eq!(
            snapshot.state.reports[&reference.event_id].delivery, status,
            "确定终态不得被重启恢复改写"
        );
    }
    assert_eq!(
        snapshot.revision,
        before.revision + 5,
        "只为五条遗留活动报告增加恢复事实"
    );
    assert_eq!(
        foreign.snapshot().unwrap().state.reports[&foreign_report.event_id].delivery,
        ReportDelivery::Playing,
        "scope 初始化不得遍历或改写其他画布"
    );
    assert_eq!(
        service.snapshot().await.unwrap().revision,
        snapshot.revision,
        "再次读取不应重复恢复或重放旧事件"
    );
}

/// 场景：前台消费者释放后，同进程播放 worker 仍活跃，新消费者不能把它当成重启遗留。
/// 构造：生产 SharedContext 与 delivery handle 将一条报告置为 Playing。
/// 执行：释放原 service，创建同 scope 的新实例读取，再让原 handle 完成播放。
/// 预期：读取保持 Playing 和版本，最终 Completed，不额外生成 Unknown 恢复事实。
#[tokio::test]
async fn report_restart_does_not_recover_live_delivery_from_another_consumer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("context.sqlite");
    let service = SharedContext::new(path.clone(), scope("a")).unwrap();
    let reference = service
        .run(None, |access| access.report("live", "当前播报", vec![]))
        .await
        .unwrap()
        .reference;
    let delivery = service.report_delivery(reference.clone()).unwrap();
    delivery.set(ReportDelivery::Playing);
    delivery.flush().await.unwrap();
    let before = service.snapshot().await.unwrap();
    drop(service);
    // Trigger registry pruning while only the delivery worker retains the original scope.
    let _other = SharedContext::new(path.clone(), scope("b")).unwrap();
    let current = SharedContext::new(path, scope("a")).unwrap();
    let after = current.snapshot().await.unwrap();
    assert_eq!(
        after.revision, before.revision,
        "新消费者不能对活跃报告追加重启事实"
    );
    assert_eq!(
        after.state.reports[&reference.event_id].delivery,
        ReportDelivery::Playing,
        "后台播放仍持有宿主，不能误恢复为 Unknown"
    );
    delivery.set(ReportDelivery::Completed);
    delivery.flush().await.unwrap();
    assert_eq!(
        current.snapshot().await.unwrap().state.reports[&reference.event_id].delivery,
        ReportDelivery::Completed,
        "原播放的真实完成回调仍应有效"
    );
}

/// 场景：初始化恢复在第二条记录提交时失败，不能放行业务，也不能永久标成已恢复。
/// 构造：两个遗留 Playing，SQLite 触发器只允许第一条 Unknown 提交。
/// 执行：首次业务失败后检查实际部分提交，移除故障后重试业务。
/// 预期：失败期间 gate 未就绪、新报告未创建；重试只补余下一条，新报告保持 Created。
#[tokio::test]
async fn report_restart_retries_partial_recovery_before_accepting_new_work() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("context.sqlite");
    let access = ContextAccess::new(ContextStore::new(path.clone()), scope("a")).unwrap();
    let old = [
        seed(&access, "old-1", ReportDelivery::Playing),
        seed(&access, "old-2", ReportDelivery::Playing),
    ];
    let before = access.snapshot().unwrap().revision;
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_second_recovery BEFORE UPDATE ON shared_context_state WHEN EXISTS (SELECT 1 FROM shared_context_events WHERE json_extract(payload,'$.payload.status')='unknown' AND revision < NEW.revision) BEGIN SELECT RAISE(ABORT,'injected recovery failure'); END;").unwrap();
    let service = SharedContext::new(path, scope("a")).unwrap();
    let result = service
        .run(None, |access| access.report("fresh", "新报告", vec![]))
        .await;
    assert!(result.is_err(), "恢复失败必须阻止本次新业务");
    assert!(
        service
            .execute(ContextOperation::DispatchTask, None, || Ok(()))
            .is_err(),
        "部分恢复不能开放执行 gate"
    );
    let partial = access.snapshot().unwrap();
    assert_eq!(partial.revision, before + 1, "首条恢复已提交，第二条应回滚");
    assert_eq!(
        partial.state.reports.len(),
        2,
        "新报告不可越过失败恢复提前保存"
    );
    db.execute_batch("DROP TRIGGER fail_second_recovery;")
        .unwrap();
    let fresh = service
        .run(None, |access| access.report("fresh", "新报告", vec![]))
        .await
        .unwrap()
        .reference;
    let after = service.snapshot().await.unwrap();
    assert_eq!(after.revision, before + 3, "两条恢复与一个新报告各提交一次");
    for reference in old {
        assert_eq!(
            after.state.reports[&reference.event_id].delivery,
            ReportDelivery::Unknown,
            "重试必须补齐遗留状态，不能重放旧音频"
        );
    }
    assert_eq!(
        after.state.reports[&fresh.event_id].delivery,
        ReportDelivery::Created,
        "初始化完成后的新报告不能再次被恢复"
    );
    assert!(
        service
            .execute(ContextOperation::DispatchTask, None, || Ok(()))
            .is_ok(),
        "完整恢复并安装快照后执行 gate 才可就绪"
    );
}

/// 场景：持有真实 delivery worker 的进程在播放期间直接退出，没有运行 Drop 清理。
/// 构造：子测试进程在隔离数据库保存目标和 Playing，随后 exit(73) 跳过析构。
/// 执行：父进程确认磁盘仍为 Playing，再通过生产 SharedContext 首次加载。
/// 预期：恢复为 Unknown，目标与旧来源保留，只增加一条恢复事实；不重放任何音频。
/// 这是进程退出后的持久化恢复证据，不代替断电或扬声器验收。
#[test]
fn report_restart_recovers_after_process_exit_without_drop() {
    const CHILD_PATH: &str = "NORMA_TEST_REPORT_RESTART_DB";
    let runtime = super::super::super::agent_runtime::shared_tokio_runtime().unwrap();
    if let Some(path) = std::env::var_os(CHILD_PATH) {
        let service = SharedContext::new(path.into(), scope("crash")).unwrap();
        let record = runtime
            .block_on(service.run(Some("restart-goal".into()), |access| {
                let input =
                    access.record_input("goal-input", ContextOrigin::Voice, "先完成测试")?;
                access.update(
                    "goal",
                    &input.reference,
                    &ContextStateUpdate {
                        input: input.reference.clone(),
                        expected_revision: input.reference.revision,
                        change: ContextStateChange::Goal {
                            text: "先完成测试".into(),
                            supersedes: None,
                        },
                    },
                )?;
                access.report("crash-report", "正在汇报", vec![input.reference])
            }))
            .unwrap();
        let delivery = service.report_delivery(record.reference).unwrap();
        delivery.set(ReportDelivery::Playing);
        runtime.block_on(delivery.flush()).unwrap();
        std::process::exit(73);
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("context.sqlite");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("app::shared_context::report::restart_tests::report_restart_recovers_after_process_exit_without_drop")
        .arg("--nocapture")
        .env(CHILD_PATH, &path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() {
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("子进程必须在 10 秒内提交并退出，不能留下后台测试进程");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let result = child.wait_with_output().unwrap();
    assert_eq!(
        result.status.code(),
        Some(73),
        "子进程必须在已提交 Playing 后直接退出，不能只启动零条测试：{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let access = ContextAccess::new(ContextStore::new(path.clone()), scope("crash")).unwrap();
    let before = access.snapshot().unwrap();
    let report = before.state.reports.values().next().unwrap();
    assert_eq!(
        report.delivery,
        ReportDelivery::Playing,
        "子进程退出不得先执行 Drop 取消报告"
    );
    let service = SharedContext::new(path, scope("crash")).unwrap();
    let after = runtime.block_on(service.snapshot()).unwrap();
    assert_eq!(
        after.state.reports[&report.reference.event_id].delivery,
        ReportDelivery::Unknown,
        "新进程宿主只确认旧报告失去播放归属"
    );
    assert_eq!(
        after.state.goal, before.state.goal,
        "已提交用户目标必须完整恢复"
    );
    assert_eq!(after.revision, before.revision + 1, "只能追加一次恢复事实");
    assert_eq!(
        access.read(&report.reference).unwrap().event.related.len(),
        1,
        "跨进程恢复保留原报告的用户输入来源"
    );
}
