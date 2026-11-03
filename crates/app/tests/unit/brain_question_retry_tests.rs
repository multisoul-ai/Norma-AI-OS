use super::*;
use gpui::TestAppContext;
use norma::agent_hook::QuestionHookStatus;

/// 场景：已持久化但未发送的答案重试时遇到上下文读取故障，轮询不能丢掉发送资格。
/// 构造：真实原Hook和隔离SQLite，首次切换scope，第二次仅给原scope注入无效checkpoint。
/// 执行：每次开始后运行生产问题轮询；前两次失败，恢复条件后第三次提交。
/// 预期：前两次没有Hook响应，最终复用唯一delivery并只产生一次原答案。
#[test]
fn question_prepared_lease_survives_poll_and_context_read_failure() {
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
    let (target, ticket, answers) = source.update(cx, |app, cx| {
        super::tests::make_question(app, cx, dir.path())
    });
    let mut original_delivery = None;
    for attempt in 0..3 {
        let fault = (attempt == 1).then(|| {
            let scope = source.read_with(cx, |app, _| {
                let mut scope = app.shared_context_scope.clone().unwrap();
                scope.area_id = "a".into();
                serde_json::to_string(&scope).unwrap()
            });
            let db = rusqlite::Connection::open(&database).unwrap();
            let existing: u64 = db.query_row(
                "SELECT (SELECT COUNT(*) FROM shared_context_state WHERE scope=?1) + (SELECT COUNT(*) FROM shared_context_events WHERE scope=?1)",
                [&scope], |row| row.get(0),
            ).unwrap();
            assert_eq!(existing, 0, "故障夹具只能占用空scope，不能覆盖或清除真实checkpoint和事件");
            db.execute(
                "INSERT INTO shared_context_state(scope,revision,payload) VALUES(?1,0,?2)",
                [&scope, "\"injected-context-read-failure\""],
            ).unwrap();
            (db, scope)
        });
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
        if let Some((db, scope)) = fault {
            assert_eq!(
                db.execute("DELETE FROM shared_context_state WHERE scope=?1", [&scope])
                    .unwrap(),
                1,
                "准备明确结束后只恢复原scope的故障行，不移动活跃SQLite文件"
            );
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
                "未发送失败可重试，轮询不能转成未知结果；attempt={attempt}；snapshot={snapshot:?}"
            );
            match attempt {
                0 => assert_eq!(snapshot.error.as_deref(), Some("question_authorization_expired"), "首次失败必须来自scope切换"),
                1 => assert!(snapshot.error.as_deref().is_some_and(|error| error.contains("injected-context-read-failure")), "第二次必须经过生产snapshot真实解码失败，不能被其他错误假阳性替代：{snapshot:?}"),
                _ => assert!(snapshot.error.is_none(), "恢复后必须清除准备错误：{snapshot:?}"),
            }
            assert_eq!(app.brain.questions.prepared.len(), usize::from(attempt != 2), "失败时保留唯一准备lease，成功发送后才移除");
            assert_eq!(app.brain.questions.receipts.len(), 1, "三次准备必须只跟踪一份持久回执");
            if attempt == 0 {
                original_delivery = snapshot.delivery_id.clone();
                assert!(original_delivery.as_deref().is_some_and(|id| !id.is_empty()), "首次准备必须生成非空持久delivery身份");
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
