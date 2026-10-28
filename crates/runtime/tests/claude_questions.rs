use canvas::{AgentProfile, CanvasRect};
use norma::agent_hook::{parse_hook_event, HookQuestion, HookQuestionRequest, HookSource};
use norma::protocol::{QuestionAnswer, QuestionRef, QuestionState};
use runtime::{AgentHookRuntimeConfig, AgentTaskState, AgentTaskUpdate, Runtime};
use serde_json::Value;
use std::time::Instant;

#[path = "unit/question_takeover_tests.rs"]
mod takeover;

fn setup() -> (Runtime, HookQuestion, QuestionRef) {
    setup_turn(None)
}

fn setup_turn(turn: Option<&str>) -> (Runtime, HookQuestion, QuestionRef) {
    let mut runtime = Runtime::new_for_workspace("/tmp/claude-question-test");
    runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:49152/agent-hooks".into(),
        token: "test".into(),
        workspace_id: "w".into(),
        workspace_route_id: "area".into(),
        norma_socket: "/tmp/question.sock".into(),
    }));
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::Claude,
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .unwrap();
    let raw: Value = serde_json::from_str(include_str!(
        "../../norma/tests/fixtures/questions/claude-pre-tool-use.json"
    ))
    .unwrap();
    let mut event = parse_hook_event(HookSource::Claude, node, raw).unwrap();
    event.turn_id = turn.map(str::to_owned);
    let run_id = runtime.mobile_agent_run_id(node).unwrap().to_owned();
    event.control_run_id = Some(run_id.clone());
    let mut start = parse_hook_event(
        HookSource::Claude,
        node,
        serde_json::json!({
            "hook_event_name":"SessionStart", "session_id":event.session_id, "source":"startup"
        }),
    )
    .unwrap();
    start.control_run_id = Some(run_id.clone());
    assert!(
        runtime.observe_mobile_hook(&start),
        "测试先经过真实SessionStart身份绑定路径"
    );
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id: node,
        profile: AgentProfile::Claude,
        provider_event: "SessionStart".into(),
        to: AgentTaskState::Thinking,
        task_title: None,
        session_id: event.session_id.clone(),
    });
    let ticket = HookQuestion::new(HookQuestionRequest { run_id, event }, Instant::now()).unwrap();
    let target = runtime
        .accept_native_question(ticket.clone(), "project")
        .unwrap();
    (runtime, ticket, target)
}

/// 场景：工具取消/失败须结束原问题，另一轮同工具ID回执不能结束当前问题。
/// 构造：显式turn-1的真实问题；复用tool_use_id，分别构造其它轮成功和本轮失败。
/// 执行：先投递turn-2成功（拒绝），再投递严格关联的PostToolUseFailure。
/// 预期：旧轮不改变Pending；工具已结束则Cancelled，错误原因区分中断与失败，并解除阻塞。
#[test]
fn correlated_tool_failure_closes_question_but_other_turn_does_not() {
    for interrupted in [true, false] {
        let (mut runtime, ticket, target) = setup_turn(Some("turn-1"));
        let raw = serde_json::from_str(include_str!(
            "../../norma/tests/fixtures/questions/claude-post-tool-use.json"
        ))
        .unwrap();
        let mut post = parse_hook_event(HookSource::Claude, target.node_id, raw).unwrap();
        post.control_run_id = Some(target.run_id.clone());
        post.turn_id = Some("turn-2".into());
        assert!(
            !runtime.observe_native_question_result(&post),
            "另一轮复用同工具ID不得命中本轮问题"
        );
        assert_eq!(
            runtime.question(&target).unwrap().state,
            QuestionState::Pending,
            "错误轮的回执不改变状态"
        );
        let mut raw = ticket.request().event.raw_json.clone();
        raw["hook_event_name"] = "PostToolUseFailure".into();
        raw["is_interrupt"] = interrupted.into();
        raw["error"] = "question tool ended".into();
        let mut failure = parse_hook_event(HookSource::Claude, target.node_id, raw).unwrap();
        failure.control_run_id = Some(target.run_id.clone());
        failure.turn_id = Some("turn-1".into());
        assert!(
            runtime.observe_native_question_result(&failure),
            "本轮工具失败须被接收"
        );
        assert_eq!(
            runtime.question(&target).unwrap().state,
            QuestionState::Cancelled,
            "工具已结束，原问题不能重试"
        );
        assert_eq!(
            runtime.question(&target).unwrap().error.as_deref(),
            Some(if interrupted {
                "question_interrupted"
            } else {
                "question_tool_failed"
            }),
            "区分工具中断与错误"
        );
        assert!(
            !runtime.has_pending_question(target.node_id),
            "明确工具终态解除阻塞"
        );
        assert!(
            ticket.take_response(Instant::now()).is_none(),
            "失败原题不能继续发送答案"
        );
    }
}

