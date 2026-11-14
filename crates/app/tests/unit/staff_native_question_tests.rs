use super::*;
use gpui::TestAppContext;

/// 场景：幕僚镜像和原卡片共用一次回答资格，第二入口不能再次提交。
/// 数据构造：隔离App/SQLite/echo PTY和真实Claude原Hook；答案由现有原题fixture生成。
/// 执行过程：幕僚入口开始回答，原卡授权入口与再次幕僚提交立即重试。
/// 预期结果：原生状态Submitting，另一入口和重复镜像提交都失败，未创建任何新任务。
#[test]
fn staff_mirror_and_native_card_share_first_answer_claim() {
    let dir = tempfile::tempdir().expect("isolated root");
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    source.update(cx, |app, cx| {
        let (target, _ticket, answers) = super::tests::make_question(app, cx, dir.path());
        app.start_staff_worker_answer(&target, answers.clone(), cx)
            .expect("first submission");
        assert_eq!(
            app.runtime
                .question(&target)
                .expect("native question")
                .state,
            QuestionState::Submitting,
            "真实原生所有者持有提交资格"
        );
        assert!(
            app.authorize_question_answer(&target, answers.clone())
                .is_err(),
            "原卡片不能抢第二份资格"
        );
        assert!(
            app.start_staff_worker_answer(&target, answers, cx).is_err(),
            "重复点击不能再次提交"
        );
        let q = norma::protocol::StaffQuestion::new(
            "q".into(),
            app.runtime
                .question(&target)
                .expect("question")
                .question
                .items
                .clone(),
            Some(target.clone()),
        );
        assert_eq!(
            app.staff_question_status(&q),
            "chrome.question_submitting",
            "实际Submitting状态才展示正在提交"
        );
        assert_eq!(app.staff.worker_answers.len(), 1, "只跟踪一个已授权答案");
        assert!(app.staff.tasks.is_empty(), "回答不能创建新任务");
    });
    cx.run_until_parked();
}

/// 场景：原卡接管未答题不能误报已提交。
/// 数据构造：真实原题注册后由用户释放给原卡；执行：读取幕僚卡片使用的状态文案。
/// 预期结果：原生仍Pending，提示去原卡回答，不显示已提交或已回答。
#[test]
fn staff_native_takeover_does_not_claim_an_answer_was_submitted() {
    let dir = tempfile::tempdir().expect("root");
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    source.update(cx, |app, cx| {
        let (target, _ticket, _) = super::tests::make_question(app, cx, dir.path());
        app.runtime
            .release_question_to_native(&target)
            .expect("native takeover");
        let snapshot = app.runtime.question(&target).expect("question");
        let q = norma::protocol::StaffQuestion::new(
            "q".into(),
            snapshot.question.items.clone(),
            Some(target),
        );
        assert_eq!(snapshot.state, QuestionState::Pending, "释放不等于回答");
        assert_eq!(
            app.staff_question_status(&q),
            "chrome.question_native_only",
            "卡片指明真实可用回答入口"
        );
        assert_ne!(
            app.staff_question_status(&q),
            "chrome.question_submitted",
            "未提交不能误报提交成功"
        );
    });
}

/// 场景：Worker答案已取得发送资格，但Hook传输尚未真正写出。
/// 数据构造：隔离App和真实原题fixture；claim后start_delivery，不执行网络发送。
/// 执行过程：读取幕僚镜像卡片的用户可见状态。
/// 预期结果：仍提示正在提交，不能提前声称已提交或已回答。
#[test]
fn staff_mirror_waits_for_transport_before_reporting_submitted() {
    let dir = tempfile::tempdir().expect("root");
    let paths = storage::DatabasePaths::isolated_test(&dir.path().join("data"));
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    source.update(cx, |app, cx| {
        let (target, _ticket, answers) = super::tests::make_question(app, cx, dir.path());
        let submission = app
            .runtime
            .claim_question_answer(&target, answers, "staff-transport")
            .expect("claim")
            .expect("first answer");
        app.runtime
            .start_question_delivery(&submission)
            .expect("prepare delivery");
        let snapshot = app
            .runtime
            .question_snapshot(&target)
            .expect("public question");
        assert_eq!(
            snapshot.state,
            QuestionState::Submitting,
            "未发送时公开状态仍在提交"
        );
        let question =
            norma::protocol::StaffQuestion::new("q".into(), snapshot.question.items, Some(target));
        assert_eq!(
            app.staff_question_status(&question),
            "chrome.question_submitting",
            "镜像遵循真实传输进度"
        );
        assert_ne!(
            app.staff_question_status(&question),
            "chrome.question_submitted",
            "准备发送不等于已发送"
        );
    });
}
