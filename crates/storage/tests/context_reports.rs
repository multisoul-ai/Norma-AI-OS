use agent::context::*;
use storage::ContextStore;

fn event(id: &str, sequence: u64, payload: ContextPayload) -> ContextEvent {
    ContextEvent {
        event_id: id.into(),
        scope: ContextScope {
            workspace_id: "w".into(),
            area_id: "a".into(),
            project: ContextProject::Personal,
            task_id: None,
            conversation_id: None,
        },
        source: ContextSource {
            origin: ContextOrigin::Host,
            stream_id: "host-1".into(),
            sequence,
        },
        created_at_ms: 1000,
        related: vec![],
        payload,
    }
}

/// 场景：幕僚汇报有决定来源，Live ACK 不能变成已播放。
/// 构造：等待确认的决定、引用该决定的汇报、发送和 ACK 事件。
/// 执行：依次入库；尝试直接 completed、以及引用另一个画布的汇报。
/// 预期：ACK 保持 acknowledged；不能无播放证据完成；越界关联整个事务失败。
#[test]
fn report_provenance_and_delivery_are_separate_facts() {
    let dir = tempfile::tempdir().unwrap();
    let store = ContextStore::new(dir.path().join("app.sqlite"));
    let decision = event(
        "d1",
        1,
        ContextPayload::Decision {
            status: DecisionStatus::AwaitingConfirmation,
            reason: "需要用户确认发布范围".into(),
        },
    );
    let d = store.append(&decision, None).unwrap();
    let mut report = event(
        "r1",
        2,
        ContextPayload::Report {
            text: "发布操作正在等你确认".into(),
        },
    );
    report.related.push(d.reference.clone());
    let r = store.append(&report, None).unwrap();
    for (id, seq, status) in [
        ("s1", 3, ReportDelivery::Sent),
        ("a1", 4, ReportDelivery::Acknowledged),
    ] {
        store
            .append(
                &event(
                    id,
                    seq,
                    ContextPayload::Delivery {
                        report: r.reference.clone(),
                        status,
                    },
                ),
                None,
            )
            .unwrap();
    }
    let state = store.snapshot(&report.scope, 20).unwrap();
    assert_eq!(
        state.state.reports["r1"].delivery,
        ReportDelivery::Acknowledged,
        "接收确认不能提升为播完"
    );
    assert_eq!(
        state.events[1].event.related,
        vec![d.reference],
        "汇报保留决定的精确引用"
    );
    assert!(
        store
            .append(
                &event(
                    "bad-complete",
                    5,
                    ContextPayload::Delivery {
                        report: r.reference,
                        status: ReportDelivery::Completed
                    }
                ),
                None
            )
            .is_err(),
        "没有播放归属不能标记播完"
    );
    let mut foreign = report.clone();
    foreign.event_id = "r2".into();
    foreign.scope.area_id = "b".into();
    assert!(
        store.append(&foreign, None).is_err(),
        "不能引用其他画布的决定"
    );
    assert_eq!(
        store.snapshot(&report.scope, 20).unwrap().revision,
        4,
        "失败事件不推进版本"
    );
}

/// 场景：未开始播放的队列项取消不能标成“听过”；活动报告不能被索引静默淘汰。
/// 构造：32 条 created 报告填满近期索引，再提交第 33 条。
/// 执行：尝试未播中断和超容量创建；完成最旧一条后重试第 33 条。
/// 预期：未播中断、满容量均明确失败；旧活动报告仍可更新，终态腾出空间。
#[test]
fn active_reports_are_never_evicted_or_mislabelled_as_heard() {
    let dir = tempfile::tempdir().unwrap();
    let store = ContextStore::new(dir.path().join("app.sqlite"));
    let first = store
        .append(
            &event(
                "r0",
                1,
                ContextPayload::Report {
                    text: "第一条".into(),
                },
            ),
            None,
        )
        .unwrap();
    assert!(
        store
            .append(
                &event(
                    "interrupt",
                    2,
                    ContextPayload::Delivery {
                        report: first.reference.clone(),
                        status: ReportDelivery::Interrupted
                    }
                ),
                None
            )
            .is_err(),
        "没有开始播放就不能产生听过的中断记录"
    );
    for i in 1..32 {
        store
            .append(
                &event(
                    &format!("r{i}"),
                    i + 1,
                    ContextPayload::Report {
                        text: "等待播报".into(),
                    },
                ),
                None,
            )
            .unwrap();
    }
    assert!(
        store
            .append(
                &event(
                    "r32",
                    33,
                    ContextPayload::Report {
                        text: "超出容量".into()
                    }
                ),
                None
            )
            .is_err(),
        "活动索引满时应明确失败"
    );
    for (id, seq, status) in [
        ("playing", 33, ReportDelivery::Playing),
        ("done", 34, ReportDelivery::Completed),
    ] {
        store
            .append(
                &event(
                    id,
                    seq,
                    ContextPayload::Delivery {
                        report: first.reference.clone(),
                        status,
                    },
                ),
                None,
            )
            .unwrap();
    }
    store
        .append(
            &event(
                "r32",
                35,
                ContextPayload::Report {
                    text: "可以入队".into(),
                },
            ),
            None,
        )
        .unwrap();
    let state = store.snapshot(&first.event.scope, 100).unwrap();
    assert_eq!(state.state.reports.len(), 32, "仅终态索引可以腾出空间");
    assert!(
        !state.state.reports.contains_key("r0"),
        "已完成最旧报告应移出近期索引"
    );
    assert!(
        store.read(&first.event.scope, &first.reference).is_ok(),
        "移出索引仍保留不可变来源记录"
    );
}

