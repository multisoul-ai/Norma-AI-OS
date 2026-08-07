use super::*;
use crate::motion::breathing::{BreathingIntent, BreathingState};
use runtime::{AgentTaskState, CardAttentionKind, CardAttentionState};

// Helper: maps state+attention to BreathingState via agent_breathing_intent.
fn agent_breathing_state(
    state: Option<AgentTaskState>,
    attention: Option<CardAttentionState>,
) -> BreathingState {
    agent_breathing_intent(state, attention).breathing_state()
}

/// Running 状态（Thinking/RunningTool）使用 Chase 动画，呼吸关闭；
/// WaitingApproval 保持暖色呼吸。
///
/// 数据构造：
///   Thinking / RunningTool → Chase 负责动画，breathing intent 为 Inactive
///   WaitingApproval → Warm breathing（节奏急促）
///
/// 执行过程：
///   1. 分别构造 Thinking、RunningTool、WaitingApproval
///   2. 调用 agent_breathing_state / agent_chase_intent 映射
///
/// 预期结果：
///   - Thinking/RunningTool breathing_state 为 Inactive（Chase 接管）
///   - Thinking/RunningTool chase_intent 为 true
///   - WaitingApproval breathing_state 为 Warm，chase_intent 为 false
#[test]
fn test_agent_breathing_state_maps_active_states_to_color_groups() {
    assert_eq!(
        agent_breathing_state(Some(AgentTaskState::Thinking), None),
        BreathingState::Inactive,
        "Thinking 由 Chase 动画驱动，breathing 应为 Inactive"
    );
    assert_eq!(
        agent_breathing_state(Some(AgentTaskState::RunningTool { tool_name: None }), None),
        BreathingState::Inactive,
        "RunningTool 由 Chase 动画驱动，breathing 应为 Inactive"
    );
    assert!(
        agent_chase_intent(Some(AgentTaskState::Thinking)),
        "Thinking 应激活 Chase 动画"
    );
    assert!(
        agent_chase_intent(Some(AgentTaskState::RunningTool { tool_name: None })),
        "RunningTool 应激活 Chase 动画"
    );
    assert_eq!(
        agent_breathing_state(
            Some(AgentTaskState::WaitingApproval { tool_name: None }),
            None
        ),
        BreathingState::Warm,
        "WaitingApproval 应映射为暖色呼吸"
    );
    assert!(
        !agent_chase_intent(Some(AgentTaskState::WaitingApproval { tool_name: None })),
        "WaitingApproval 不应激活 Chase"
    );
}

/// Done 只有在存在未查看完成提醒时使用 Cool 呼吸；已查看或缺失 attention 时静止。
/// Ready/Failed/Ended/None 保持 Inactive。
///
/// 数据构造（含关键数值推导）：
///   Done + AgentTaskDone(viewed=false) → Cool 呼吸，提示用户查看结果
///   Done + AgentTaskDone(viewed=true)  → Inactive，表示用户已查看
///   Done + None                        → Inactive，避免缺失 attention 时误闪
///
/// 执行过程：
///   1. 分别构造 Ready、Done(unviewed/viewed/none)、Failed、Ended、None
///   2. 调用 agent_breathing_state 映射
///
/// 预期结果：
///   - Done 未查看映射为 Cool 呼吸
///   - Done 已查看和缺失 attention 映射为 Inactive
///   - Done 不应激活 Chase
#[test]
fn test_agent_breathing_state_keeps_terminal_states_static() {
    let unviewed_done = Some(CardAttentionState {
        kind: CardAttentionKind::AgentTaskDone,
        viewed: false,
    });
    let viewed_done = Some(CardAttentionState {
        kind: CardAttentionKind::AgentTaskDone,
        viewed: true,
    });

    assert_eq!(
        agent_breathing_state(Some(AgentTaskState::Ready), None),
        BreathingState::Inactive,
        "Ready 不应呼吸"
    );
    assert_eq!(
        agent_breathing_state(Some(AgentTaskState::Done), unviewed_done),
        BreathingState::Cool,
        "未查看 Done attention 应保持 Cool 呼吸，提示用户查看结果"
    );
    assert_eq!(
        agent_breathing_state(Some(AgentTaskState::Done), viewed_done),
        BreathingState::Inactive,
        "已查看 Done attention 应关闭呼吸灯"
    );
    assert_eq!(
        agent_breathing_state(Some(AgentTaskState::Done), None),
        BreathingState::Inactive,
        "缺失 attention 的 Done 不应呼吸，避免恢复/异常状态误提示"
    );
    assert_eq!(
        agent_breathing_state(
            Some(AgentTaskState::Failed {
                message: "boom".to_string()
            }),
            None
        ),
        BreathingState::Inactive,
        "Failed 应维持现有错误色静态高亮，不应呼吸"
    );
    assert_eq!(
        agent_breathing_state(Some(AgentTaskState::Ended), None),
        BreathingState::Inactive,
        "Ended 不应呼吸"
    );
    assert_eq!(
        agent_breathing_state(None, None),
        BreathingState::Inactive,
        "缺失 task state 时不应呼吸"
    );
    assert!(
        !agent_chase_intent(Some(AgentTaskState::Done)),
        "Done 不应激活 Chase"
    );
}

/// Done 的 breathing intent 由通用 attention viewed 状态控制。
///
/// 数据构造（含关键数值推导）：
///   AgentTaskState::Done 表示 provider 已完成当前任务。
///   unviewed attention = AgentTaskDone{viewed=false}
///   viewed attention   = AgentTaskDone{viewed=true}
///
/// 执行过程：
///   1. 调用 agent_breathing_intent(Done, unviewed)
///   2. 调用 agent_breathing_intent(Done, viewed)
///
/// 预期结果：
///   - 正断言：未查看 Done 使用 BreathingIntent::Cool
///   - 正断言：已查看 Done 使用 BreathingIntent::Inactive
///   - 负断言：已查看 Done 不应继续保持 Cool
#[test]
fn test_agent_breathing_intent_uses_attention_for_done_prompt() {
    let unviewed_done = Some(CardAttentionState {
        kind: CardAttentionKind::AgentTaskDone,
        viewed: false,
    });
    let viewed_done = Some(CardAttentionState {
        kind: CardAttentionKind::AgentTaskDone,
        viewed: true,
    });

    assert_eq!(
        agent_breathing_intent(Some(AgentTaskState::Done), unviewed_done),
        BreathingIntent::Cool,
        "未查看 Done attention 应使用 Cool 呼吸提示用户查看结果"
    );
    assert_eq!(
        agent_breathing_state(Some(AgentTaskState::Done), unviewed_done),
        BreathingState::Cool,
        "未查看 Done 应持续参与 Cool 循环呼吸"
    );
    assert_eq!(
        agent_breathing_intent(Some(AgentTaskState::Done), viewed_done),
        BreathingIntent::Inactive,
        "已查看 Done attention 应关闭呼吸灯"
    );
    assert_ne!(
        agent_breathing_intent(Some(AgentTaskState::Done), viewed_done),
        BreathingIntent::Cool,
        "已查看 Done 不应继续保持 Cool 呼吸"
    );
}
