use super::*;
use gpui::TestAppContext;
use norma::agent_hook::{parse_hook_event, HookSource, QuestionHookStatus};

/// 场景：持久意图已占有但尚未发出，原题可能在准备完成前或准备失败后结束。
/// 构造：隔离App/SQLite和原Claude Hook；切scope令发送前校验失败，或提前人工消费。
/// 执行：遍历提前人工消费、失败后人工消费、失败后SessionEnd；准备完成后轮询并保存。
/// 预期：每个已占有意图都有正确终态，原Hook从未发送代答，prepared清理不丢失对账资格。
#[test]
fn question_receipt_tracks_unsent_intent_through_takeover_and_expiry() {
    for (early, expired) in [(true, false), (false, false), (false, true)] {
        let dir = tempfile::tempdir().unwrap();
        let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
        let database = paths.app_db_path().to_path_buf();
        let mut test = TestAppContext::single();
        let (source, cx) = test.add_window_view(|_, cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths,
            )
        });
        let (target, ticket, work) = source.update(cx, |app, cx| {
            let (target, ticket, answers) = super::tests::make_question(app, cx, dir.path());
            let permit = app.authorize_question_answer(&target, answers).unwrap();
            let work = app.begin_authorized_answer(permit, cx).unwrap();
            app.shared_context_scope.as_mut().unwrap().area_id = "away".into();
            (target, ticket, work)
        });
        let finish_original = |app: &mut MultisoulApp| {
            if !expired {
                app.runtime.release_question_to_native(&target).unwrap();
            }
            let raw = serde_json::from_str(include_str!(
                "../../../norma/tests/fixtures/questions/claude-post-tool-use.json"
            ))
            .unwrap();
            let mut event = parse_hook_event(HookSource::Claude, target.node_id, raw).unwrap();
            event.control_run_id = Some(target.run_id.clone());
            if expired {
                event.provider_event = "SessionEnd".into();
                assert!(
                    app.runtime.observe_mobile_hook(&event),
                    "原会话关闭事件被接受"
                );
            } else {
                assert!(
                    app.runtime.observe_native_question_result(&event),
                    "原题人工消费被接受"
                );
            }
            assert_eq!(
                app.runtime.question(&target).unwrap().state,
                if expired {
                    QuestionState::Expired
                } else {
                    QuestionState::AnsweredElsewhere
                },
                "未发送意图结束后应保留准确终态"
            );
        };
        if early {
            source.update(cx, |app, _| finish_original(app));
        }
        let (done, completed) = async_channel::bounded(1);
        cx.spawn(move |_| async move {
            work.await;
            done.send(()).await.unwrap();
        })
        .detach();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            cx.run_until_parked();
            if completed.try_recv().is_ok() {
                break;
            }
            assert!(Instant::now() < deadline, "意图准备有界结束");
            std::thread::sleep(Duration::from_millis(1));
        }
        source.update(cx, |app, cx| {
            if !early {
                finish_original(app);
            }
            app.poll_brain_questions();
            app.flush_question_receipts(cx);
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while source.read_with(cx, |app, _| app.brain.questions.receipt_write_pending) {
            cx.run_until_parked();
            assert!(Instant::now() < deadline, "终态保存有界完成");
            std::thread::sleep(Duration::from_millis(1));
        }
        let receipt = QuestionDeliveryStore::new(database)
            .get(&serde_json::to_value(&target).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            receipt.state,
            if expired {
                QuestionDeliveryState::Expired
            } else {
                QuestionDeliveryState::AnsweredElsewhere
            },
            "未发送的已占有意图也必须保存终态，early={early}, expired={expired}"
        );
        assert!(
            ticket.take_response(Instant::now()).is_none(),
            "终态对账从未发送代答"
        );
    }
}

