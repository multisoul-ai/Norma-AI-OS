use super::*;
use gpui::TestAppContext;

/// 场景：混合题含一项敏感答案时，幕僚建议及理由都不能进入概览。
/// 构造：真实原题连接上登记包含普通题和secret题的新版本，准备含敏感正文的答案/理由。
/// 执行：调用建议入口，并模拟旧缓存已有该建议，再读取生产概览投影。
/// 预期：建议入口拒绝；即便缓存带入，也不向概览暴露答案或理由。
#[test]
fn question_secret_items_reject_suggestions_and_hide_cached_reason() {
    let dir = tempfile::tempdir().unwrap();
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
        let (target, _, mut answers) = super::tests::make_question(app, cx, dir.path());
        let mut question = app.runtime.question(&target).unwrap().question.clone();
        question.target.revision += 1;
        let mut private = question.items[0].clone();
        private.key = "private".into();
        private.secret = true;
        private.allow_custom = true;
        private.prompt = "Enter private value".into();
        question.items.push(private);
        let target = question.target.clone();
        app.runtime.register_question(question).unwrap();
        answers.push(QuestionAnswer {
            key: "private".into(),
            selected: vec![],
            text: Some("private-value".into()),
        });
        let reason = "Use private-value".to_owned();
        assert!(
            app.propose_question_answer(target.clone(), answers.clone(), reason.clone())
                .is_err(),
            "敏感答案不能通过幕僚建议入口暴露"
        );
        app.brain.questions.suggestions.push(AnswerSuggestion {
            target: target.clone(),
            answers,
            reason,
        });
        let rows = app.question_overview_rows("a");
        let row = rows
            .iter()
            .find(|row| row.snapshot.question.target == target)
            .unwrap();
        assert!(
            row.suggestion.is_none(),
            "混合题的旧建议和理由必须整体从概览隐去"
        );
        assert!(!row.can_delegate, "敏感题不提供模型代答入口");
    });
}
