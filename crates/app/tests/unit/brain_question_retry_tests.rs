use super::*;
use gpui::TestAppContext;
use norma::agent_hook::QuestionHookStatus;

/// 场景：已持久化但未发送的答案重试时遇到上下文读取故障，轮询不能丢掉发送资格。
/// 构造：真实原Hook和隔离SQLite，首次准备中切换scope，第二次临时阻断数据库路径。
/// 执行：每次开始后运行生产问题轮询；前两次失败，恢复条件后第三次提交。
/// 预期：前两次没有Hook响应，最终复用唯一delivery并只产生一次原答案。
#[test]
fn question_prepared_lease_survives_poll_and_context_read_failure() {
    let dir = tempfile::tempdir().unwrap();
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let database = paths.app_db_path().to_path_buf();
    let saved = database.with_extension("saved");
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    let (target, ticket, answers) = source.update(cx, |app, cx| {
        super::tests::make_question(app, cx, dir.path())
    });
    let mut original_delivery = None;
    for attempt in 0..3 {
        if attempt == 1 {
            std::fs::rename(&database, &saved).unwrap();
            std::fs::create_dir(&database).unwrap();
        }
        let work = source.update(cx, |app, cx| {
            app.shared_context_scope.as_mut().unwrap().area_id = "a".into();
            let permit = app
                .authorize_question_answer(&target, answers.clone())
                .unwrap();
            let work = app.begin_authorized_answer(permit, cx).unwrap();
            app.poll_brain_questions();
            if attempt == 0 {
                app.shared_context_scope.as_mut().unwrap().area_id = "away".into();
            }
            work
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
            assert!(
                Instant::now() < deadline,
                "准备阶段必须有界结束，attempt={attempt}"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        if attempt == 1 {
            std::fs::remove_dir(&database).unwrap();
            std::fs::rename(&saved, &database).unwrap();
        }
        source.read_with(cx, |app, _| {
            let snapshot = app.runtime.question(&target).unwrap();
            assert_eq!(
                snapshot.state,
                if attempt == 2 {
                    QuestionState::Submitted
                } else {
                    QuestionState::Failed
                },
                "未发送失败可重试，轮询不能转成未知结果"
            );
            if attempt == 0 {
                original_delivery = snapshot.delivery_id.clone();
            }
            assert_eq!(
                snapshot.delivery_id, original_delivery,
                "三次准备必须沿用首次持久意图身份"
            );
        });
        assert_eq!(
            ticket.status(Instant::now()),
            if attempt == 2 {
                QuestionHookStatus::Ready
            } else {
                QuestionHookStatus::Pending
            },
            "准备失败不发送，恢复后才送达原Hook"
        );
    }
    assert!(
        ticket.take_response(Instant::now()).is_some(),
        "第三次准备成功后有唯一答案"
    );
    assert!(
        ticket.take_response(Instant::now()).is_none(),
        "重试不能重复发送"
    );
}
