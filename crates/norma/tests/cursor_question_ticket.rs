use norma::agent_hook::{AgentHookEvent, HookQuestion, HookQuestionRequest, NativeSendStatus};
use norma::protocol::{AgentProfile, QuestionAnswer};
use serde_json::{json, Value};
use std::time::Instant;
fn ticket() -> HookQuestion {
    let call: Value =
        serde_json::from_str(include_str!("fixtures/questions/cursor-tool-call.json")).unwrap();
    HookQuestion::new(
        HookQuestionRequest {
            run_id: "process".into(),
            event: AgentHookEvent {
                node_id: 7,
                profile: AgentProfile::Cursor,
                control_run_id: Some("process".into()),
                provider_event: "NormaQuestion".into(),
                session_id: Some("session".into()),
                turn_id: None,
                tool_name: Some("AskQuestion".into()),
                message: None,
                raw_json: json!({"connection_id":"cursor-native:owned","source":call}),
            },
        },
        Instant::now(),
    )
    .unwrap()
}
fn answers() -> Vec<QuestionAnswer> {
    vec![
        QuestionAnswer {
            key: "question-0".into(),
            selected: vec!["option-1".into()],
            text: None,
        },
        QuestionAnswer {
            key: "question-1".into(),
            selected: vec!["option-0".into(), "option-2".into()],
            text: None,
        },
        QuestionAnswer {
            key: "question-2".into(),
            selected: vec![],
            text: Some("NATIVE_ONLY_7F2".into()),
        },
    ]
}

/// 场景：Cursor代答在原PTY逐步写入，首键前可撤销，部分输入后人工接管不能再补发。
/// 构造：同原始三题的两个ticket，先撤销一个，另一个写一键后撤销。
/// 执行：提交、取响应、逐步写入并尝试迟到写入。
/// 预期：未写ticket无字节；部分写ticket为Unknown并拒绝后续键，绝不伪装Sent。
#[test]
fn cursor_ticket_revokes_unsent_and_partially_written_terminal_answers() {
    let now = Instant::now();
    let before = ticket();
    before.submit_delivery(&answers(), "d", now).unwrap();
    before.take_response(now).unwrap();
    before.stop_terminal_send();
    assert!(
        before
            .with_native_send("d", now, || panic!("撤销后不能写入"))
            .is_err(),
        "首键前取消拒绝写入"
    );
    assert_eq!(
        before.native_send_status(),
        NativeSendStatus::Unsent,
        "首键前保持未发送"
    );
    let partial = ticket();
    partial.submit_delivery(&answers(), "d", now).unwrap();
    partial.take_response(now).unwrap();
    assert!(
        !partial.with_native_send("d", now, || Ok(false)).unwrap(),
        "只写一键不能提交完成"
    );
    assert_eq!(
        partial.native_send_status(),
        NativeSendStatus::Writing,
        "部分终端操作仍在写入"
    );
    partial.stop_terminal_send();
    assert_eq!(
        partial.native_send_status(),
        NativeSendStatus::Unknown,
        "部分输入后接管结果待核对"
    );
    assert!(
        partial
            .with_native_send("d", now, || panic!("接管后不能继续"))
            .is_err(),
        "禁止半串重放"
    );
    assert!(!partial.transport_sent(), "没有最终提交不能声称已发送");
}
