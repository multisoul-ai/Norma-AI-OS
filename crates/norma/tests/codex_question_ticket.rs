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

/// 场景：Codex 原 RPC 仍在等待时，用户阅读超过 Hook 的两分钟期限后仍可回答。
/// 构造：真实 RPC/source 票据，将可控时间推进 121 秒，超过 Claude Hook 的 120 秒。
/// 执行：此时首次提交并发送；另一个同龄票据先取消再尝试提交。
/// 预期：活 RPC 可发送且不能重发，明确取消的 RPC 仍拒绝迟到答案。
#[test]
fn live_codex_question_outlives_hook_timeout_but_remains_revocable() {
    let start = Instant::now();
    let later = start + std::time::Duration::from_secs(121);
    let answers = vec![QuestionAnswer {
        key: "question-0".into(),
        selected: vec!["option-1".into()],
        text: None,
    }];
    let ticket = HookQuestion::new(request(), start).unwrap();
    assert!(
        ticket.submit_delivery(&answers, "late", later).unwrap(),
        "活 RPC 不受 Hook 等待期限限制"
    );
    assert!(
        ticket.take_response(later).is_some(),
        "当前授权可以取出答案"
    );
    assert!(
        ticket.with_native_send("late", later, || Ok(true)).unwrap(),
        "原 RPC 可以完整发送"
    );
    assert!(
        ticket.with_native_send("late", later, || Ok(true)).is_err(),
        "延长活问题寿命不允许重发"
    );
    let cancelled = HookQuestion::new(request(), start).unwrap();
    cancelled.cancel();
    assert!(
        !cancelled
            .submit_delivery(&answers, "cancelled", later)
            .unwrap(),
        "取消仍然永久撤销该票据"
    );
}

/// 场景：Codex撤权应阻止首帧，但已开始的一帧必须可flush，绝不允许第二帧重放。
/// 构造：原RPC票据与可切换gate，分别在首字节前和部分写出后撤权。
/// 执行：尝试发送，后者继续flush，再次重试同delivery。
/// 预期：前者零写出且明确拒绝，后者完成同一帧且不重新取得发送资格。
#[test]
fn codex_final_gate_revokes_first_write_but_keeps_owned_flush() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    for started in [false, true] {
        let now = Instant::now();
        let ticket = HookQuestion::new(request(), now).unwrap();
        let revoked = Arc::new(AtomicBool::new(false));
        let gate = revoked.clone();
        ticket.set_send_guard(Arc::new(move |send| {
            if gate.load(Ordering::Acquire) {
                return Err("revoked");
            }
            send();
            Ok(())
        }));
        ticket
            .submit_delivery(
                &[QuestionAnswer {
                    key: "question-0".into(),
                    selected: vec!["option-1".into()],
                    text: None,
                }],
                "once",
                now,
            )
            .unwrap();
        ticket.take_response(now).unwrap();
        if started {
            assert!(
                !ticket.with_native_send("once", now, || Ok(false)).unwrap(),
                "已开始同一帧"
            );
        }
        revoked.store(true, Ordering::Release);
        let mut writes = 0;
        let result = ticket.with_native_send("once", now, || {
            writes += 1;
            Ok(true)
        });
        assert_eq!(result.is_ok(), started, "只允许已开始帧flush");
        assert_eq!(writes, usize::from(started), "首次拒绝零写出，flush仅一次");
        assert_eq!(ticket.transport_sent(), started, "状态对应实际传输");
        assert!(
            ticket.with_native_send("once", now, || Ok(true)).is_err(),
            "同delivery不能重放"
        );
    }
}
