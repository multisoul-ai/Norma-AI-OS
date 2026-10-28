use canvas::{AgentProfile, CanvasRect};
use norma::agent_hook::{
    parse_hook_event, AgentHookEvent, HookQuestion, HookQuestionRequest, HookSource,
};
use norma::protocol::{QuestionAnswer, QuestionRef, QuestionRequestId};
use runtime::{AgentHookRuntimeConfig, AgentTaskState, AgentTaskUpdate, Runtime};
use std::time::Instant;

fn fixture() -> (Runtime, AgentHookEvent, QuestionRef) {
    let mut runtime = Runtime::new_for_workspace("/tmp/fallback-question-test");
    runtime.set_agent_hook_config(Some(AgentHookRuntimeConfig {
        endpoint: "http://127.0.0.1:49152/agent-hooks".into(),
        token: "test".into(),
        workspace_id: "w".into(),
        workspace_route_id: "a".into(),
        norma_socket: "/tmp/question-test.sock".into(),
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
    let run = runtime.mobile_agent_run_id(node).unwrap().to_owned();
    let raw = serde_json::from_str(include_str!(
        "../../norma/tests/fixtures/questions/claude-pre-tool-use.json"
    ))
    .unwrap();
    let mut pre = parse_hook_event(HookSource::Claude, node, raw).unwrap();
    pre.control_run_id = Some(run.clone());
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id: node,
        profile: AgentProfile::Claude,
        provider_event: "SessionStart".into(),
        to: AgentTaskState::Thinking,
        task_title: None,
        session_id: pre.session_id.clone(),
    });
    let target = QuestionRef {
        workspace_id: "w".into(),
        project_id: "p".into(),
        node_id: node,
        provider: norma::protocol::AgentProfile::Claude,
        run_id: run.clone(),
        connection_id: format!("claude-hook:{run}"),
        session_id: pre.session_id.clone().unwrap(),
        turn_id: None,
        request_id: QuestionRequestId::String(pre.raw_json["tool_use_id"].as_str().unwrap().into()),
        revision: 1,
    };
    (runtime, pre, target)
}

/// 场景：未决问题容量达到256条后，新原生问答回退人工仍必须阻塞普通任务。
/// 构造：同卡片当前run内256个未决问题，再到达一个新的Claude Hook。
/// 执行：登记失败后完成其余256题、尝试派任务、人工键入，再投递对应的工具终态。
/// 预期：不提供代答但仍阻止任务覆盖；人工可操作，精确终态后解除阻塞。
#[test]
fn question_capacity_fallback_still_protects_native_prompt() {
    let (mut runtime, pre, target) = fixture();
    runtime.bind_question_connection(&target).unwrap();
    let adapter = norma::agent_hook::ClaudeQuestion::from_hook(&pre.raw_json).unwrap();
    let items = adapter.question(target.clone()).unwrap().items;
    let answers = vec![QuestionAnswer {
        key: "question-0".into(),
        selected: vec!["option-1".into()],
        text: None,
    }];
    for index in 0..256 {
        let mut old = target.clone();
        old.request_id = QuestionRequestId::String(format!("old-{index}"));
        runtime
            .register_question(norma::protocol::PendingQuestion {
                target: old.clone(),
                items: items.clone(),
            })
            .unwrap();
    }
    let ticket = HookQuestion::new(
        HookQuestionRequest {
            run_id: target.run_id.clone(),
            event: pre.clone(),
        },
        Instant::now(),
    )
    .unwrap();
    assert!(
        runtime.accept_native_question(ticket.clone(), "p").is_err(),
        "达到容量时不接管新问题"
    );
    // Resolve the 256 admitted questions so the rejected native prompt is the only blocker.
    for index in 0..256 {
        let mut old = target.clone();
        old.request_id = QuestionRequestId::String(format!("old-{index}"));
        let submission = runtime
            .claim_question_answer(&old, answers.clone(), &format!("delivery-{index}"))
            .unwrap()
            .unwrap();
        runtime.start_question_delivery(&submission).unwrap();
        runtime
            .confirm_question_answer(&submission, &answers, "test completed question")
            .unwrap();
        runtime.acknowledge_question_receipt(&submission);
    }
    assert!(
        runtime.has_pending_question(target.node_id),
        "容量拒绝不能丢失原生问答阻塞"
    );
    assert!(
        runtime
            .send_agent_task_text(target.node_id, "new task")
            .is_err(),
        "新任务不能覆盖原题"
    );
    assert!(
        runtime
            .send_node_input(target.node_id, runtime::NodeInput::Text("Green".into()))
            .is_ok(),
        "仍允许人工在原卡片回答"
    );
    assert!(
        ticket.take_response(Instant::now()).is_none(),
        "拒绝接管不应产生回答"
    );
    let mut post = pre.clone();
    post.provider_event = "PostToolUseFailure".into();
    post.raw_json["hook_event_name"] = "PostToolUseFailure".into();
    post.raw_json["error"] = "cancelled by user".into();
    assert!(
        runtime.observe_native_question_result(&post),
        "同一原题的工具终态应解除回退阻塞"
    );
    assert!(
        !runtime.has_pending_question(target.node_id),
        "原题结束后不残留阻塞"
    );
}

/// 场景：原生问题合法但选项标签重复，结构化代答无法无歧义映射。
/// 构造：同一当前session的AskUserQuestion，把第二项label改为与第一项相同。
/// 执行：经普通Hook通知宿主，投递后到Thinking和不相干工具回执，再结束原工具。
/// 预期：不支持代答仍保持WaitingQuestion并拒绝新任务；只有同题终态可解除阻塞。
#[test]
fn unsupported_question_shape_blocks_tasks_until_correlated_native_result() {
    let (mut runtime, mut pre, target) = fixture();
    pre.raw_json["tool_input"]["questions"][0]["options"][1]["label"] = "Blue".into();
    assert!(
        norma::agent_hook::ClaudeQuestion::from_hook(&pre.raw_json).is_err(),
        "夹具确实不能无歧义地自动映射"
    );
    assert!(runtime.observe_mobile_hook(&pre), "同进程原题仍应通知宿主");
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id: target.node_id,
        profile: AgentProfile::Claude,
        provider_event: "PreToolUse".into(),
        to: AgentTaskState::Thinking,
        task_title: None,
        session_id: pre.session_id.clone(),
    });
    assert_eq!(
        runtime.agent_task_state(target.node_id),
        Some(AgentTaskState::WaitingQuestion),
        "后到状态不能覆盖不支持代答的原题"
    );
    assert!(
        runtime
            .send_agent_task_text(target.node_id, "new task")
            .is_err(),
        "无法解析不代表可以写入普通任务"
    );
    let mut post = pre.clone();
    post.provider_event = "PostToolUseFailure".into();
    post.raw_json["hook_event_name"] = "PostToolUseFailure".into();
    post.raw_json["error"] = "user cancelled".into();
    post.raw_json["tool_use_id"] = "another-tool".into();
    assert!(
        !runtime.observe_native_question_result(&post),
        "另一个工具的终态不能解除阻塞"
    );
    assert!(
        runtime.has_pending_question(target.node_id),
        "错误工具回执后仍受保护"
    );
    post.raw_json["tool_use_id"] = pre.raw_json["tool_use_id"].clone();
    assert!(
        runtime.observe_native_question_result(&post),
        "同一原题结束后可以解除阻塞"
    );
    assert!(
        !runtime.has_pending_question(target.node_id),
        "未留下永久待答"
    );
}