/// 场景：原生Claude的等待句柄、Runtime待答和同工具消费回执形成一个闭环。
/// 构造：真实Pre/PostToolUse夹具、录制PTY，以及明确的workspace/run/session身份。
/// 执行：接收Hook、占有答案、提交，取出原Hook响应，然后投递真实PostToolUse。
/// 预期：取走响应只到Submitted；只有原工具确认才Answered，不派发任何新任务。
#[test]
fn claude_ticket_delivers_to_original_hook_and_confirms_consumption() {
    let (mut runtime, ticket, target) = setup();
    let node = target.node_id;
    assert!(
        runtime.native_question_available(&target),
        "原Hook存活时可以准备答案"
    );
    assert!(runtime.has_pending_question(node), "原卡片被真正问题阻塞");
    let answers = vec![QuestionAnswer {
        key: "question-0".into(),
        selected: vec!["option-1".into()],
        text: None,
    }];
    let submission = runtime
        .claim_question_answer(&target, answers, "delivery")
        .unwrap()
        .unwrap();
    runtime.deliver_native_question(&submission).unwrap();
    assert!(
        ticket.take_response(Instant::now()).is_some(),
        "必须通过原等待句柄回填"
    );
    ticket
        .with_response_write(Instant::now(), || Ok(true))
        .unwrap();
    assert_eq!(
        runtime.question(&target).unwrap().state,
        QuestionState::Submitted,
        "Hook取走输出不证明消费"
    );
    let raw: Value = serde_json::from_str(include_str!(
        "../../norma/tests/fixtures/questions/claude-post-tool-use.json"
    ))
    .unwrap();
    let mut post = parse_hook_event(HookSource::Claude, node, raw).unwrap();
    post.control_run_id = Some(target.run_id.clone());
    assert!(
        runtime.observe_native_question_result(&post),
        "同工具真实结果应关联成功"
    );
    assert_eq!(
        runtime.question(&target).unwrap().state,
        QuestionState::Answered,
        "只有消费证据能完成问题"
    );
    assert_eq!(
        runtime.agent_task_state(node),
        Some(AgentTaskState::Thinking),
        "完成答题恢复原任务而非另派任务"
    );
    assert!(
        !runtime.native_question_available(&target),
        "已答请求不能再次发送"
    );
}

/// 场景：代答尚未完整送达时，人工先回答同选项，不能归因给Norma。
/// 构造：真实问答夹具，分别在Ready和部分Responding阶段交接原TUI。
/// 执行：占有并提交Green；可选取走/取消响应，再投递人工Green消费回执。
/// 预期：三种顺序均为AnsweredElsewhere；旧响应不可继续取出或补写。
#[test]
fn native_answer_before_response_completion_cancels_unsent_answer() {
    for phase in 0..3 {
        let (mut runtime, ticket, target) = setup();
        let answers = vec![QuestionAnswer {
            key: "question-0".into(),
            selected: vec!["option-1".into()],
            text: None,
        }];
        let submission = runtime
            .claim_question_answer(&target, answers, "delivery")
            .unwrap()
            .unwrap();
        runtime.deliver_native_question(&submission).unwrap();
        if phase == 1 {
            ticket.cancel();
        }
        if phase == 2 {
            ticket.take_response(Instant::now()).unwrap();
        }
        let raw = serde_json::from_str(include_str!(
            "../../norma/tests/fixtures/questions/claude-post-tool-use.json"
        ))
        .unwrap();
        let mut post = parse_hook_event(HookSource::Claude, target.node_id, raw).unwrap();
        post.control_run_id = Some(target.run_id.clone());
        assert!(
            runtime.observe_native_question_result(&post),
            "原题的人工消费回执应被接收"
        );
        assert_eq!(
            runtime.question(&target).unwrap().state,
            QuestionState::AnsweredElsewhere,
            "尚未完整传输的代答不能认领人工答案，phase={phase}"
        );
        assert!(
            ticket.take_response(Instant::now()).is_none(),
            "已结束的问题不得残留Ready响应"
        );
        assert!(
            ticket
                .with_response_write(Instant::now(), || panic!("旧题不得再写socket"))
                .is_err(),
            "消费回执撤销所有未完成传输"
        );
    }
}

