use crate::app::MultisoulApp;
use gpui::{AppContext, TestAppContext};
use norma::agent_hook::QuestionHookStatus;
use norma::protocol::QuestionState;
use std::time::{Duration, Instant};

/// 场景：概览采用答案必须操作来源项目的原题，不能依赖当前焦点或同名卡片。
/// 构造：两个项目实体各有同node编号的Claude问题，宿主汇总到同一工作空间。
/// 执行：把A题误交给B（拒绝），再通过生产概览采用入口提交到A。
/// 预期：概览保留原题及建议；只有A原Hook收到一次答案，B仍Pending，没有新任务。
#[test]
fn overview_question_submission_targets_owning_project_and_original_question() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths.clone(),
        )
    });
    let decoy = cx.new(|cx| {
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
    let (target, ticket, answers) = source.update(cx, |app, cx| {
        let result = super::tests::make_question(app, cx, dir.path());
        app.propose_question_answer(
            result.0.clone(),
            result.2.clone(),
            "Use the requested label".into(),
        )
        .unwrap();
        result
    });
    let (_, other_ticket, _) = decoy.update(cx, |app, cx| {
        let result = super::tests::make_question(app, cx, dir.path());
        app.shared_context_scope.as_mut().unwrap().area_id = "b".into();
        result
    });
    let work = host.update(cx, |app, cx| {
        app.desktop_workspace.active_id = Some("w".into());
        app.desktop_workspace
            .sessions
            .insert(("w".into(), "a".into()), source.clone());
        app.desktop_workspace
            .sessions
            .insert(("w".into(), "b".into()), decoy.clone());
        app.refresh_brain_overview(cx);
        assert_eq!(
            app.brain_overview().questions.len(),
            2,
            "两个原题各有一个待处理项"
        );
        let row = app
            .brain_overview()
            .questions
            .iter()
            .find(|row| row.snapshot.question.target == target)
            .unwrap();
        assert_eq!(
            row.suggestion.as_ref().unwrap().answers,
            answers,
            "建议与原题绑定显示"
        );
        app.submit_overview_question("b", target.clone(), answers.clone(), cx)
            .detach();
        app.submit_overview_question("a", target.clone(), answers, cx)
    });
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
        assert!(Instant::now() < deadline, "概览采用必须有界完成");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        ticket.status(Instant::now()),
        QuestionHookStatus::Ready,
        "采用原题确实回填A的Hook"
    );
    assert_eq!(
        other_ticket.status(Instant::now()),
        QuestionHookStatus::Pending,
        "错误项目不能误答B题"
    );
    source.read_with(cx, |app, _| {
        assert_eq!(
            app.runtime.question(&target).unwrap().state,
            QuestionState::Submitted,
            "界面操作沿用同一问题身份"
        );
        assert!(
            app.agent_task_coordinator
                .as_ref()
                .is_none_or(|coordinator| coordinator.tasks().is_empty()),
            "答题不派新任务"
        );
    });
}

/// 场景：用户开始手动提交时，尚未消费的幕僚委托必须撤销，即使手动答案尚不完整。
/// 构造：来源项目持有真实原题与一次委托；桌面宿主通过area路由操作该项目。
/// 执行：用户提交不完整答案后，模拟模型迟到提交完整答案。
/// 预期：手动校验失败仍保留原题，但模型无权抢答，Hook保持Pending。
#[test]
fn manual_question_submission_revokes_unconsumed_delegation() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
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
    let (target, ticket, answers) = source.update(cx, |app, cx| {
        let result = super::tests::make_question(app, cx, dir.path());
        app.authorize_question_delegation(&result.0).unwrap();
        result
    });
    host.update(cx, |app, cx| {
        app.desktop_workspace.active_id = Some("w".into());
        app.desktop_workspace
            .sessions
            .insert(("w".into(), "a".into()), source.clone());
        app.submit_overview_question("a", target.clone(), vec![], cx)
            .detach();
    });
    source.update(cx, |app, cx| {
        assert!(
            app.answer_question_from_brain(target.clone(), answers, cx)
                .is_err(),
            "手动操作已撤销代答资格，迟到模型不得抢答"
        );
        assert_eq!(
            app.runtime.question(&target).unwrap().state,
            QuestionState::Pending,
            "不完整手动答案不关闭或占有原题"
        );
        assert_eq!(
            ticket.status(Instant::now()),
            QuestionHookStatus::Pending,
            "尚未向原Hook提交任何答案"
        );
    });
}
