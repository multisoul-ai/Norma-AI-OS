use super::*;
use agent::context::*;
use gpui::{AppContext, TestAppContext};

/// 场景：用户从概览采纳幕僚提议，语音随后需要知道是谁确认、实际交给了哪个任务。
/// 构造：隔离数据库、桌面宿主与来源项目两个实体、echo PTY 卡片和一条待采纳提议。
/// 执行：调用生产概览派发入口，等待整个异步任务结束，再读取来源项目的共享记录。
/// 预期：提议已消费且仅一个任务；决定说明用户采纳，真实接收回执关联决定，宿主范围不被污染。
#[test]
fn brain_proposal_context_acceptance_saves_decision_and_receipt() {
    proposal_acceptance_fixture(false);
}

/// 场景：用户采纳的任务已提交，但随后共享回执保存失败。
/// 构造：沿用真实概览入口与独立实体，SQLite 触发器只拒绝 Receipt。
/// 执行：等待完整采纳流程结束，检查来源、提议、协调器任务与界面反馈。
/// 预期：提示任务已接收而来源未保存；提议保持移除，仅一个任务，不声称已保存回执。
#[test]
fn brain_proposal_context_failed_save_does_not_restore_or_redispatch() {
    proposal_acceptance_fixture(true);
}

fn proposal_acceptance_fixture(fail_receipt: bool) {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let database = paths.app_db_path().to_path_buf();
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths.clone(),
        )
    });
    let host = cx.new(|cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    // 应用构造时会按系统语言重设界面语言，所以必须在实体创建之后再固定。
    let _locale = crate::pinned_locale::PinnedLocale::zh_cn();
    let (service, id) = source.update(cx, |app, cx| {
        app.enter_canvas_for_test(dir.path(), cx);
        app.shared_context_scope = Some(ContextScope {
            workspace_id: "w".into(),
            area_id: "a".into(),
            project: ContextProject::Personal,
            task_id: None,
            conversation_id: None,
        });
        app.brain.proposals_path = Some(dir.path().join("proposals.json"));
        let id = app
            .brain
            .proposals
            .add(
                super::super::proposals::Draft {
                    agent: "Alan".into(),
                    title: "检查边界".into(),
                    task: "检查边界条件".into(),
                    why: "先补验证".into(),
                },
                std::time::SystemTime::now(),
            )
            .unwrap();
        (app.shared_context_service().unwrap(), id)
    });
    source.update_in(cx, |app, window, _| {
        let created = crate::app::execute_agent_card_command(
            app,
            window,
            norma::protocol::CardCommand::CreateAgent {
                name: Some("Alan".into()),
                profile: norma::protocol::AgentProfile::Codex,
                activate: true,
            },
        )
        .unwrap();
        let norma::protocol::CardCommandResult::AgentCreated {
            node_id: Some(node),
            ..
        } = created
        else {
            panic!("必须返回卡片")
        };
        app.pending_agent_submissions.mark_ready(node);
    });
    crate::app::shared_tokio_runtime()
        .unwrap()
        .block_on(service.snapshot())
        .unwrap();
    if fail_receipt {
        rusqlite::Connection::open(&database).unwrap().execute_batch(
            "CREATE TRIGGER fail_proposal_receipt BEFORE INSERT ON shared_context_events WHEN json_extract(NEW.payload,'$.payload.kind')='receipt' BEGIN SELECT RAISE(ABORT,'test_proposal_receipt_failure'); END;"
        ).unwrap();
    }
    let work = host.update(cx, |app, cx| {
        app.desktop_workspace.active_id = Some("w".into());
        app.desktop_workspace
            .sessions
            .insert(("w".into(), "a".into()), source.clone());
        let work = app.dispatch_overview_proposal("a", id, cx);
        assert!(
            matches!(
                app.brain_proposal_feedback("a", id),
                Some(crate::app::ProposalFeedback::Dispatching)
            ),
            "点击后立即显示派发中"
        );
        app.desktop_workspace.active_id = Some("other".into());
        app.refresh_brain_overview(cx);
        app.desktop_workspace.active_id = Some("w".into());
        app.refresh_brain_overview(cx);
        assert!(
            matches!(
                app.brain_proposal_feedback("a", id),
                Some(crate::app::ProposalFeedback::Dispatching)
            ),
            "切换工作空间不能解除未完成派发的防重复状态"
        );
        app.dispatch_overview_proposal("a", id, cx).detach();
        work
    });
    let (done, completed) = async_channel::bounded(1);
    cx.spawn(move |_| async move {
        work.await;
        done.send(()).await.unwrap();
    })
    .detach();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        if completed.try_recv().is_ok() {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "采纳任务应有界完成");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let tasks = source.read_with(cx, |app, _| {
        assert!(
            app.brain.proposals.items().is_empty(),
            "派发成功必须移除提议"
        );
        app.agent_task_coordinator.as_ref().unwrap().tasks()
    });
    assert_eq!(tasks.len(), 1, "只能提交一个任务，不得额外排队重复任务");
    if !fail_receipt {
        assert!(
            host.read_with(cx, |app, _| app.desktop_workspace.error.is_none()),
            "连续点击不能在成功后产生已失效错误"
        );
    }
    assert_ne!(tasks[0].state, "queued", "任务必须实际进入提交状态");
    let snapshot = crate::app::shared_tokio_runtime()
        .unwrap()
        .block_on(service.snapshot())
        .unwrap();
    let decision = snapshot
        .events
        .iter()
        .find(|r| matches!(r.event.payload, ContextPayload::Decision { .. }))
        .expect("采纳决定应进入共享记录");
    let ContextPayload::Decision { status, reason } = &decision.event.payload else {
        unreachable!()
    };
    assert_eq!(*status, DecisionStatus::Allowed, "采纳后允许派发");
    assert!(
        reason.contains("批准") || reason.contains("采纳"),
        "不能冒充幕僚自主决定：{reason}"
    );
    if fail_receipt {
        assert!(
            !snapshot
                .events
                .iter()
                .any(|r| matches!(r.event.payload, ContextPayload::Receipt { .. })),
            "失败写入不能冒充已保存回执"
        );
        assert!(
            host.read_with(cx, |app, _| {
                app.desktop_workspace
                    .error
                    .as_ref()
                    .is_some_and(|e| e.contains("任务已接收") && e.contains("请勿重复派发"))
            }),
            "界面应区别真实任务结果与来源保存失败"
        );
        return;
    }
    let receipt = snapshot
        .events
        .iter()
        .find(|r| matches!(r.event.payload, ContextPayload::Receipt { .. }))
        .expect("必须保留派发真实回执");
    let ContextPayload::Receipt {
        operation_id,
        status,
        ..
    } = &receipt.event.payload
    else {
        unreachable!()
    };
    assert_eq!(operation_id, &tasks[0].task_ref, "应精确对应唯一已派发任务");
    assert_eq!(*status, ReceiptStatus::Accepted, "不能声称任务已经执行完成");
    assert_eq!(
        receipt.event.related,
        vec![decision.reference.clone()],
        "回执应关联用户采纳决定"
    );
    assert!(
        host.read_with(cx, |app, _| app.desktop_workspace.error.is_none()),
        "成功保存不应显示失败"
    );
    let host_service = host.update(cx, |app, _| app.shared_context_service().unwrap());
    assert!(
        crate::app::shared_tokio_runtime()
            .unwrap()
            .block_on(host_service.snapshot())
            .unwrap()
            .events
            .is_empty(),
        "来源不能写进桌面宿主范围"
    );
}

