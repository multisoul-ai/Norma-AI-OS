//! Island registration for hook-shaped question tools.
use canvas::{AgentProfile as CanvasProfile, CanvasRect};
use norma::agent_hook::{
    parse_hook_event, ClaudeQuestion, HookQuestion, HookQuestionRequest, HookSource,
};
use norma::protocol::{AgentProfile, QuestionAnswer, QuestionState};
use runtime::{AgentHookRuntimeConfig, AgentTaskState, AgentTaskUpdate, Runtime};
use serde_json::{json, Value};
use std::time::Instant;

fn pre(tool: &str) -> Value {
    json!({
        "hook_event_name": "PreToolUse",
        "tool_name": tool,
        "session_id": "sess-spike",
        "tool_use_id": "call-spike",
        "tool_input": {
            "questions": [{
                "question": "Spike color?",
                "header": "Color",
                "options": [
                    {"label": "Blue", "description": "Blue option"},
                    {"label": "Green", "description": "Green option"}
                ]
            }]
        }
    })
}

fn open(
    profile: CanvasProfile,
    source: HookSource,
    tool: &str,
    native_first: bool,
) -> (Runtime, HookQuestion, norma::protocol::QuestionRef) {
    let mut runtime = Runtime::new_for_workspace("/tmp/hook-question-provider");
    runtime.set_native_first_questions(native_first);
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
        .spawn_agent(profile, CanvasRect::new(0., 0., 640., 400.), 80, 24)
        .unwrap();
    let mut event = parse_hook_event(source, node, pre(tool)).unwrap();
    let run_id = runtime.mobile_agent_run_id(node).unwrap().to_owned();
    event.control_run_id = Some(run_id.clone());
    let mut start = parse_hook_event(
        source,
        node,
        json!({"hook_event_name":"SessionStart","session_id":"sess-spike","source":"startup"}),
    )
    .unwrap();
    start.control_run_id = Some(run_id.clone());
    assert!(
        runtime.observe_mobile_hook(&start),
        "SessionStart 必须先绑定这张卡片"
    );
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id: node,
        profile,
        provider_event: "SessionStart".into(),
        to: AgentTaskState::Thinking,
        task_title: None,
        session_id: Some("sess-spike".into()),
    });
    let ticket = HookQuestion::new(
        HookQuestionRequest {
            run_id,
            event: event.clone(),
        },
        Instant::now(),
    )
    .unwrap();
    let target = runtime
        .accept_native_question(ticket.clone(), "project")
        .unwrap();
    let mut other = parse_hook_event(source, node, pre("Bash")).unwrap();
    other.control_run_id = event.control_run_id.clone();
    other.raw_json["tool_use_id"] = json!("call-bash");
    assert!(
        HookQuestion::new(
            HookQuestionRequest {
                run_id: event.control_run_id.clone().unwrap(),
                event: other,
            },
            Instant::now(),
        )
        .is_err(),
        "非问答 PreToolUse 不能登记成待答"
    );
    (runtime, ticket, target)
}

