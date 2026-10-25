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

/// 场景：智能体运行中途，卡片只能靠 hook 状态知道正在做什么。
/// 预期：思考中、调用具名工具、未知工具各有文案；空闲、完成状态没有实时动作。
#[test]
fn live_label_describes_running_states_only() {
    let tool = |name: Option<&str>| AgentTaskState::RunningTool {
        tool_name: name.map(str::to_string),
    };
    assert_eq!(
        AgentTaskState::Thinking.live_label().as_deref(),
        Some("思考中")
    );
    assert_eq!(
        tool(Some("Bash")).live_label().as_deref(),
        Some("调用 Bash")
    );
    assert_eq!(tool(Some("")).live_label().as_deref(), Some("执行工具"));
    assert_eq!(tool(None).live_label().as_deref(), Some("执行工具"));
    assert_eq!(AgentTaskState::Done.live_label(), None);
    assert_eq!(AgentTaskState::Ready.live_label(), None);
}

/// 场景：打开项目后恢复出已绑定会话的智能体，此时没有任何 hook 事件。
/// 数据构造：节点 a 绑定会话 s-a，节点 b 绑定 s-b 并已有会话名。
/// 预期：只返回 a 一次；再次取为空；SessionStart 清掉会话名后同一会话可以重新读取。
#[test]
fn restored_sessions_without_title_are_probed_once() {
    let mut runtime = Runtime::new();
    let (a, b) = (add_claude(&mut runtime), add_claude(&mut runtime));
    assert!(runtime.bind_discovered_agent_session(a, "s-a"));
    assert!(runtime.bind_discovered_agent_session(b, "s-b"));
    runtime.set_agent_session_title(b, "已有标题".into());
    let first = runtime.take_unprobed_session_titles();
    assert_eq!(first.len(), 1);
    assert_eq!((first[0].0, first[0].2.as_str()), (a, "s-a"));
    assert!(
        runtime.take_unprobed_session_titles().is_empty(),
        "同一会话只读一次"
    );
}
