use canvas::{AgentProfile, CanvasRect};
use norma::agent_hook::{AgentHookEvent, HookQuestion, HookQuestionRequest};
use norma::protocol::{QuestionAnswer, QuestionRequestId, QuestionState};
use runtime::{AgentHookRuntimeConfig, AgentTaskState, AgentTaskUpdate, Runtime};
use serde_json::{json, Value};
use std::time::Instant;

/// 场景：原 RPC 已写出或结果未知时，用户仍需通过原卡片入口查看和接管。
/// 构造：真实 Codex 题面，分别模拟完整发送和写入后断连。
/// 执行：点击对应的 Runtime 释放/定位入口，再尝试重复发送与普通任务输入。
/// 预期：允许 UI 定位，保留原传输结局，既不重发也不把新任务写进待答控件。
#[test]
fn native_navigation_survives_sent_and_unknown_answers() {
    for unknown in [false, true] {
        let (mut runtime, ticket, target, _) = fixture();
        let submission = runtime
            .claim_question_answer(
                &target,
                vec![QuestionAnswer {
                    key: "question-0".into(),
                    selected: vec!["option-1".into()],
                    text: None,
                }],
                "in-flight",
            )
            .unwrap()
            .unwrap();
        runtime.deliver_native_question(&submission).unwrap();
        ticket.take_response(Instant::now()).unwrap();
        let _ = ticket.with_native_send("in-flight", Instant::now(), || {
            if unknown {
                Err(std::io::Error::other("ack lost"))
            } else {
                Ok(true)
            }
        });
        let before = ticket.native_send_status();
        assert!(
            runtime.release_question_to_native(&target).is_ok(),
            "已发送或未知仍必须允许原卡片定位"
        );
        assert_eq!(
            ticket.native_send_status(),
            before,
            "导航不得伪造取消或成功"
        );
        assert!(
            !runtime.native_question_available(&target),
            "原答案不得重复采用"
        );
        assert!(
            runtime.deliver_native_question(&submission).is_err(),
            "原票据不能重新提交"
        );
        assert!(
            runtime
                .send_agent_task_text(target.node_id, "new task")
                .is_err(),
            "未确认原题仍阻止普通任务输入"
        );
    }
}

/// 场景：Codex原题复用宿主的持久化后提交入口，并以同连接精确工具结果确认消费。
/// 构造：真实去标识RPC/source/result，录制PTY，明确node/run/session/connection身份。
/// 执行：登记原题、占有Green、向原ticket响应；错连接回执被拒，正确结果才确认。
/// 预期：RPC数值ID不变，等待期间阻止新任务，写出不算已答，消费后解除阻塞。
#[test]
fn codex_question_ticket_is_bound_to_original_run_connection_and_consumption() {
    let (mut runtime, ticket, target, mut event) = fixture();
    let node = target.node_id;
    assert_eq!(
        target.request_id,
        QuestionRequestId::Number(0),
        "RPC数值0必须原样保留"
    );
    assert_eq!(
        target.connection_id, "codex-relay:instance:1",
        "绑定原relay实例与附着代数"
    );
    assert!(runtime.native_question_available(&target), "当前原题可采用");
    assert!(
        runtime.has_pending_question(node),
        "阻止把下一步当新任务塞入原问题"
    );
    let answers = vec![QuestionAnswer {
        key: "question-0".into(),
        selected: vec!["option-1".into()],
        text: None,
    }];
    let submission = runtime
        .claim_question_answer(&target, answers.clone(), "durable-delivery")
        .unwrap()
        .unwrap();
    runtime.deliver_native_question(&submission).unwrap();
    assert_eq!(
        ticket.take_response(Instant::now()).unwrap(),
        json!({"delivery_id":"durable-delivery","answers":answers}),
        "回填原ticket不写PTY"
    );
    ticket
        .with_response_write(Instant::now(), || Ok(true))
        .unwrap();
    ticket
        .with_native_send("durable-delivery", Instant::now(), || Ok(true))
        .unwrap();
    assert_eq!(
        runtime.question_snapshot(&target).unwrap().state,
        QuestionState::Submitted,
        "写出仍等待消费确认"
    );
    let consumed: Value = serde_json::from_str(include_str!(
        "../../norma/tests/fixtures/questions/codex-consumed.json"
    ))
    .unwrap();
    event.provider_event = "NormaQuestionResult".into();
    event.raw_json = json!({"delivery_id":"durable-delivery","connection_id":"codex-relay:another-instance:1","event":consumed});
    assert!(
        !runtime.observe_native_question_result(&event),
        "另一relay同RPC结果不能命中"
    );
    assert_eq!(
        runtime.question(&target).unwrap().state,
        QuestionState::Submitted,
        "错误回执不改变提交状态"
    );
    event.raw_json["connection_id"] = "codex-relay:instance:1".into();
    assert!(
        runtime.observe_native_question_result(&event),
        "精确原连接原调用的Green确认消费"
    );
    assert_eq!(
        runtime.question(&target).unwrap().state,
        QuestionState::Answered,
        "消费后才已回答"
    );
    assert!(
        !runtime.has_pending_question(node),
        "完成原问题后解除输入阻塞"
    );
}

