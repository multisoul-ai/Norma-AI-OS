use super::{
    dispatch::*,
    dispatch_test_support::{prepare_request, with_workspace},
};
use agent::{card_instance::CardInstanceStage, WorkspaceTaskPlan};
use std::{
    sync::{Arc, Mutex},
    task::Poll,
    time::{Duration, Instant},
};

/// 场景：整组接收后数据库被真实写锁阻塞，Armed 和失败落账均不能同步提交。
/// 构造：Alan/Betty 两项完整计划；在 acceptance commit 与 arm 之间暂停执行并持写锁。
/// 执行：公开 submit 遇到 busy_timeout，释放锁后等待后台 journal，重发原来源。
/// 预期：两项均零输入失败且保留安全重试证据；重发只取回旧回执，不残留 Queued 或重放。
#[test]
fn shared_service_retains_every_zero_input_failure_after_acceptance_database_lock() {
    with_workspace(|host, cx, paths, workspace, _, sessions, writes| {
        let service = WorkspaceDispatchService::new(paths.app_db_path().to_owned());
        let body = "让 Alan 和 Betty 修复登录";
        let pending = prepare_request(&host, cx, &service, &workspace.id, "locked-operation", body);
        assert_eq!(pending.cards().len(), 2, "完整原目标为两张卡");
        let plan = WorkspaceTaskPlan::from_response(r#"{"assignments":[{"target_index":0,"text":"第一项"},{"target_index":1,"text":"第二项"}]}"#, &pending.cards()).expect("完整两项计划");
        let source = norma::workspace_operation::WorkspaceOperationSource {
            workspace_id: workspace.id.clone(),
            origin: norma::workspace_operation::WorkspaceOperationOrigin::Desktop {
                host_id: "host".into(),
            },
            operation_id: "locked-operation".into(),
            message_id: "message".into(),
            original_text: body.into(),
        };
        let store = storage::WorkspaceOperationStore::new(paths.app_db_path().to_owned());
        let capture = Arc::new(Mutex::new(None));
        let output = capture.clone();
        let task = host.update(cx, |_, cx| service.submit(pending, plan, cx));
        cx.spawn(async move |_| {
            *output.lock().expect("结果") = Some(task.await);
        })
        .detach();
        let deadline = Instant::now() + Duration::from_secs(5);
        cx.executor().block_test(std::future::poll_fn(|cx| {
            if store.receipt(&source).expect("公开只读回执").is_some() {
                return Poll::Ready(());
            }
            assert!(Instant::now() < deadline, "整组接收必须在有界时间内完成");
            cx.waker().wake_by_ref();
            Poll::Pending
        }));
        let blocker = rusqlite::Connection::open(paths.app_db_path()).expect("写锁连接");
        blocker
            .execute_batch("BEGIN IMMEDIATE")
            .expect("接收后阻塞 arm");
        cx.run_until_parked();
        blocker.execute_batch("ROLLBACK").expect("释放真实写锁");
        let result = capture.lock().expect("结果").take().expect("submit 已返回");
        assert!(
            result.is_ok(),
            "已经接收的组不能因失败落账错误消失: {result:?}"
        );
        let deadline = Instant::now() + Duration::from_secs(8);
        let receipt = loop {
            let receipt = store.receipt(&source).expect("回执").expect("已接收组");
            if receipt
                .assignments
                .iter()
                .all(|item| item.receipt.stage == CardInstanceStage::Failed)
            {
                break receipt;
            }
            assert!(
                Instant::now() < deadline,
                "所有未安装项均须可靠落账，当前 {receipt:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(receipt.assignments.len(), 2, "接收原组不能丢后续项");
        for item in &receipt.assignments {
            assert!(
                item.receipt
                    .failure
                    .as_ref()
                    .is_some_and(|failure| failure.retry_safe),
                "零输入失败须可安全手动重试"
            );
            assert!(item.receipt.bound_turn.is_none(), "零输入不能伪造 turn");
            assert_eq!(
                item.input_phase,
                storage::WorkspaceInputPhase::Queued,
                "失败保留零输入事实"
            );
        }
        let now = Instant::now();
        for (_, session) in &sessions {
            session.update(cx, |app, _| {
                app.advance_card_instance_tasks_at(now);
                app.advance_card_instance_tasks_at(now + Duration::from_millis(101));
            });
        }
        assert!(
            writes
                .iter()
                .all(|record| record.lock().expect("PTY").is_empty()),
            "失败及恢复均不得发送任一正文或 Enter"
        );
        let capture = Arc::new(Mutex::new(None));
        let output = capture.clone();
        let task = host.update(cx, |_, cx| {
            service.prepare(
                WorkspaceDispatchEntry::desktop(
                    source,
                    norma::workspace_dispatch::WorkspaceRoutingMode::Execute,
                    vec![],
                )
                .expect("原来源"),
                cx,
            )
        });
        cx.spawn(async move |_| {
            *output.lock().expect("结果") = Some(task.await);
        })
        .detach();
        cx.run_until_parked();
        match capture
            .lock()
            .expect("結果")
            .take()
            .expect("重发")
            .expect("幂等读取")
        {
            WorkspaceDispatchPreparation::Existing(existing) => {
                assert_eq!(existing, receipt, "重发只返回已落账的原组")
            }
            _ => panic!("不得重新生成投递权"),
        };
    });
}

/// 场景：帧的正文事件遇 journal sender 丢失，随后原 session 停止并从宿主删除。
/// 构造：通过共享服务接收 Betty，注入 writer 断开返回事件的 transport 故障。
/// 执行：发送正文、帧 enqueue、生产 stop_project，再删 session；恢复 writer 后读持久回执。
/// 预期：正文与中断证据跨 session 存活；BodyWritten/待核对且不可安全重试，绝不补 Enter。
#[test]
fn shared_service_frame_evidence_survives_sender_loss_and_session_removal() {
    with_workspace(|host, cx, paths, workspace, _, mut sessions, writes| {
        let service = WorkspaceDispatchService::new(paths.app_db_path().to_owned());
        let pending = super::dispatch_test_support::prepare_betty(
            &host,
            cx,
            &service,
            &workspace.id,
            "lost-writer",
        );
        let plan = WorkspaceTaskPlan::from_response(
            r#"{"assignments":[{"target_index":0,"text":"断开前正文"}]}"#,
            &pending.cards(),
        )
        .expect("完整计划");
        let capture = Arc::new(Mutex::new(None));
        let output = capture.clone();
        let task = host.update(cx, |_, cx| service.submit(pending, plan, cx));
        cx.spawn(async move |_| {
            *output.lock().expect("结果") = Some(task.await);
        })
        .detach();
        cx.run_until_parked();
        let accepted = capture
            .lock()
            .expect("结果")
            .take()
            .expect("提交")
            .expect("接收");
        let mut async_cx = cx.to_async();
        let journal = cx
            .executor()
            .block_test(super::dispatch::journal::shared_journal(
                paths.app_db_path(),
                &mut async_cx,
            ))
            .expect("同一共享 writer");
        journal.simulate_sender_loss();
        let now = Instant::now();
        let (area, session) = sessions.remove(1);
        session.update(cx, |app, cx| {
            app.advance_card_instance_tasks_at(now);
            app.journal_workspace_card_instance_events();
            app.stop_project(cx);
        });
        host.update(cx, |app, _| {
            app.desktop_workspace
                .sessions
                .remove(&(workspace.id.clone(), area.id));
        });
        drop(session);
        drop(journal);
        cx.run_until_parked();
        let store = storage::WorkspaceOperationStore::new(paths.app_db_path().to_owned());
        let deadline = Instant::now() + Duration::from_secs(5);
        let receipt = loop {
            let receipt = store
                .receipt(&accepted.source)
                .expect("回执")
                .expect("原操作");
            if receipt.assignments[0].receipt.stage == CardInstanceStage::OutcomeUnknown {
                break receipt;
            }
            assert!(
                Instant::now() < deadline,
                "删 session 后 writer 仍须保存原证据: {receipt:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(
            receipt.assignments[0].input_phase,
            storage::WorkspaceInputPhase::BodyWritten,
            "不能丢正文证据退回 Armed"
        );
        assert!(
            receipt.assignments[0]
                .receipt
                .failure
                .as_ref()
                .is_some_and(|failure| !failure.retry_safe),
            "已写正文必须禁止安全重试"
        );
        let events = store.events(&workspace.id, 0, 32).expect("原空间持久事件");
        assert!(
            events
                .iter()
                .any(|event| event.evidence == "draft_pasted_enter_pending"),
            "正文事件不能随 session 丢失"
        );
        assert!(
            !events
                .iter()
                .any(|event| event.evidence == "enter_confirmed"),
            "writer 恢复不得补发 Enter"
        );
        assert_eq!(
            *writes[1].lock().expect("B"),
            vec!["断开前正文".as_bytes().to_vec()],
            "原卡只有正文"
        );
        assert!(writes[0].lock().expect("A").is_empty(), "非目标零输入");
    });
}