/// 场景：Grok 的 ask_user_question 进入岛待答，Bash 不进入；写完 deny 后卡片结束。
///
/// 数据构造：
///   一张 Grok 卡片，session sess-spike，tool_use_id call-spike
///   选择第 0 个选项 Blue
///
/// 执行过程：
///   1. SessionStart 后接收 PreToolUse(ask_user_question)
///   2. 尝试把 Bash 登记成问题，必须失败
///   3. 岛内提交 Blue，写出 hook stdout，再结算 deny
///
/// 预期结果：
///   - 正断言：待答 provider 是 Grok，结算后为 Answered 且不再阻塞
///   - 负断言：结算前仍是 Submitted，不能把未写出的 hook 当成已答
#[test]
fn grok_pre_tool_use_registers_and_deny_write_answers() {
    let (mut runtime, ticket, target) = open(
        CanvasProfile::Grok,
        HookSource::Grok,
        "ask_user_question",
        false,
    );
    assert_eq!(target.provider, AgentProfile::Grok, "待答必须标成 Grok");
    assert_eq!(
        runtime.questions().len(),
        1,
        "只有问答工具进入 questions 快照"
    );
    assert!(
        runtime.has_pending_question(target.node_id),
        "登记后岛侧应看到待处理"
    );
    let answers = vec![QuestionAnswer {
        key: "question-0".into(),
        selected: vec!["option-0".into()],
        text: None,
    }];
    let submission = runtime
        .claim_question_answer(&target, answers, "delivery-grok")
        .unwrap()
        .unwrap();
    runtime.deliver_native_question(&submission).unwrap();
    let response = ticket.take_response(Instant::now()).unwrap();
    assert_eq!(response["decision"], "deny", "回给 Grok 的是 deny");
    assert!(
        !runtime.settle_delivered_hook_answers(),
        "stdout 还没写完时不能收卡"
    );
    assert_eq!(
        runtime.question(&target).unwrap().state,
        QuestionState::Submitted,
        "只取走 JSON 仍是已提交"
    );
    ticket
        .with_response_write(Instant::now(), || Ok(true))
        .unwrap();
    assert!(
        runtime.settle_delivered_hook_answers(),
        "deny 写完就是这题的结果，不再等 PostToolUse"
    );
    assert_eq!(
        runtime.question(&target).unwrap().state,
        QuestionState::Answered,
        "岛内作答应结束这题"
    );
    assert!(
        !runtime.has_pending_question(target.node_id),
        "答完后不再占着需要你处理"
    );
}

/// 场景：native_first 时 Grok 不挂起 hook。
///
/// 数据构造：同一 Grok 问答，native_first=true。
/// 执行：接收 PreToolUse。
/// 预期：hook 已 Cancelled，岛不能再代答。
#[test]
fn grok_native_first_releases_the_hook() {
    let (runtime, ticket, target) = open(
        CanvasProfile::Grok,
        HookSource::Grok,
        "ask_user_question",
        true,
    );
    assert_eq!(
        ticket.status(Instant::now()),
        norma::agent_hook::QuestionHookStatus::Cancelled,
        "交给卡片回答时不能 deny"
    );
    assert!(
        !runtime.native_question_available(&target),
        "hook 释放后岛不能再提交"
    );
}

/// 场景：终端里的 Grok UserAnswered 收起岛卡，别的 tool_use_id 不能收。
///
/// 数据构造：native_first 释放后的同一题；一条错误 id，一条带 UserAnswered.message 的 PostToolUse。
/// 执行：先投错误 id，再投本调用。
/// 预期：错误 id 保持待处理；本调用变成 AnsweredElsewhere 并不再阻塞。
#[test]
fn grok_terminal_user_answered_closes_the_card() {
    let (mut runtime, _ticket, target) = open(
        CanvasProfile::Grok,
        HookSource::Grok,
        "ask_user_question",
        true,
    );
    let mut wrong = parse_hook_event(
        HookSource::Grok,
        target.node_id,
        json!({
            "hook_event_name": "PostToolUse",
            "tool_name": "ask_user_question",
            "session_id": "sess-spike",
            "tool_use_id": "call-other",
            "tool_response": {"type":"AskUserQuestion","UserAnswered":{"message":"Blue"}}
        }),
    )
    .unwrap();
    wrong.control_run_id = Some(target.run_id.clone());
    assert!(
        !runtime.observe_native_question_result(&wrong),
        "别的调用不能收掉这张卡"
    );
    let mut post = parse_hook_event(
        HookSource::Grok,
        target.node_id,
        json!({
            "hook_event_name": "PostToolUse",
            "tool_name": "ask_user_question",
            "session_id": "sess-spike",
            "tool_use_id": "call-spike",
            "tool_response": {
                "type": "AskUserQuestion",
                "UserAnswered": {"message": "User declined to answer the questions"}
            }
        }),
    )
    .unwrap();
    post.control_run_id = Some(target.run_id.clone());
    assert!(
        runtime.observe_native_question_result(&post),
        "工具真正跑完的 UserAnswered 必须收卡"
    );
    assert_eq!(
        runtime.question(&target).unwrap().state,
        QuestionState::AnsweredElsewhere,
        "终端作答不能算岛代答成功"
    );
    assert!(
        !runtime.has_pending_question(target.node_id),
        "收卡后不再阻塞"
    );
}

