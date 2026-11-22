use canvas::{AgentProfile, AgentSpec, CanvasNodeKind, CanvasRect};
use runtime::{AgentTaskState, AgentTaskUpdate, Runtime};

fn open_code_card() -> (Runtime, canvas::NodeId) {
    let mut runtime = Runtime::new();
    let node = runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::OpenCode,
            alias: None,
            definition_id: None,
            mobile_control: false,
        }),
        CanvasRect::new(0.0, 0.0, 640.0, 400.0),
    );
    (runtime, node)
}

fn update(node: canvas::NodeId, event: &str, to: AgentTaskState) -> AgentTaskUpdate {
    AgentTaskUpdate {
        node_id: node,
        profile: AgentProfile::OpenCode,
        provider_event: event.into(),
        to,
        task_title: None,
        session_id: Some("ses_card".into()),
    }
}

/// 场景：OpenCode 2 的问题表单打开后，后面的工具事件不能把卡片打回运行中。
/// 构造：一张没有问答宿主的 OpenCode 卡片，先进入思考，再收到 QuestionRequest。
/// 执行：表单仍打开时再投递 PreToolUse，然后 QuestionSettled，最后 Stop。
/// 预期：表单打开期间保持 WaitingQuestion；回答后回到 Thinking；结束后是 Done。
///   负断言：没有真实 question 条目时，单独的 Thinking 不得显示为等待回答。
#[test]
fn opencode_form_holds_waiting_question_until_settled() {
    let (mut runtime, node) = open_code_card();
    runtime.apply_agent_task_update(update(node, "UserPromptSubmit", AgentTaskState::Thinking));
    assert_eq!(
        runtime.agent_task_state(node),
        Some(AgentTaskState::Thinking),
        "尚未出题时卡片是运行中，不是等待回答"
    );

    runtime.apply_agent_task_update(update(node, "QuestionRequest", AgentTaskState::Thinking));
    runtime.apply_agent_task_update(update(
        node,
        "PreToolUse",
        AgentTaskState::RunningTool {
            tool_name: Some("Question".into()),
        },
    ));
    assert_eq!(
        runtime.agent_task_state(node),
        Some(AgentTaskState::WaitingQuestion),
        "表单未关闭时，后续工具事件仍须显示等待回答"
    );

    runtime.apply_agent_task_update(update(node, "QuestionSettled", AgentTaskState::Thinking));
    assert_eq!(
        runtime.agent_task_state(node),
        Some(AgentTaskState::Thinking),
        "表单回答后应回到运行中"
    );
    runtime.apply_agent_task_update(update(node, "Stop", AgentTaskState::Done));
    assert_eq!(
        runtime.agent_task_state(node),
        Some(AgentTaskState::Done),
        "回合结束后不得继续停在等待回答"
    );
}
