use super::*;

/// 场景：询问能力、引用、否定与未来授权不能被当成当前原题的明确委托。
/// 构造：精确命令及常见含同样词语但不构成命令的文本。
/// 执行：对宿主输入分类器逐项判定。
/// 预期：只接受完整肯定命令，不能通过子串匹配扩大用户授权。
#[test]
fn question_user_intent_requires_an_explicit_current_question_command() {
    for text in [
        "这题你直接回答",
        " 这题你直接回答。 ",
        "当前问题你直接回答！",
    ] {
        assert!(explicit_delegation(text), "明确当前题命令应接纳：{text}");
    }
    for text in [
        "如果这题你直接回答，会怎样？",
        "不要这题你直接回答",
        "‘这题你直接回答’是什么意思",
        "这题你直接回答吗？",
        "以后所有问题你直接回答",
        "这里是日志：这题你直接回答",
    ] {
        assert!(
            !explicit_delegation(text),
            "不能把假设/引用/否定扩大为授权：{text}"
        );
    }
}

/// 场景：用户明确选中另一张卡片时，“这题”不能悄悄指向唯一有问题的旧卡片。
/// 构造：一张Claude原题卡片及另一张无题卡片，幕僚通道已就绪。
/// 执行：选中无题卡片后发明确代答命令；再清空选择并以唯一原题重试。
/// 预期：前者拒绝且无授权/事件；后者只授予原题一次资格。
#[test]
fn question_user_intent_never_substitutes_another_selected_card() {
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
    source.update(cx, |app, cx| {
        let (target, _, _) = super::super::tests::make_question(app, cx, dir.path());
        app.brain.host = Some(crate::app::brain_host::BrainHost {
            service: None,
            status: std::sync::mpsc::channel().1,
            token: "user-input".into(),
        });
        app.brain.pending_events.borrow_mut().clear();
        let other = app
            .runtime
            .spawn_agent(
                canvas::AgentProfile::Claude,
                canvas::CanvasRect::new(800., 0., 640., 400.),
                80,
                24,
            )
            .unwrap();
        app.runtime.focus.exit_to_canvas();
        app.runtime.selection.select_only(other);
        assert!(
            app.question_delegation_from_user("这题你直接回答")
                .unwrap()
                .is_err(),
            "有明确选择时不能退回别的卡片"
        );
        assert!(!app.question_delegated(&target), "错误目标没有授予旧题授权");
        assert!(
            app.brain.pending_events.borrow().is_empty(),
            "不向幕僚发送错误题目"
        );
        app.runtime.selection.clear();
        assert!(
            app.question_delegation_from_user("这题你直接回答")
                .unwrap()
                .is_ok(),
            "无选择且仅一道原题时目标明确"
        );
        assert!(app.question_delegated(&target), "仅明确原题获得一次委托");
    });
}

/// 场景：原会话未确认的Unknown历史不能使新会话的唯一当前题被误认为多题。
/// 构造：同一原生Claude进程，旧题提交后SessionStart切换，随后登记新session的新题。
/// 执行：在同一张卡片上发明确代答命令。
/// 预期：旧记录保留Unknown供对账，但授权只绑定新题，不扩大到历史会话。
#[test]
fn question_user_intent_ignores_retired_session_unknown_receipts() {
    use norma::agent_hook::{AgentHookEvent, HookQuestion, HookQuestionRequest};
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
    source.update(cx, |app, cx| {
        let (old, ticket, answers) = super::super::tests::make_question(app, cx, dir.path());
        let submission = app
            .runtime
            .claim_question_answer(&old, answers, "old-delivery")
            .unwrap()
            .unwrap();
        app.runtime.deliver_native_question(&submission).unwrap();
        let mut event = AgentHookEvent {
            node_id: old.node_id,
            profile: old.provider,
            control_run_id: Some(old.run_id.clone()),
            provider_event: "SessionStart".into(),
            session_id: Some("fresh-session".into()),
            turn_id: None,
            tool_name: None,
            message: None,
            raw_json: serde_json::json!({"source":"clear"}),
        };
        assert!(
            app.runtime.observe_mobile_hook(&event),
            "同原进程切到新会话"
        );
        event.provider_event = "PreToolUse".into();
        event.tool_name = Some("AskUserQuestion".into());
        event.raw_json = ticket.request().event.raw_json.clone();
        event.raw_json["session_id"] = "fresh-session".into();
        event.raw_json["tool_use_id"] = "fresh-call".into();
        let hook = HookQuestion::new(
            HookQuestionRequest {
                run_id: old.run_id.clone(),
                event,
            },
            Instant::now(),
        )
        .unwrap();
        let current = app.runtime.accept_native_question(hook, "project").unwrap();
        assert_eq!(
            app.runtime.question_snapshot(&old).unwrap().state,
            QuestionState::Unknown,
            "旧回执仍需对账，不删除历史来隐藏问题"
        );
        app.runtime.focus.exit_to_canvas();
        app.runtime.selection.select_only(old.node_id);
        app.brain.host = Some(crate::app::brain_host::BrainHost {
            service: None,
            status: std::sync::mpsc::channel().1,
            token: "session-current".into(),
        });
        assert!(
            app.question_delegation_from_user("这题你直接回答")
                .unwrap()
                .is_ok(),
            "旧Unknown不是第二道当前题"
        );
        assert!(app.question_delegated(&current), "只授权新会话原题");
        assert!(!app.question_delegated(&old), "不授权历史未确认意图");
    });
}
