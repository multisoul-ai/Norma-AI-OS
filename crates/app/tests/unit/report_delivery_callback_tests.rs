use super::*;
use agent::context::ReportDelivery;
use gpui::TestAppContext;

/// 场景：已持久化报告由真实宿主回调从入队转为开播、播完或取消。
/// 构造：隔离存储、同一稳定引用、Playing 的未开播阶段；保留观察 handle 防止 Drop 代替回调。
/// 执行：分别调用未开播取消、mark_started 后取消、mark_started 后正常完成；再投递迟到回调。
/// 预期：得到 Skipped / Interrupted / Completed，迟到回调不能改变终态；本测试不代表真实硬件播放。
#[test]
fn report_delivery_callbacks_distinguish_start_completion_and_cancellation() {
    for (started, completed, expected) in [
        (false, false, ReportDelivery::Skipped),
        (true, false, ReportDelivery::Interrupted),
        (true, true, ReportDelivery::Completed),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let paths = storage::DatabasePaths::isolated_test(dir.path());
        let store = storage::AppStore::initialize(&paths).unwrap();
        let mut test = TestAppContext::single();
        let (app, cx) =
            test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
        let service = app.update(cx, |app, _| app.shared_context_service().unwrap());
        let reference = crate::app::shared_tokio_runtime()
            .unwrap()
            .block_on(service.run(None, |access| access.report("callback", "报告正文", vec![])))
            .unwrap()
            .reference;
        let delivery = service.report_delivery(reference.clone()).unwrap();
        delivery.set(ReportDelivery::Queued);
        let generation = app.update(cx, |app, cx| {
            let mut announcement = AgentAnnouncement::report("报告正文".into());
            announcement.delivery = Some(delivery.clone());
            let generation = app.agent_announcement_playback.generation;
            app.agent_announcement_playback.phase = AgentAnnouncementPlaybackPhase::Playing {
                announcement,
                started: false,
            };
            if started {
                assert!(
                    app.mark_agent_announcement_started(generation),
                    "真实开始回调应被接收一次"
                );
                assert!(
                    !app.mark_agent_announcement_started(generation),
                    "重复开始回调不得重复记账"
                );
            }
            if completed {
                app.finish_agent_announcement_playback(generation, true, cx);
            } else {
                app.cancel_agent_announcements();
            }
            generation
        });
        crate::app::shared_tokio_runtime()
            .unwrap()
            .block_on(delivery.flush())
            .unwrap();
        let snapshot = crate::app::shared_tokio_runtime()
            .unwrap()
            .block_on(service.snapshot())
            .unwrap();
        assert_eq!(
            snapshot.state.reports[&reference.event_id].delivery, expected,
            "入队不等于开播，终态必须由对应回调产生"
        );
        app.update(cx, |app, cx| {
            assert!(
                !app.mark_agent_announcement_started(generation),
                "已结束回合不能被迟到开始回调复活"
            );
            app.finish_agent_announcement_playback(generation, true, cx);
        });
        crate::app::shared_tokio_runtime()
            .unwrap()
            .block_on(delivery.flush())
            .unwrap();
        let snapshot = crate::app::shared_tokio_runtime()
            .unwrap()
            .block_on(service.snapshot())
            .unwrap();
        assert_eq!(
            snapshot.state.reports[&reference.event_id].delivery, expected,
            "迟到完成不得把中断或跳过改为完成"
        );
    }
}

/// 场景：通道未就绪期间，普通任务通知后紧跟带稳定引用的幕僚报告。
/// 构造：两条 deferred 通知，第二条有独立报告 handle；均已具备输出条件。
/// 执行：通过生产 take_ready_batch 连续取出两批。
/// 预期：报告单独交付且保留引用，不因正文合并而被提前标记 Skipped。
#[test]
fn report_delivery_backlog_keeps_report_identity_separate() {
    let dir = tempfile::tempdir().unwrap();
    let service = crate::app::shared_context::SharedContext::new(
        dir.path().join("app.db"),
        agent::context::ContextScope {
            workspace_id: "w".into(),
            area_id: "a".into(),
            project: agent::context::ContextProject::Personal,
            task_id: None,
            conversation_id: None,
        },
    )
    .unwrap();
    let record = crate::app::shared_tokio_runtime()
        .unwrap()
        .block_on(service.run(None, |access| access.report("batch", "独立报告", vec![])))
        .unwrap();
    let delivery = service.report_delivery(record.reference).unwrap();
    delivery.set(ReportDelivery::Queued);
    let mut report = AgentAnnouncement::report("独立报告".into());
    report.delivery = Some(delivery.clone());
    let now = std::time::Instant::now();
    let mut queue = AgentAnnouncements::default();
    for announcement in [AgentAnnouncement::new(7, "Alan, Done"), report] {
        queue.pending.push_back(PendingAgentAnnouncement {
            announcement,
            waiting_for_tts: false,
            deferred: true,
            queued_at: now,
        });
    }
    let (first, _) = queue.take_ready_batch(now).unwrap();
    assert_eq!(
        first.text, "Alan, Done",
        "普通通知不能吞并有稳定引用的报告正文"
    );
    let (second, _) = queue.take_ready_batch(now).expect("报告应保持独立批次");
    assert_eq!(second.text, "独立报告", "报告正文不能丢失");
    assert_eq!(second.delivery, Some(delivery), "稳定引用必须跟随报告交付");
}