/// 场景：ZCode 岛内作答仍要等带 answers 的 PostToolUse，不能在 hook 写出时就当答完。
///
/// 数据构造：AskUserQuestion，选择 Blue，PostToolUse 的 tool_response.answers 与之相同。
/// 执行：写出 allow 后先结算，再投递 PostToolUse。
/// 预期：结算不改变 Submitted；PostToolUse 后才是 Answered。
#[test]
fn zcode_waits_for_post_tool_use_answers() {
    let (mut runtime, ticket, target) = open(
        CanvasProfile::ZCode,
        HookSource::ZCode,
        "AskUserQuestion",
        false,
    );
    let answers = vec![QuestionAnswer {
        key: "question-0".into(),
        selected: vec!["option-0".into()],
        text: None,
    }];
    let submission = runtime
        .claim_question_answer(&target, answers, "delivery-zcode")
        .unwrap()
        .unwrap();
    runtime.deliver_native_question(&submission).unwrap();
    let response = ticket.take_response(Instant::now()).unwrap();
    assert_eq!(
        response["hookSpecificOutput"]["updatedInput"]["answers"]["Spike color?"], "Blue",
        "ZCode 需要完整 answers"
    );
    ticket
        .with_response_write(Instant::now(), || Ok(true))
        .unwrap();
    assert!(
        !runtime.settle_delivered_hook_answers(),
        "ZCode 的 allow 不是终态"
    );
    assert_eq!(
        runtime.question(&target).unwrap().state,
        QuestionState::Submitted,
        "PostToolUse 之前卡片还在"
    );
    let post_raw = json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "AskUserQuestion",
        "session_id": "sess-spike",
        "tool_use_id": "call-spike",
        "tool_input": pre("AskUserQuestion")["tool_input"],
        "tool_response": {
            "questions": pre("AskUserQuestion")["tool_input"]["questions"],
            "answers": {"Spike color?": "Blue"}
        }
    });
    let mut post = parse_hook_event(HookSource::ZCode, target.node_id, post_raw).unwrap();
    post.control_run_id = Some(target.run_id.clone());
    assert!(
        runtime.observe_native_question_result(&post),
        "带 answers 的 PostToolUse 确认岛内作答"
    );
    assert_eq!(
        runtime.question(&target).unwrap().state,
        QuestionState::Answered,
        "ZCode 消费证据到齐后才算答完"
    );
}

