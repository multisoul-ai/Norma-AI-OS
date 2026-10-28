use norma::agent_hook::{
    AgentHookEvent, HookQuestion, HookQuestionRequest, NativeQuestionReply, NativeSendStatus,
};
use norma::protocol::{AgentProfile, QuestionAnswer};
use serde_json::{json, Value};
use std::time::Instant;

fn ticket() -> HookQuestion {
    let request: Value =
        serde_json::from_str(include_str!("fixtures/questions/opencode-request.json")).unwrap();
    HookQuestion::new(
        HookQuestionRequest {
            run_id: "owned-run".into(),
            event: AgentHookEvent {
                node_id: 7,
                control_run_id: Some("owned-run".into()),
                profile: AgentProfile::OpenCode,
                provider_event: "NormaQuestion".into(),
                session_id: request["sessionID"].as_str().map(str::to_owned),
                turn_id: None,
                tool_name: Some("question".into()),
                message: None,
                raw_json: json!({"connection_id":"opencode-native:owned","request":request}),
            },
        },
        Instant::now(),
    )
    .expect("拥有实例的OpenCode原题应可创建ticket")
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
            text: Some("NATIVE_OPENCODE_7F2".into()),
        },
    ]
}

/// 场景：OpenCode HTTP回答与Codex一样，返回规范化数据本身不能被当成原题已提交。
/// 构造：真实三题夹具、实例connection与进程run；宿主提交一次后取原ticket响应。
/// 执行：读取answer/delivery并尝试重复提交、撤销、迟到发送。
/// 预期：原Provider/原session保留；只取一次，撤销后不可发送，完整HTTP消费之前无Sent。
#[test]
fn opencode_ticket_carries_original_question_and_revokes_before_send() {
    let ticket = ticket();
    assert_eq!(
        ticket.adapter().provider(),
        AgentProfile::OpenCode,
        "原题Provider不得误归Claude/Codex"
    );
    assert!(
        ticket
            .submit_delivery(&answers(), "delivery", Instant::now())
            .unwrap(),
        "首次提交占有原问题"
    );
    let reply: NativeQuestionReply =
        serde_json::from_value(ticket.take_response(Instant::now()).unwrap()).unwrap();
    assert_eq!(reply.delivery_id, "delivery", "保留宿主delivery身份");
    assert_eq!(reply.answers, answers(), "保留规范化全部子题答案");
    assert!(!ticket.transport_sent(), "仅取数据没有原HTTP发送");
    assert_eq!(
        ticket.native_send_status(),
        NativeSendStatus::Unsent,
        "准备态仍可撤销"
    );
    assert!(
        ticket.take_response(Instant::now()).is_none(),
        "同ticket不能取两次"
    );
    assert!(ticket.try_cancel(), "原HTTP尚未开始可交还原卡片");
    assert!(
        !ticket
            .submit_delivery(&answers(), "again", Instant::now())
            .unwrap(),
        "取消后不能再提交"
    );
}

/// 场景：HTTP可阻塞，但发送占有与人工取消必须立即互斥，不能持有前台会争用的锁做网络IO。
/// 构造：OpenCode题目ticket与本次delivery；分别模拟取消先发生、HTTP接纳、发送后结果未知。
/// 执行：取得一次性发送guard，发送期间尝试取消/重复发送，随后接纳或丢弃guard。
/// 预期：取消赢时无guard；提交赢时取消立即返回false，200接纳才Sent，丢弃为Unknown且不可重发。
#[test]
fn http_send_lease_is_once_only_without_holding_ticket_lock() {
    for outcome in ["cancel", "accepted", "unknown", "rejected"] {
        let ticket = ticket();
        ticket
            .submit_delivery(&answers(), "delivery", Instant::now())
            .unwrap();
        ticket.take_response(Instant::now()).unwrap();
        if outcome == "cancel" {
            assert!(ticket.try_cancel(), "发送前取消必须生效");
        }
        let sending = ticket.begin_http_delivery("delivery", Instant::now());
        if outcome == "cancel" {
            assert!(sending.is_err(), "已取消不能取得发送许可");
            continue;
        }
        let sending = sending.expect("当前唯一delivery可以占有发送");
        assert!(!ticket.try_cancel(), "HTTP发送已开始，不能声称成功取消");
        assert!(
            ticket
                .begin_http_delivery("delivery", Instant::now())
                .is_err(),
            "同delivery不能第二次发HTTP"
        );
        assert_eq!(
            ticket.native_send_status(),
            NativeSendStatus::Writing,
            "持guard可立即读取状态而不等待HTTP"
        );
        match outcome {
            "accepted" => sending.accepted(),
            "rejected" => sending.rejected(),
            _ => drop(sending),
        }
        assert_eq!(
            ticket.transport_sent(),
            outcome == "accepted",
            "只有明确接纳才算Submitted"
        );
        assert_eq!(
            ticket.native_send_status(),
            match outcome {
                "accepted" => NativeSendStatus::Sent,
                "rejected" => NativeSendStatus::Unsent,
                _ => NativeSendStatus::Unknown,
            },
            "保留真实发送结局"
        );
        assert!(
            ticket
                .begin_http_delivery("delivery", Instant::now())
                .is_err(),
            "结束后的guard不能重新获取"
        );
    }
}