fn fixture() -> (
    Runtime,
    HookQuestion,
    norma::protocol::QuestionRef,
    AgentHookEvent,
) {
    let mut runtime = Runtime::new_for_workspace("/tmp/codex-question-test");
    runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:49152/agent-hooks".into(),
        token: "test".into(),
        workspace_id: "w".into(),
        workspace_route_id: "area".into(),
        norma_socket: "/tmp/q.sock".into(),
    }));
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let node = runtime
        .spawn_agent(
            AgentProfile::Codex,
            CanvasRect::new(0., 0., 640., 400.),
            80,
            24,
        )
        .unwrap();
    let run = runtime.mobile_agent_run_id(node).unwrap().to_owned();
    let mut event = AgentHookEvent {
        control_run_id: Some(run.clone()),
        node_id: node,
        profile: norma::protocol::AgentProfile::Codex,
        provider_event: "SessionStart".into(),
        session_id: Some("codex-question-test-thread".into()),
        turn_id: None,
        tool_name: None,
        message: None,
        raw_json: json!({"source":"startup"}),
    };
    assert!(runtime.observe_mobile_hook(&event), "先登记受控原生session");
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id: node,
        profile: AgentProfile::Codex,
        provider_event: "SessionStart".into(),
        to: AgentTaskState::Thinking,
        task_title: None,
        session_id: event.session_id.clone(),
    });
    let request: Value = serde_json::from_str(include_str!(
        "../../norma/tests/fixtures/questions/codex-request.json"
    ))
    .unwrap();
    let source: Value = serde_json::from_str(include_str!(
        "../../norma/tests/fixtures/questions/codex-source.json"
    ))
    .unwrap();
    event.provider_event = "NormaQuestion".into();
    event.tool_name = Some("request_user_input".into());
    event.turn_id = Some("codex-question-test-turn".into());
    event.raw_json =
        json!({"connection_id":"codex-relay:instance:1","request":request,"source":source});
    let ticket = HookQuestion::new(
        HookQuestionRequest {
            run_id: run.clone(),
            event: event.clone(),
        },
        Instant::now(),
    )
    .unwrap();
    let binding = norma::protocol::QuestionRef {
        workspace_id: "w".into(),
        project_id: "p".into(),
        node_id: node,
        provider: norma::protocol::AgentProfile::Codex,
        run_id: run,
        connection_id: "codex-relay:instance:1".into(),
        session_id: event.session_id.clone().unwrap(),
        turn_id: event.turn_id.clone(),
        request_id: QuestionRequestId::Number(0),
        revision: 1,
    };
    runtime.bind_question_connection(&binding).unwrap();
    let target = runtime.accept_native_question(ticket.clone(), "p").unwrap();
    (runtime, ticket, target, event)
}

/// 场景：宿主HTTP回包后，人工抢先提交完全相同的Green，不能归为本次代答成功。
/// 构造：原题ticket已经返回答案，但relay尚未获得发送资格；人工消费结果无delivery归属。
/// 执行：完成HTTP写出后读公开状态，再观察原工具的Green结果。
/// 预期：HTTP写出仍是Submitting；最终为AnsweredElsewhere，不确认宿主持久化意图。
#[test]
fn codex_http_response_does_not_own_an_identical_native_answer() {
    let (mut runtime, ticket, target, mut event) = fixture();
    let answers = vec![QuestionAnswer {
        key: "question-0".into(),
        selected: vec!["option-1".into()],
        text: None,
    }];
    let submission = runtime
        .claim_question_answer(&target, answers, "our-delivery")
        .unwrap()
        .unwrap();
    runtime.deliver_native_question(&submission).unwrap();
    ticket.take_response(Instant::now()).unwrap();
    ticket
        .with_response_write(Instant::now(), || Ok(true))
        .unwrap();
    assert_eq!(
        runtime.question_snapshot(&target).unwrap().state,
        QuestionState::Submitting,
        "HTTP回包不能冒充原RPC已发送"
    );
    event.provider_event = "NormaQuestionResult".into();
    let consumed: Value = serde_json::from_str(include_str!(
        "../../norma/tests/fixtures/questions/codex-consumed.json"
    ))
    .unwrap();
    event.raw_json = json!({"connection_id":target.connection_id,"event":consumed});
    assert!(
        runtime.observe_native_question_result(&event),
        "同连接原工具消费仍应关闭问题"
    );
    assert_eq!(
        runtime.question_snapshot(&target).unwrap().state,
        QuestionState::AnsweredElsewhere,
        "相同文本不是宿主发送归属证据"
    );
}

