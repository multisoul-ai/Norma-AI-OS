use super::*;
use norma::protocol::{
    ConstraintEnforcement, ContextOrigin, ContextStateChange, ContextStateUpdate,
};

/// 场景：用户采用后答案已经排队，但首次写出前用户撤回代答许可。
/// 构造：隔离 App/SQLite 和原 Claude Hook，通过生产授权入口投递到 Ready。
/// 执行：取出待写回答，持久化并安装禁止 AnswerQuestion 的限制，再尝试实际写出。
/// 预期：发送闭包不执行、票据不标已发送；既有本地排队不能绕过新限制。
#[test]
fn queued_question_observes_restriction_before_actual_write() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let mut test = gpui::TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    let (target, ticket, service, work) = source.update(cx, |app, cx| {
        let (target, ticket, answers) = super::tests::make_question(app, cx, dir.path());
        let permit = app.authorize_question_answer(&target, answers).unwrap();
        let work = app.begin_authorized_answer(permit, cx).unwrap();
        (target, ticket, app.shared_context_service().unwrap(), work)
    });
    let (done, completed) = async_channel::bounded(1);
    cx.spawn(move |_| async move {
        work.await;
        done.send(()).await.unwrap();
    })
    .detach();
    let deadline = Instant::now() + Duration::from_secs(5);
    while completed.try_recv().is_err() {
        cx.run_until_parked();
        assert!(Instant::now() < deadline, "本地排队必须有界完成");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        ticket.take_response(Instant::now()).is_some(),
        "回答已排队但尚未写出"
    );
    crate::app::shared_tokio_runtime()
        .unwrap()
        .block_on(service.run(Some("deny-answer".into()), |access| {
            let input = access.record_input("stop", ContextOrigin::Voice, "不要代答问题")?;
            access.update(
                "deny",
                &input.reference,
                &ContextStateUpdate {
                    input: input.reference.clone(),
                    expected_revision: input.reference.revision,
                    change: ContextStateChange::Constraint {
                        key: "no-answer".into(),
                        text: "不要代答问题".into(),
                        supersedes: None,
                        enforcement: ConstraintEnforcement::Enforced {
                            operation: ContextOperation::AnswerQuestion,
                            target: None,
                        },
                    },
                },
            )
        }))
        .unwrap();
    let mut wrote = false;
    let result = ticket.with_response_write(Instant::now(), || {
        wrote = true;
        Ok(true)
    });
    assert!(result.is_err(), "安装限制后不能使用排队时的旧授权");
    assert!(!wrote, "禁止执行实际写出的副作用");
    assert!(!ticket.transport_sent(), "被拒绝的发送不得报告 Submitted");
    source.update(cx, |app, _| {
        let snapshot = app.runtime.question_snapshot(&target).unwrap();
        assert_eq!(
            snapshot.state,
            QuestionState::Failed,
            "明确未发送须显示失败，不永久提交中"
        );
        assert_eq!(
            snapshot.error.as_deref(),
            Some("question_context_restricted"),
            "失败提供可见原因"
        );
        assert!(
            app.runtime.release_question_to_native(&target).is_ok(),
            "用户仍可原卡片处理"
        );
    });
}

/// 场景：Live尾字/插话取消发生在委托已消费之后，仍须阻止旧答案真正写出。
/// 构造：原Claude Hook、携带取消令牌的单题授权；分别在reserve之前及票据排队之后取消。
/// 执行：生产consume→begin→后台准备，再尝试provider写入。
/// 预期：两个阶段均零写出、非Submitted，不能因grant已消费而绕过语音修订。
#[test]
fn voice_question_cancellation_survives_grant_consumption_and_queueing() {
    for queued in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
        let mut test = gpui::TestAppContext::single();
        let (source, cx) = test.add_window_view(|_, cx| {
            MultisoulApp::new_with_pty_factory_and_database_paths(
                cx,
                Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
                paths,
            )
        });
        let token = tokio_util::sync::CancellationToken::new();
        let (target, ticket, work) = source.update(cx, |app, cx| {
            let (target, ticket, answers) = super::tests::make_question(app, cx, dir.path());
            app.install_question_brain_for_test();
            assert!(
                app.question_delegation_from_voice("这题你直接回答", Some(token.clone()))
                    .unwrap()
                    .is_ok(),
                "真实单题委托被接受"
            );
            let permit = app.consume_question_delegation(&target, answers).unwrap();
            let work = app.begin_authorized_answer(permit, cx).unwrap();
            (target, ticket, work)
        });
        if !queued {
            token.cancel();
        }
        let (done, completed) = async_channel::bounded(1);
        cx.spawn(move |_| async move {
            work.await;
            done.send(()).await.unwrap();
        })
        .detach();
        let deadline = Instant::now() + Duration::from_secs(5);
        while completed.try_recv().is_err() {
            cx.run_until_parked();
            assert!(Instant::now() < deadline, "准备有界结束");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            ticket.take_response(Instant::now()).is_some(),
            queued,
            "只在取消前已准备时有待写票据"
        );
        token.cancel();
        let mut writes = 0;
        assert!(
            ticket
                .with_response_write(Instant::now(), || {
                    writes += 1;
                    Ok(true)
                })
                .is_err(),
            "语音取消阻止实际发送"
        );
        assert_eq!(writes, 0, "两个阶段的provider写入均为零");
        assert!(!ticket.transport_sent(), "取消不得显示已发送");
        source.read_with(cx, |app, _| {
            assert_eq!(
                app.runtime.question_snapshot(&target).unwrap().state,
                QuestionState::Failed,
                "不永久卡在提交中"
            )
        });
    }
}