/// 场景：用户第一次点击提议时，来源项目的共享状态还没有被语音或幕僚加载。
/// 构造：隔离库里预存暂停派发规则；新建两个 App 实体，只有项目实体持有该 scope。
/// 执行：从生产概览按钮入口发起派发，推进异步准备，检查返回原因和提议。
/// 预期：第一次点击即读到持久化限制；不能因未初始化误拒绝，也不能绕过限制执行。
#[test]
fn brain_proposal_context_cold_overview_click_loads_source_restrictions() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let store = storage::AppStore::initialize(&paths).unwrap();
    let scope = ContextScope {
        workspace_id: "w".into(),
        area_id: "a".into(),
        project: ContextProject::Personal,
        task_id: None,
        conversation_id: None,
    };
    let access = storage::ContextAccess::new(
        storage::ContextStore::new(paths.app_db_path().to_path_buf()),
        scope.clone(),
    )
    .unwrap();
    let input = access
        .record_input("pause", ContextOrigin::Voice, "不要派发任务")
        .unwrap();
    access
        .update(
            "pause",
            &input.reference,
            &ContextStateUpdate {
                input: input.reference.clone(),
                expected_revision: input.reference.revision,
                change: ContextStateChange::Constraint {
                    key: "pause".into(),
                    text: "不要派发任务".into(),
                    supersedes: None,
                    enforcement: ConstraintEnforcement::Enforced {
                        operation: ContextOperation::DispatchTask,
                        target: None,
                    },
                },
            },
        )
        .unwrap();
    let mut test = TestAppContext::single();
    let (host, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_database_context(cx, paths.clone(), store.clone())
    });
    let source = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
    let id = source.update(cx, |app, _| {
        app.shared_context_scope = Some(scope);
        app.brain
            .proposals
            .add(
                super::super::proposals::Draft {
                    agent: "Alan".into(),
                    title: "待执行".into(),
                    task: "修复问题".into(),
                    why: "用户目标".into(),
                },
                std::time::SystemTime::now(),
            )
            .unwrap()
    });
    host.update(cx, |app, cx| {
        app.desktop_workspace.active_id = Some("w".into());
        app.desktop_workspace
            .sessions
            .insert(("w".into(), "a".into()), source.clone());
        app.dispatch_overview_proposal("a", id, cx).detach();
        assert!(
            app.desktop_workspace.error.is_none(),
            "点击不应同步读取 SQLite 或提前写错误"
        );
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        cx.run_until_parked();
        if let Some(error) = host.read_with(cx, |app, _| app.desktop_workspace.error.clone()) {
            assert!(host.read_with(cx, |app, _| matches!(app.brain_proposal_feedback("a", id),
                Some(crate::app::ProposalFeedback::Failed(message)) if message.contains("context_operation_restricted"))),
                "失败原因必须绑定到原提议，供项目画布内就地展示");
            assert!(
                error.contains("context_operation_restricted"),
                "冷状态准备后应读到限制，实际 {error}"
            );
            assert!(
                !error.contains("uninitialized"),
                "首次点击不能要求用户再试一次"
            );
            assert_eq!(
                source.read_with(cx, |app, _| app.brain.proposals.items().len()),
                1,
                "拒绝后仍保留提议"
            );
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "后台准备后应给出明确结果"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// 场景：点击提议后，在后台读取结束前切换工作区或替换来源项目会话。
/// 构造：两个独立 App 实体，旧会话持有提议；读取完成任务作为确定性结束屏障。
/// 执行：同一前台轮内点击并切换/替换，随后等生产派发任务真正返回。
/// 预期：原提议不被取走，新会话无副作用；不能把迟到回调派到新的来源。
#[test]
fn brain_proposal_context_rejects_late_workspace_or_session_replacement() {
    for replace in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let paths = storage::DatabasePaths::isolated_test(dir.path());
        let store = storage::AppStore::initialize(&paths).unwrap();
        let mut test = TestAppContext::single();
        let (host, cx) = test.add_window_view(|_, cx| {
            MultisoulApp::new_with_database_context(cx, paths.clone(), store.clone())
        });
        let source =
            cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store.clone()));
        let replacement = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        let id = source.update(cx, |app, _| {
            app.brain
                .proposals
                .add(
                    super::super::proposals::Draft {
                        agent: "Alan".into(),
                        title: "旧提议".into(),
                        task: "修复旧任务".into(),
                        why: "用户目标".into(),
                    },
                    std::time::SystemTime::now(),
                )
                .unwrap()
        });
        let work = host.update(cx, |app, cx| {
            app.desktop_workspace.active_id = Some("w".into());
            app.desktop_workspace
                .sessions
                .insert(("w".into(), "a".into()), source.clone());
            let work = app.dispatch_overview_proposal("a", id, cx);
            if replace {
                replacement.update(cx, |app, _| {
                    app.brain
                        .proposals
                        .add(
                            super::super::proposals::Draft {
                                agent: "Alan".into(),
                                title: "恢复的提议".into(),
                                task: "同编号的新会话任务".into(),
                                why: "持久化恢复".into(),
                            },
                            std::time::SystemTime::now(),
                        )
                        .unwrap();
                });
                app.desktop_workspace
                    .sessions
                    .insert(("w".into(), "a".into()), replacement.clone());
            } else {
                app.desktop_workspace.active_id = Some("other".into());
            }
            work
        });
        let (done, completed) = async_channel::bounded(1);
        cx.spawn(move |_| async move {
            work.await;
            let _ = done.send(()).await;
        })
        .detach();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            cx.run_until_parked();
            if completed.try_recv().is_ok() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "异步派发任务必须实际返回"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(
            source.read_with(cx, |app, _| app.brain.proposals.items().len()),
            1,
            "不能消费旧会话的提议"
        );
        assert_eq!(
            replacement.read_with(cx, |app, _| app.brain.proposals.items().len()),
            usize::from(replace),
            "新会话的提议不能被旧派发消费或增加"
        );
        assert!(
            host.read_with(cx, |app, _| app.brain_proposal_feedback("a", id).is_none()),
            "旧会话的反馈不能污染替代会话的同编号提议"
        );
        let error = host.read_with(cx, |app, _| app.desktop_workspace.error.clone());
        if replace {
            assert!(
                error.is_some_and(|e| e.contains("context_scope_mismatch")),
                "会话替换必须明确拒绝"
            );
        } else {
            assert!(error.is_none(), "切到其他工作区后不应收到旧界面的错误");
        }
    }
}