/// 场景：HTTP已返回，但原RPC还未开始发送时，用户选择回原卡片处理。
/// 构造：同一Codex题与持久化delivery；传输尚未获得ticket取消锁。
/// 执行：返回HTTP后接管，再尝试通过旧ticket写原RPC。
/// 预期：接管撤销旧答案；发送闭包完全不执行，原题继续等待人工。
#[test]
fn codex_takeover_revokes_answer_after_http_before_rpc_write() {
    let (mut runtime, ticket, target, _) = fixture();
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
    ticket.take_response(Instant::now()).unwrap();
    ticket
        .with_response_write(Instant::now(), || Ok(true))
        .unwrap();
    runtime.release_question_to_native(&target).unwrap();
    let mut wrote = false;
    assert!(
        ticket
            .with_native_send("delivery", Instant::now(), || {
                wrote = true;
                Ok(true)
            })
            .is_err(),
        "人工接管后旧ticket不能写RPC"
    );
    assert!(!wrote, "必须在执行任何Provider IO之前取消");
    assert!(
        !runtime.native_question_available(&target),
        "旧建议不能重新采用"
    );
}

/// 场景：当前连接B建立后，同进程会话的旧连接A问题延迟到达。
/// 构造：A原题已登记，宿主独立绑定B；迟到ticket携带A的JSON身份。
/// 执行：登记迟到A，再按当前B登记另一道原题。
/// 预期：A被拒绝且不能改回连接或取消B；B仍可采用。
#[test]
fn delayed_question_cannot_rebind_the_current_native_connection() {
    let (mut runtime, _, target, event) = fixture();
    let mut current = target.clone();
    current.connection_id = "codex-relay:instance:2".into();
    current.request_id = QuestionRequestId::Number(1);
    runtime.bind_question_connection(&current).unwrap();
    let mut newer = event.clone();
    newer.raw_json["connection_id"] = current.connection_id.clone().into();
    newer.raw_json["request"]["id"] = 1.into();
    let run = newer.control_run_id.clone().unwrap();
    let current_ticket = HookQuestion::new(
        HookQuestionRequest {
            run_id: run.clone(),
            event: newer,
        },
        Instant::now(),
    )
    .unwrap();
    let current = runtime.accept_native_question(current_ticket, "p").unwrap();
    let old_ticket =
        HookQuestion::new(HookQuestionRequest { run_id: run, event }, Instant::now()).unwrap();
    assert_eq!(
        runtime.accept_native_question(old_ticket, "p"),
        Err("question_connection_changed"),
        "题目JSON只验证连接，不能建立连接"
    );
    assert!(
        runtime.native_question_available(&current),
        "迟到A不能撤销B的执行资格"
    );
    assert_eq!(
        runtime.question_snapshot(&current).unwrap().state,
        QuestionState::Pending,
        "B仍等待当前用户选择"
    );
}

/// 场景：Codex原会话结束后，迟到的题目和答案不能继续操作该会话。
/// 构造：已登记的原ticket与同run/session的SessionEnd。
/// 执行：观察结束事件，再检查题目可用性并重放旧问题。
/// 预期：问题过期、ticket取消、旧会话的新ticket被拒；不得再写原RPC。
#[test]
fn codex_session_end_revokes_pending_question_and_late_ticket() {
    let (mut runtime, ticket, target, mut event) = fixture();
    let request = ticket.request().clone();
    event.provider_event = "SessionEnd".into();
    event.tool_name = None;
    event.raw_json = json!({});
    assert!(runtime.observe_mobile_hook(&event), "接受当前原生会话结束");
    assert!(
        !runtime.native_question_available(&target),
        "会话结束撤销执行资格"
    );
    assert_eq!(
        runtime.question_snapshot(&target).unwrap().state,
        QuestionState::Expired,
        "未发送题目随会话过期"
    );
    assert_eq!(
        ticket.status(Instant::now()),
        norma::agent_hook::QuestionHookStatus::Cancelled,
        "原等待ticket被取消"
    );
    let late = HookQuestion::new(request, Instant::now()).unwrap();
    assert_eq!(
        runtime.accept_native_question(late, "p"),
        Err("question_session_ended"),
        "迟到请求不能重新开启已结束会话"
    );
}