/// 场景：同一原生进程/new换会话后，新问题可答，旧问题和迟到生命周期不能复活。
/// 构造：已登记的问题与原进程control_run_id，带明确新session的SessionStart。
/// 执行：新会话Hook、状态绑定、新PreToolUse；随后重放旧SessionStart。
/// 预期：旧ticket取消，新问题绑定新run但使用原进程传输身份；旧会话无法抢回卡片。
#[test]
fn native_new_session_rotates_question_identity_without_changing_process() {
    let (mut runtime, old_ticket, old_target) = setup();
    let start_raw = serde_json::json!({"hook_event_name":"SessionStart", "session_id":"new-session", "source":"clear"});
    let mut start = parse_hook_event(HookSource::Claude, old_target.node_id, start_raw).unwrap();
    start.control_run_id = Some(old_target.run_id.clone());
    assert!(
        runtime.observe_mobile_hook(&start),
        "同一原进程的明确SessionStart必须能切换会话"
    );
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id: old_target.node_id,
        profile: AgentProfile::Claude,
        provider_event: "SessionStart".into(),
        to: AgentTaskState::Ready,
        task_title: None,
        session_id: Some("new-session".into()),
    });
    assert_eq!(
        old_ticket.status(Instant::now()),
        norma::agent_hook::QuestionHookStatus::Cancelled,
        "切换会话撤销旧问题等待句柄"
    );
    let mut event = old_ticket.request().event.clone();
    event.session_id = Some("new-session".into());
    event.raw_json["session_id"] = "new-session".into();
    let new_ticket = HookQuestion::new(
        HookQuestionRequest {
            run_id: old_target.run_id.clone(),
            event,
        },
        Instant::now(),
    )
    .unwrap();
    let new_target = runtime
        .accept_native_question(new_ticket.clone(), "project")
        .expect("新会话沿用原进程环境也应能登记问题");
    assert_ne!(
        new_target.run_id, old_target.run_id,
        "问题引用必须具有新的会话运行代数"
    );
    assert!(
        runtime.native_question_available(&new_target),
        "新问题可以回答"
    );
    assert!(
        !runtime.native_question_available(&old_target),
        "旧建议不得命中新会话"
    );
    start.session_id = Some(old_target.session_id.clone());
    start.raw_json["session_id"] = old_target.session_id.clone().into();
    assert!(
        !runtime.observe_mobile_hook(&start),
        "迟到的旧SessionStart不能回退到退休会话"
    );
    assert_eq!(
        runtime.agent_session_id(old_target.node_id).as_deref(),
        Some("new-session"),
        "旧Hook不得改绑当前session"
    );
    start.raw_json["source"] = "resume".into();
    assert!(
        !runtime.observe_mobile_hook(&start),
        "未观察到/resume输入时，不采信旧resume Hook"
    );
    let mut ended = new_ticket.request().event.clone();
    ended.provider_event = "PostToolUseFailure".into();
    ended.raw_json["hook_event_name"] = "PostToolUseFailure".into();
    ended.raw_json["error"] = "cancelled by user".into();
    ended.raw_json["is_interrupt"] = true.into();
    assert!(
        runtime.observe_native_question_result(&ended),
        "人工先取消当前问题"
    );
    runtime
        .send_node_input(
            old_target.node_id,
            runtime::NodeInput::Text("/resume".into()),
        )
        .unwrap();
    runtime
        .send_node_input(
            old_target.node_id,
            runtime::NodeInput::Key(terminal::KeyInput {
                key: terminal::Key::Enter,
                ctrl: false,
                alt: false,
                shift: false,
            }),
        )
        .unwrap();
    assert!(
        runtime.observe_mobile_hook(&start),
        "用户实际/resume之后可以回到退休会话"
    );
    assert_eq!(
        runtime.agent_session_id(old_target.node_id).as_deref(),
        Some(old_target.session_id.as_str()),
        "原生恢复与Runtime身份一致"
    );
    assert_ne!(
        runtime.mobile_agent_run_id(old_target.node_id),
        Some(old_target.run_id.as_str()),
        "恢复历史也必须产生新的运行代数"
    );
}