/// 场景：InfCode 用工具名 question 登记；Kimi 的对象形 error 能收口失败。
///
/// 数据构造：InfCode PreToolUse tool=question；Kimi PostToolUseFailure.error 是 {message}。
/// 执行：InfCode 登记后看 provider；另开 Kimi 并投递失败事件。
/// 预期：InfCode provider 正确且 Bash 未登记；Kimi 失败后 Cancelled 且不再阻塞。
#[test]
fn infcode_question_tool_registers_and_kimi_object_error_closes() {
    let (runtime, _ticket, target) = open(
        CanvasProfile::InfCode,
        HookSource::InfCode,
        "question",
        false,
    );
    assert_eq!(target.provider, AgentProfile::InfCode, "待答标成 InfCode");
    assert_eq!(runtime.questions().len(), 1, "question 工具登记一道");
    assert!(
        ClaudeQuestion::from_profile(AgentProfile::InfCode, &pre("AskUserQuestion")).is_err(),
        "AskUserQuestion 不能冒充 InfCode 的 question"
    );

    let (mut runtime, _ticket, target) = open(
        CanvasProfile::Kimi,
        HookSource::Kimi,
        "AskUserQuestion",
        true,
    );
    let mut failure = parse_hook_event(
        HookSource::Kimi,
        target.node_id,
        json!({
            "hook_event_name": "PostToolUseFailure",
            "tool_name": "AskUserQuestion",
            "session_id": "sess-spike",
            "tool_call_id": "call-spike",
            "tool_input": pre("AskUserQuestion")["tool_input"],
            "error": {"code":"internal","message":"AskUserQuestion is disabled while auto permission mode is active; decide and continue.","retryable":false}
        }),
    )
    .unwrap();
    failure.control_run_id = Some(target.run_id.clone());
    assert!(
        runtime.observe_native_question_result(&failure),
        "Kimi 的对象 error 也是失败收口"
    );
    assert_eq!(
        runtime.question(&target).unwrap().state,
        QuestionState::Cancelled,
        "工具失败后不再等岛内作答"
    );
    assert!(
        !runtime.has_pending_question(target.node_id),
        "失败收口后解除阻塞"
    );
}

/// 场景：岛上的 deny 已经写给 agent 之后，迟到的 Post 不能改成答在别处。
///
/// 数据构造：Grok 选择 Blue，stdout 已写完，状态仍是 Submitted。
/// 随后同一 tool_use_id call-spike 的 PostToolUse 带 UserAnswered。
/// 同一段结算也挡在 PostToolUseFailure 之前，所以失败事件同样不能改判。
///
/// 执行：写出 deny，不先调用 settle，再 observe 这条 Post。
///
/// 预期：状态是 Answered，证据是 Grok/hook-deny，不是 PostToolUse 收口。
#[test]
fn delivered_deny_survives_a_later_post_tool_use() {
    let (mut runtime, ticket, target) = open(
        CanvasProfile::Grok,
        HookSource::Grok,
        "ask_user_question",
        false,
    );
    let submission = runtime
        .claim_question_answer(
            &target,
            vec![QuestionAnswer {
                key: "question-0".into(),
                selected: vec!["option-0".into()],
                text: None,
            }],
            "delivery-grok-post",
        )
        .unwrap()
        .unwrap();
    runtime.deliver_native_question(&submission).unwrap();
    assert!(
        ticket.take_response(Instant::now()).is_some(),
        "CLI 取到 deny"
    );
    ticket
        .with_response_write(Instant::now(), || Ok(true))
        .unwrap();
    let mut post = parse_hook_event(
        HookSource::Grok,
        target.node_id,
        json!({
            "hook_event_name": "PostToolUse",
            "tool_name": "ask_user_question",
            "session_id": "sess-spike",
            "tool_use_id": "call-spike",
            "tool_response": {"UserAnswered": {"message": "Blue"}}
        }),
    )
    .unwrap();
    post.control_run_id = Some(target.run_id.clone());
    assert!(
        runtime.observe_native_question_result(&post),
        "写出 deny 后的 Post 仍应让界面刷新"
    );
    let grok = runtime.question(&target).unwrap();
    assert_eq!(
        grok.state,
        QuestionState::Answered,
        "已经交给 Grok 的 deny 不能被事后事件改成答在别处"
    );
    assert_ne!(
        grok.evidence.as_deref(),
        Some("Grok/PostToolUse:ask_user_question"),
        "不能把岛内 deny 记成终端 Post 收口"
    );
    assert_eq!(
        grok.evidence.as_deref(),
        Some("Grok/hook-deny"),
        "证据应是 hook 写出的 deny"
    );
    assert!(
        !runtime.has_pending_question(target.node_id),
        "答完后不再占着需要你处理"
    );
}
