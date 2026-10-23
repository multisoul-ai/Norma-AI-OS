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

fn prompt(node_id: NodeId, to: AgentTaskState, event: &str, text: Option<&str>) -> AgentTaskUpdate {
    AgentTaskUpdate {
        node_id,
        profile: canvas::AgentProfile::Claude,
        provider_event: event.to_string(),
        to,
        task_title: text.map(str::to_string),
        session_id: None,
    }
}

/// 运行概览需要“会话名称 + 最新一条输入”：标题锁定首条，最新输入每轮刷新。
///
/// 数据构造：
///   同一个 Claude 节点连续提交两条 UserPromptSubmit，文本分别为“帮我重构”“现在再做别的”；
///   中间夹一个不带文本的 PostToolUse，用来确认非提示词事件不会清空最新输入。
///
/// 执行过程：
///   1. 提交第一条提示词，读取标题与最新输入。
///   2. 提交 PostToolUse（无文本）。
///   3. 提交第二条提示词，再读取。
///   4. 触发 SessionStart（Ready），读取。
///
/// 预期结果：
///   - 正断言：第一条后标题与最新输入均为“帮我重构”。
///   - 正断言：PostToolUse 后最新输入仍为“帮我重构”。
///   - 正断言：第二条后标题仍为“帮我重构”，最新输入变为“现在再做别的”。
///   - 负断言：SessionStart 后标题与最新输入都被清空，避免新会话沿用旧文案。
#[test]
fn test_latest_prompt_refreshes_while_title_stays_locked() {
    let mut runtime = Runtime::new();
    let node_id = add_claude(&mut runtime);

    runtime.apply_agent_task_update(prompt(
        node_id,
        AgentTaskState::Thinking,
        "UserPromptSubmit",
        Some("帮我重构"),
    ));
    assert_eq!(runtime.agent_task_title(node_id).as_deref(), Some("帮我重构"));
    assert_eq!(
        runtime.agent_latest_prompt(node_id).as_deref(),
        Some("帮我重构"),
        "首条提示词应同时成为最新输入"
    );

    runtime.apply_agent_task_update(prompt(
        node_id,
        AgentTaskState::RunningTool {
            tool_name: Some("Bash".into()),
        },
        "PostToolUse",
        None,
    ));
    assert_eq!(
        runtime.agent_latest_prompt(node_id).as_deref(),
        Some("帮我重构"),
        "非提示词事件不得清空最新输入"
    );

    runtime.apply_agent_task_update(prompt(
        node_id,
        AgentTaskState::Thinking,
        "UserPromptSubmit",
        Some("现在再做别的"),
    ));
    assert_eq!(
        runtime.agent_task_title(node_id).as_deref(),
        Some("帮我重构"),
        "标题应保持首条提示词"
    );
    assert_eq!(
        runtime.agent_latest_prompt(node_id).as_deref(),
        Some("现在再做别的"),
        "最新输入应刷新为第二条提示词"
    );

    runtime.apply_agent_task_update(prompt(node_id, AgentTaskState::Ready, "SessionStart", None));
    assert_eq!(runtime.agent_task_title(node_id), None, "新会话应清空标题");
    assert_eq!(
        runtime.agent_latest_prompt(node_id),
        None,
        "新会话不得沿用上一会话的最新输入"
    );
}
