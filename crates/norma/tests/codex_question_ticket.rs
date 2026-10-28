use norma::agent_hook::{AgentHookEvent, HookQuestion, HookQuestionRequest, QuestionHookStatus};
use norma::protocol::{AgentProfile, QuestionAnswer, QuestionRef, QuestionRequestId};
use serde_json::{json, Value};
use std::time::Instant;

fn request() -> HookQuestionRequest {
    let request: Value =
        serde_json::from_str(include_str!("fixtures/questions/codex-request.json")).unwrap();
    let source: Value =
        serde_json::from_str(include_str!("fixtures/questions/codex-source.json")).unwrap();
    HookQuestionRequest {
        run_id: "owned-process".into(),
        event: AgentHookEvent {
            control_run_id: Some("owned-process".into()),
            node_id: 7,
            profile: AgentProfile::Codex,
            provider_event: "NormaQuestion".into(),
            session_id: Some("codex-question-test-thread".into()),
            turn_id: Some("codex-question-test-turn".into()),
            tool_name: Some("request_user_input".into()),
            message: None,
            raw_json: json!({"connection_id":"codex-relay:private-connection:1","request":request,"source":source}),
        },
    }
}

/// 场景：Codex原题通过现有可撤销等待桥交给宿主，不能混入审批或伪造另一轮身份。
/// 构造：真实RPC/source封装在受控run的事件，原RPC ID为数值0，Green为原第二选项。
/// 执行：创建ticket并生成共享题面，提交Green、取回原RPC响应；变更来源/turn后重建。
/// 预期：原身份与答案保留，一次输出且仅到Responding；来源不明或身份错配全部拒绝。
#[test]
fn codex_native_question_uses_the_revocable_host_ticket_with_exact_identity() {
    let now = Instant::now();
    let ticket = HookQuestion::new(request(), now).unwrap();
    let target = QuestionRef {
        workspace_id: "w".into(),
        project_id: "p".into(),
        node_id: 7,
        provider: AgentProfile::Codex,
        run_id: "run".into(),
        connection_id: "codex-relay:private-connection:1".into(),
        session_id: "codex-question-test-thread".into(),
        turn_id: Some("codex-question-test-turn".into()),
        request_id: QuestionRequestId::Number(0),
        revision: 1,
    };
    let question = ticket.adapter().question(target).unwrap();
    assert_eq!(question.items.len(), 1, "宿主持有原题完整题面");
    let answers = vec![QuestionAnswer {
        key: "question-0".into(),
        selected: vec!["option-1".into()],
        text: None,
    }];
    assert!(
        ticket.submit_delivery(&answers, "delivery", now).unwrap(),
        "采用完整答案后可回原等待桥"
    );
    assert_eq!(
        ticket.take_response(now).unwrap(),
        json!({"delivery_id":"delivery","answers":answers}),
        "桥只返回规范化答案，由relay再次核验并回原RPC"
    );
    assert_eq!(
        ticket.status(now),
        QuestionHookStatus::Responding,
        "取走响应仍不能宣称已消费"
    );
    assert!(
        ticket.take_response(now).is_none(),
        "一次ticket最多返回一条答案"
    );
    let mut invalid = request();
    invalid.event.turn_id = Some("wrong-turn".into());
    assert!(
        HookQuestion::new(invalid, now).is_err(),
        "信封轮次不能与原RPC不一致"
    );
    let mut invalid = request();
    invalid.event.raw_json["source"]["params"]["item"]["name"] =
        "mcp__connector__delete_document".into();
    assert!(
        HookQuestion::new(invalid, now).is_err(),
        "connector审批不得进入答题ticket"
    );
}