/// 场景：原生工具取消比主线程登记question通道更早被普通Hook通道观察。
/// 构造：同一进程/session的新tool_use_id，失败回执与PreToolUse题面完全对应。
/// 执行：先投递失败回执，再登记原Hook，模拟两个独立队列同一帧的反序。
/// 预期：登记后立即对账为Cancelled，不留下无法处理的WaitingQuestion。
#[test]
fn early_tool_failure_is_reconciled_when_question_arrives() {
    let (mut runtime, previous, _) = setup();
    let mut pre = previous.request().clone();
    pre.event.raw_json["tool_use_id"] = "early-tool".into();
    let mut failure_raw = pre.event.raw_json.clone();
    failure_raw["hook_event_name"] = "PostToolUseFailure".into();
    failure_raw["is_interrupt"] = true.into();
    failure_raw["error"] = "interrupted before host registration".into();
    let mut failure = parse_hook_event(HookSource::Claude, pre.event.node_id, failure_raw).unwrap();
    failure.control_run_id = Some(pre.run_id.clone());
    runtime.observe_native_question_result(&failure);
    let ticket = HookQuestion::new(pre, Instant::now()).unwrap();
    let target = runtime
        .accept_native_question(ticket.clone(), "project")
        .unwrap();
    assert_eq!(
        runtime.question(&target).unwrap().state,
        QuestionState::Cancelled,
        "早到消费终态必须被对账，不能遗留Pending"
    );
    assert_eq!(
        ticket.status(Instant::now()),
        norma::agent_hook::QuestionHookStatus::Cancelled,
        "已结束工具的Hook不可代答"
    );
    assert!(
        !runtime.native_question_available(&target),
        "早到失败后的问题不得显示可提交"
    );
}

/// 场景：Hook超时或人工接管后，原终端仍等待回答，普通任务不能趁机写入。
/// 构造：原生问题登记后撤销等待句柄；原用户随后回答Green，产生同调用工具回执。
/// 执行：取消ticket、尝试claim/普通任务，最后投递人工回答的PostToolUse。
/// 预期：迟到代答无资格，待答阻塞保留；人工消费后标记AnsweredElsewhere并解除阻塞。
#[test]
fn cancelled_hook_keeps_question_blocked_until_native_user_answers() {
    let (mut runtime, ticket, target) = setup();
    ticket.cancel();
    assert!(
        !runtime.native_question_available(&target),
        "已取消的等待句柄不能代答"
    );
    let answers = vec![QuestionAnswer {
        key: "question-0".into(),
        selected: vec!["option-1".into()],
        text: None,
    }];
    assert!(
        runtime
            .claim_question_answer(&target, answers, "late")
            .is_err(),
        "失效Hook不能取得提交资格"
    );
    assert!(
        runtime.has_pending_question(target.node_id),
        "回退原TUI不等于问题已经消失"
    );
    assert!(
        runtime
            .send_agent_task_text(target.node_id, "new task")
            .is_err(),
        "普通任务不能写进回退后的问答"
    );
    let raw: Value = serde_json::from_str(include_str!(
        "../../norma/tests/fixtures/questions/claude-post-tool-use.json"
    ))
    .unwrap();
    let mut post = parse_hook_event(HookSource::Claude, target.node_id, raw).unwrap();
    post.control_run_id = Some(target.run_id.clone());
    assert!(
        runtime.observe_native_question_result(&post),
        "原用户回答也必须结束等待"
    );
    assert_eq!(
        runtime.question(&target).unwrap().state,
        QuestionState::AnsweredElsewhere,
        "不把人工回答冒认为代答成功"
    );
    assert!(
        !runtime.has_pending_question(target.node_id),
        "消费完成后解除输入阻塞"
    );
}
