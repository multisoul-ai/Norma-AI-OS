use super::*;
use crate::{AgentTaskState, AgentTaskUpdate};

fn add_claude(runtime: &mut Runtime) -> NodeId {
    runtime.document.add_node(
        canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
            profile: canvas::AgentProfile::Claude,
            alias: None,
            definition_id: None,
            mobile_control: false,
        }),
        canvas::CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    )
}

fn update(node_id: NodeId, to: AgentTaskState, event: &str, text: Option<&str>) -> AgentTaskUpdate {
    AgentTaskUpdate {
        node_id,
        profile: canvas::AgentProfile::Claude,
        provider_event: event.to_string(),
        to,
        task_title: text.map(str::to_string),
        session_id: None,
    }
}

/// CLI 自己的会话名优先于首条提示词；取不到时回退，新会话不沿用旧会话名。
///
/// 数据构造：Claude 节点提交首条提示词“帮我重构”，随后写入会话名“重构权限闸门”。
/// 预期结果：
///   - 正断言：写入前标题是首条提示词；写入后标题是会话名，最新输入仍是提示词
///   - 正断言：重复写入同一标题返回 false，改名返回 true 且标题更新
///   - 负断言：SessionStart（Ready）后会话名被清空，标题回到无；未写入会话名的节点不受影响
#[test]
fn session_title_overrides_first_prompt_and_resets_with_session() {
    let mut runtime = Runtime::new();
    let node = add_claude(&mut runtime);
    let other = add_claude(&mut runtime);
    runtime.apply_agent_task_update(update(
        node,
        AgentTaskState::Thinking,
        "UserPromptSubmit",
        Some("帮我重构"),
    ));
    assert_eq!(runtime.agent_task_title(node).as_deref(), Some("帮我重构"));

    assert!(runtime.set_agent_session_title(node, "重构权限闸门".into()));
    assert_eq!(
        runtime.agent_task_title(node).as_deref(),
        Some("重构权限闸门")
    );
    assert_eq!(
        runtime.agent_latest_prompt(node).as_deref(),
        Some("帮我重构")
    );
    assert!(!runtime.set_agent_session_title(node, "重构权限闸门".into()));
    assert!(runtime.set_agent_session_title(node, "加固权限闸门".into()));
    assert_eq!(
        runtime.agent_task_title(node).as_deref(),
        Some("加固权限闸门")
    );
    assert_eq!(runtime.agent_task_title(other), None);

    runtime.apply_agent_task_update(update(node, AgentTaskState::Ready, "SessionStart", None));
    assert_eq!(runtime.agent_task_title(node), None);
}