/// 场景：音频先于 transport ACK 开始播放，迟到 ACK 不得丢失或回退播放状态。
/// 构造：一条汇报依次 sent、playing、completed，最后到达 acknowledged。
/// 执行：按实际到达顺序写入并读取同一快照。
/// 预期：保留 ACK 事件作为接收事实，但投影继续为 completed。
#[test]
fn late_transport_ack_does_not_regress_playback() {
    let dir = tempfile::tempdir().unwrap();
    let store = ContextStore::new(dir.path().join("app.sqlite"));
    let first = store
        .append(
            &event(
                "r",
                1,
                ContextPayload::Report {
                    text: "已拦下".into(),
                },
            ),
            None,
        )
        .unwrap();
    for (id, seq, status) in [
        ("sent", 2, ReportDelivery::Sent),
        ("playing", 3, ReportDelivery::Playing),
        ("done", 4, ReportDelivery::Completed),
        ("ack", 5, ReportDelivery::Acknowledged),
    ] {
        store
            .append(
                &event(
                    id,
                    seq,
                    ContextPayload::Delivery {
                        report: first.reference.clone(),
                        status,
                    },
                ),
                None,
            )
            .unwrap();
    }
    let snapshot = store.snapshot(&first.event.scope, 20).unwrap();
    assert_eq!(
        snapshot.state.reports["r"].delivery,
        ReportDelivery::Completed,
        "迟到确认不能回退播放状态"
    );
    assert_eq!(
        snapshot.events.last().unwrap().event.event_id,
        "ack",
        "ACK 仍作为独立事实保存"
    );
}

/// 场景：恢复为 Unknown 的报告已失去播放归属，不能被迟到回调复活，也不能堵死近期索引。
/// 构造：32 条报告全部标为 Unknown，保留第一条不可变引用。
/// 执行：先尝试迟到 Playing / Skipped / Completed，再创建第 33 条报告。
/// 预期：Unknown 是不可回退的未知终态；新报告可创建，旧报告原文仍可按引用读取。
#[test]
fn recovered_unknown_reports_are_terminal_and_release_capacity() {
    let dir = tempfile::tempdir().unwrap();
    let store = ContextStore::new(dir.path().join("app.sqlite"));
    let mut first = None;
    for i in 0..32 {
        let record = store
            .append(
                &event(
                    &format!("r{i}"),
                    i * 2 + 1,
                    ContextPayload::Report {
                        text: "遗留报告".into(),
                    },
                ),
                None,
            )
            .unwrap();
        store
            .append(
                &event(
                    &format!("u{i}"),
                    i * 2 + 2,
                    ContextPayload::Delivery {
                        report: record.reference.clone(),
                        status: ReportDelivery::Unknown,
                    },
                ),
                None,
            )
            .unwrap();
        first.get_or_insert(record.reference);
    }
    let first = first.unwrap();
    for (i, status) in [
        ReportDelivery::Playing,
        ReportDelivery::Skipped,
        ReportDelivery::Completed,
    ]
    .into_iter()
    .enumerate()
    {
        assert!(
            store
                .append(
                    &event(
                        &format!("late-{i}"),
                        65 + i as u64,
                        ContextPayload::Delivery {
                            report: first.clone(),
                            status
                        }
                    ),
                    None
                )
                .is_err(),
            "结果未知的旧报告不能因迟到 {status:?} 恢复播放或伪造确定结果"
        );
    }
    let added = store
        .append(
            &event(
                "new",
                70,
                ContextPayload::Report {
                    text: "新报告".into(),
                },
            ),
            None,
        )
        .unwrap();
    let snapshot = store.snapshot(&first.scope, 100).unwrap();
    assert_eq!(snapshot.state.reports.len(), 32, "未知终态可被近期索引淘汰");
    assert!(
        !snapshot.state.reports.contains_key(&first.event_id),
        "最旧终态应腾出容量"
    );
    assert_eq!(
        snapshot.state.reports[&added.reference.event_id].delivery,
        ReportDelivery::Created,
        "新报告不受旧播放状态影响"
    );
    assert_eq!(
        store.read(&first.scope, &first).unwrap().reference,
        first,
        "索引淘汰不能删除不可变报告来源"
    );
}
