use crate::app::MultisoulApp;
use gpui::TestAppContext;
use norma::agent_hook::QuestionHookStatus;
use std::time::{Duration, Instant};

/// 场景：明确委托只授予一道原题的一次代答，不等于模型自报用户批准。
/// 构造：隔离App/SQLite/echo PTY与真实Claude题面，准备Green答案。
/// 执行：先无授权尝试、再走宿主委托入口；错误目标/不完整答案拒绝；正确回答后再次尝试。
/// 预期：仅正确原题的一次完整答案进入原Hook，不启动任务，重复委托也不延长/增加资格。
#[test]
fn question_delegation_is_one_shot_and_bound_to_original_question() {
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
    let ticket = source.update(cx, |app, cx| {
        let (target, ticket, answers) = super::tests::make_question(app, cx, dir.path());
        assert!(
            app.answer_question_from_brain(target.clone(), answers.clone(), cx)
                .is_err(),
            "模型令牌不构成用户授权"
        );
        assert!(
            app.authorize_question_delegation(&target).unwrap(),
            "首次明确委托创建资格"
        );
        assert!(
            !app.authorize_question_delegation(&target).unwrap(),
            "重复点击不得增加第二次资格"
        );
        let mut wrong = target.clone();
        wrong.revision += 1;
        assert!(
            app.answer_question_from_brain(wrong, answers.clone(), cx)
                .is_err(),
            "另一版本不可使用本题授权"
        );
        assert!(
            app.answer_question_from_brain(target.clone(), vec![], cx)
                .is_err(),
            "不完整答案不能消耗原题发送资格"
        );
        app.answer_question_from_brain(target.clone(), answers.clone(), cx)
            .unwrap();
        assert!(
            app.answer_question_from_brain(target, answers, cx).is_err(),
            "一次发送后资格立即消耗"
        );
        assert_eq!(
            ticket.status(Instant::now()),
            QuestionHookStatus::Pending,
            "持久化完成前不得回填Hook"
        );
        ticket
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while ticket.status(Instant::now()) == QuestionHookStatus::Pending {
        cx.run_until_parked();
        assert!(Instant::now() < deadline, "明确委托答案必须有界投递");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        ticket.status(Instant::now()),
        QuestionHookStatus::Ready,
        "正确答案进入原Hook一次"
    );
    source.read_with(cx, |app, _| {
        assert!(
            app.agent_task_coordinator
                .as_ref()
                .is_none_or(|c| c.tasks().is_empty()),
            "代答不派新任务"
        )
    });
}

/// 场景：用户委托后切换上下文或在原卡片接管，旧模型答案必须失效。
/// 构造：同一原题和宿主委托；分别改变scope和撤销原Hook。
/// 执行：模型迟到调用答题入口。
/// 预期：两种情形均拒绝，原Hook没有答案，原题输入仍受保护。
#[test]
fn question_delegation_revalidates_scope_and_native_takeover() {
    for takeover in [false, true] {
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
            let (target, ticket, answers) = super::tests::make_question(app, cx, dir.path());
            app.authorize_question_delegation(&target).unwrap();
            if takeover {
                app.runtime.release_question_to_native(&target).unwrap();
            } else {
                app.shared_context_scope.as_mut().unwrap().area_id = "replaced".into();
            }
            assert!(
                app.answer_question_from_brain(target.clone(), answers, cx)
                    .is_err(),
                "原题授权失效后不得发送，takeover={takeover}"
            );
            assert!(
                ticket.take_response(Instant::now()).is_none(),
                "迟到模型没有产生Hook响应"
            );
            assert!(
                app.runtime.has_pending_question(target.node_id),
                "原问题仍需用户处理"
            );
        });
    }
}

/// 场景：幕僚服务更换token后，仍有效的单题委托必须与重新发送的提示一致。
/// 构造：真实原题、用户委托与无外部进程的宿主通道；先轮询，再模拟服务token更换。
/// 执行：两次读取宿主排队的生产QuestionRequested事件。
/// 预期：每个服务只收到一次事件，两次均标明真实委托；概览仍显示委托中。
#[test]
fn question_delegation_survives_brain_restart_with_consistent_prompt() {
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
        let (target, _, _) = super::tests::make_question(app, cx, dir.path());
        app.authorize_question_delegation(&target).unwrap();
        app.brain.host = Some(super::super::BrainHost {service: None, status: std::sync::mpsc::channel().1, token: "first".into()});
        for token in ["first", "replacement"] {
            app.brain.host.as_mut().unwrap().token = token.into();
            app.brain.pending_events.borrow_mut().clear();
            app.poll_brain_questions();
            app.poll_brain_questions();
            let events = app.brain.pending_events.borrow();
            assert_eq!(events.len(), 1, "同一服务不能每帧重复播报原问题");
            assert!(matches!(events.front(), Some(brain::BrainEvent::QuestionRequested {delegated:true, question, ..}) if question.target == target), "服务重启后提示必须携带仍有效的真实委托");
            assert!(app.question_overview_rows("a")[0].delegated, "提示与概览委托状态一致");
        }
    });
}

/// 场景：真实文字/语音UserTurn的明确代答命令要直接授权原题，不依赖模型自行宣称授权。
/// 构造：两次隔离App与Claude原题、幕僚事件通道，不配置前台模型。
/// 执行：分别经submit_project_agent发送Other/Voice“这题你直接回答”。
/// 预期：两入口均创建同题一次委托并通知幕僚，不创建前台模型会话或派发任务。
#[test]
fn question_explicit_text_and_voice_delegate_before_model_dispatch() {
    for input_source in [
        agent::AgentInputSource::Other,
        agent::AgentInputSource::Voice,
    ] {
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
        source.update_in(cx,|app,window,cx| {
            let (target,ticket,_)=super::tests::make_question(app,cx,dir.path());
            app.brain.host=Some(super::super::BrainHost {service:None,status:std::sync::mpsc::channel().1,token:"text-voice".into()});
            app.brain.pending_events.borrow_mut().clear();
            if input_source == agent::AgentInputSource::Voice {
                app.dialogue.begin_single_voice_command("这题你直接回答。").unwrap();
                assert_eq!(app.dialogue.phase(),dialogue::DialoguePhase::Thinking,"语音先进入等待命令结果阶段");
            }
            let response=app.submit_project_agent(agent::AgentCommand::UserTurn {input:"这题你直接回答。".into(),input_source},window,cx)
                .expect("明确原题委托不需要前台模型猜测或另派任务");
            assert!(response.try_recv().unwrap().is_ok(),"宿主立即返回明确接收结果");
            assert!(app.question_delegated(&target),"文字/语音均绑定当前原题");
            assert!(app.agent_runtime.is_none(),"无须创建前台模型任务来取得授权");
            if input_source == agent::AgentInputSource::Voice {
                assert_eq!(app.dialogue.last_reply_text.as_deref(),Some("已委托幕僚回答当前这道题，提交后会核对原 Agent 的消费结果。"),"Thinking阶段语音委托必须收到静态确认，不能静默挂起");
            }
            assert_eq!(ticket.status(Instant::now()),QuestionHookStatus::Pending,"委托本身不编造答案");
            let events=app.brain.pending_events.borrow();
            assert_eq!(events.len(),1,"只发送一次原题委托");
            assert!(matches!(events.front(),Some(brain::BrainEvent::QuestionRequested {question,delegated:true,..}) if question.target==target),"明确目标交给幕僚答题通路");
        });
    }
}