/// 场景：消费证据已确认但SQLite暂时不可写，随后原卡片关闭，重试不能丢掉证据。
/// 构造：隔离App/数据库/真实Claude夹具；原路径临时换成目录制造确定写库失败。
/// 执行：采用并消费原答案，等待保存失败；关闭节点、恢复数据库，只重试保存。
/// 预期：数据库最终Answered；答案传输保持Responded，没有因重试重新发送或永久Unknown。
#[test]
fn question_receipt_retry_survives_original_node_removal() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let database = paths.app_db_path().to_path_buf();
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    let (target, ticket) = source.update(cx, |app, cx| {
        let (target, ticket, answers) = super::tests::make_question(app, cx, dir.path());
        let permit = app.authorize_question_answer(&target, answers).unwrap();
        app.begin_authorized_answer(permit, cx).unwrap().detach();
        (target, ticket)
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while ticket.status(Instant::now()) != QuestionHookStatus::Ready {
        cx.run_until_parked();
        assert!(Instant::now() < deadline, "准备完成后Hook应Ready");
        std::thread::sleep(Duration::from_millis(1));
    }
    ticket.take_response(Instant::now()).unwrap();
    ticket
        .with_response_write(Instant::now(), || Ok(true))
        .unwrap();
    let saved = database.with_extension("saved");
    std::fs::rename(&database, &saved).unwrap();
    std::fs::create_dir(&database).unwrap();
    source.update(cx, |app, cx| {
        let raw = serde_json::from_str(include_str!(
            "../../../norma/tests/fixtures/questions/claude-post-tool-use.json"
        ))
        .unwrap();
        let mut event = parse_hook_event(HookSource::Claude, target.node_id, raw).unwrap();
        event.control_run_id = Some(target.run_id.clone());
        assert!(
            app.runtime.observe_native_question_result(&event),
            "原消费回执先被确认"
        );
        app.flush_question_receipts(cx);
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while source.read_with(cx, |app, _| app.brain.questions.receipt_write_pending) {
        cx.run_until_parked();
        assert!(Instant::now() < deadline, "写库错误必须返回，不能一直挂起");
        std::thread::sleep(Duration::from_millis(1));
    }
    source.update(cx, |app, _| {
        assert!(
            app.brain.questions.receipt_retry_at.is_some(),
            "故障真实进入回执保存重试"
        );
        assert!(
            app.runtime.remove_node(target.node_id),
            "删除原卡片及Runtime记录"
        );
        app.brain.questions.receipt_retry_at = None;
    });
    std::fs::remove_dir(&database).unwrap();
    std::fs::rename(&saved, &database).unwrap();
    source.update(cx, |app, cx| app.flush_question_receipts(cx));
    let deadline = Instant::now() + Duration::from_secs(5);
    while source.read_with(cx, |app, _| app.brain.questions.receipt_write_pending) {
        cx.run_until_parked();
        assert!(Instant::now() < deadline, "保存重试必须有界完成");
        std::thread::sleep(Duration::from_millis(1));
    }
    let receipt = QuestionDeliveryStore::new(database)
        .get(&serde_json::to_value(target).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        receipt.state,
        storage::question_delivery_store::QuestionDeliveryState::Answered,
        "节点关闭后保存重试仍保留已确认消费证据"
    );
    assert_eq!(
        ticket.status(Instant::now()),
        QuestionHookStatus::Responded,
        "重试只写数据库，不改变原答案传输"
    );
}

/// 场景：采用准备完成后用户改回原生回答，消费必须记录为他方而非永久Unknown或我方成功。
/// 构造：隔离App和真实Claude Hook，保留已持久化的Green提交意图。
/// 执行：撤销尚未取走的Hook响应，投递同原题人工消费回执，再运行后台保存。
/// 预期：Runtime和重开数据库均为AnsweredElsewhere；原Hook不能再取出代答。
#[test]
fn question_receipt_persists_native_takeover_outcome() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let database = paths.app_db_path().to_path_buf();
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    let (target, ticket) = source.update(cx, |app, cx| {
        let (target, ticket, answers) = super::tests::make_question(app, cx, dir.path());
        let permit = app.authorize_question_answer(&target, answers).unwrap();
        app.begin_authorized_answer(permit, cx).unwrap().detach();
        (target, ticket)
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while ticket.status(Instant::now()) != QuestionHookStatus::Ready {
        cx.run_until_parked();
        assert!(Instant::now() < deadline, "原题意图准备必须有界");
        std::thread::sleep(Duration::from_millis(1));
    }
    source.update(cx, |app, cx| {
        app.runtime.release_question_to_native(&target).unwrap();
        let raw = serde_json::from_str(include_str!(
            "../../../norma/tests/fixtures/questions/claude-post-tool-use.json"
        ))
        .unwrap();
        let mut event = parse_hook_event(HookSource::Claude, target.node_id, raw).unwrap();
        event.control_run_id = Some(target.run_id.clone());
        assert!(
            app.runtime.observe_native_question_result(&event),
            "原题人工消费应对账"
        );
        assert_eq!(
            app.runtime.question_snapshot(&target).unwrap().state,
            QuestionState::AnsweredElsewhere,
            "撤销后由人工消费不能冒认代答"
        );
        app.flush_question_receipts(cx);
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while source.read_with(cx, |app, _| app.brain.questions.receipt_write_pending) {
        cx.run_until_parked();
        assert!(Instant::now() < deadline, "保存终态有界完成");
        std::thread::sleep(Duration::from_millis(1));
    }
    let receipt = QuestionDeliveryStore::new(database)
        .get(&serde_json::to_value(target).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        receipt.state,
        storage::question_delivery_store::QuestionDeliveryState::AnsweredElsewhere,
        "重开数据库也应保留人工归属，不残留Unknown"
    );
    assert!(
        ticket.take_response(Instant::now()).is_none(),
        "保存回执不重发已撤销代答"
    );
}
