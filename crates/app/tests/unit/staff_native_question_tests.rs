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
        assert_eq!(app.staff.worker_answers.len(), 1, "只跟踪一个已授权答案");
        assert!(app.staff.tasks.is_empty(), "回答不能创建新任务");
    });
    cx.run_until_parked();
}
