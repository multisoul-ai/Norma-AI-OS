use brain::{BrainEvent, Lane, Router, RouterConfig, TriggerKind};
use norma::protocol::{
    AgentProfile, PendingQuestion, QuestionItem, QuestionOption, QuestionRef, QuestionRequestId,
};
use std::time::Instant;

fn question() -> PendingQuestion {
    PendingQuestion {
        target: QuestionRef {
            workspace_id: "w".into(),
            project_id: "p".into(),
            node_id: 1,
            provider: AgentProfile::Claude,
            run_id: "run".into(),
            connection_id: "hook".into(),
            session_id: "session".into(),
            turn_id: None,
            request_id: QuestionRequestId::String("tool'$(echo injected)".into()),
            revision: 1,
        },
        items: vec![QuestionItem {
            key: "q".into(),
            provider_id: None,
            prompt: "Which color?".into(),
            options: vec![QuestionOption {
                key: "green".into(),
                label: "Green".into(),
                description: None,
            }],
            multiple: false,
            allow_custom: true,
            secret: false,
        }],
    }
}

/// 场景：问答事件唤醒幕僚建议，只有宿主明确委托事件才要求直接回答原题。
/// 构造：带特殊字符的原请求ID；普通与明确委托两种事件。
/// 执行：经过生产Router分流，不使用模型也不调用外部命令。
/// 预期：都去主会话而非审批；普通模式只建议，委托模式允许答题，目标以完整JSON保留。
#[test]
fn question_event_distinguishes_suggestion_from_one_question_delegation() {
    let now = Instant::now();
    let mut router = Router::new(RouterConfig::default(), now);
    for delegated in [false, true] {
        let question = question();
        let actions = router.push(
            BrainEvent::QuestionRequested {
                agent: "Claude".into(),
                question: question.clone(),
                delegated,
            },
            now,
        );
        assert_eq!(actions.len(), 1, "一题只唤醒一次");
        let action = &actions[0];
        assert_eq!(action.lane, Lane::Main, "问答不是权限审批");
        assert!(action.approval_id.is_none(), "问答不占用审批身份");
        assert_eq!(
            action.trigger.kind,
            TriggerKind::QuestionRequest,
            "活动来源明确记录为问答"
        );
        assert!(
            action
                .text
                .contains(&serde_json::to_string(&question).unwrap()),
            "完整原题身份不丢失或拼成shell命令"
        );
        assert!(
            action.text.contains(if delegated {
                "brain answer --target"
            } else {
                "brain propose-answer --target"
            }),
            "按真实用户授权选择动作"
        );
        assert!(
            action.text.contains("不能用 assign 或 propose 派发新任务"),
            "指令说明原题与下一步边界"
        );
    }
}

/// 场景：异常题面不能给幕僚生成可执行答题请求。
/// 构造：完整原题后删除所有题目，破坏协议完整性。
/// 执行：投递为明确委托事件。
/// 预期：拒绝事件，不触发主会话或审批会话。
#[test]
fn invalid_question_does_not_wake_a_lane() {
    let now = Instant::now();
    let mut router = Router::new(RouterConfig::default(), now);
    let mut question = question();
    question.items.clear();
    assert!(
        router
            .push(
                BrainEvent::QuestionRequested {
                    agent: "Claude".into(),
                    question,
                    delegated: true
                },
                now
            )
            .is_empty(),
        "坏题即使带委托标记也不得继续"
    );
}
