//! Agent card renderer.

use crate::cards::chrome::{CardChrome, CardChromeTaskBadge, CardChromeTaskTone};
use crate::cards::term::card::term_resize;
use crate::cards::term::view::render_terminal_node;
use crate::cards::{Card, CardBodyContext};
use crate::motion::breathing::BreathingIntent;
#[cfg(test)]
use crate::motion::breathing::BreathingState;
use canvas::{CanvasNode, CanvasNodeKind, CanvasRect, NodeType};
use gpui::AnyElement;
use runtime::{AgentTaskState, CardAttentionKind, CardAttentionState, NodeResize, Runtime};

pub struct AgentCard;

pub static CARD: AgentCard = AgentCard;

impl Card for AgentCard {
    fn ty(&self) -> NodeType {
        NodeType::Agent
    }

    fn chrome(&self, node: &CanvasNode, runtime: &Runtime) -> CardChrome {
        let title = match &node.kind {
            CanvasNodeKind::Agent(agent) => agent
                .alias
                .as_deref()
                .unwrap_or_else(|| agent.profile.display_title())
                .to_string(),
            _ => "Agent".to_string(),
        };
        CardChrome {
            title,
            refreshable: false,
            task_badge: agent_task_badge(runtime.agent_task_state(node.id)),
        }
    }

    fn body(&self, node: &CanvasNode, context: CardBodyContext<'_>) -> AnyElement {
        let selection = context
            .text_selection
            .filter(|selection| selection.node_id == node.id && !selection.is_empty())
            .map(|selection| selection.range());
        render_terminal_node(
            context.runtime.node_view(node.id),
            context.theme,
            context.zoom,
            context.bottom_radius_px,
            selection,
            context.focused,
            context.reduce_motion,
            context.terminal_cursor_phase,
        )
    }

    fn resize(&self, bounds: CanvasRect, zoom: f32) -> Option<NodeResize> {
        Some(term_resize(bounds, zoom))
    }

    fn wheel_scroll_mode(&self) -> crate::cards::CardWheelScrollMode {
        crate::cards::CardWheelScrollMode::TerminalScrollback
    }
}

fn agent_task_badge(state: Option<AgentTaskState>) -> Option<CardChromeTaskBadge> {
    let state = state?;
    let (label, tone) = match state {
        AgentTaskState::Ready => ("Ready", CardChromeTaskTone::Muted),
        AgentTaskState::Thinking => ("Thinking", CardChromeTaskTone::Active),
        AgentTaskState::RunningTool { .. } => ("Tool", CardChromeTaskTone::Active),
        AgentTaskState::WaitingApproval { .. } => ("Approval", CardChromeTaskTone::Warning),
        AgentTaskState::Done => ("Done", CardChromeTaskTone::Muted),
        AgentTaskState::Failed { .. } => ("Failed", CardChromeTaskTone::Error),
        AgentTaskState::Ended => ("Ended", CardChromeTaskTone::Muted),
    };
    Some(CardChromeTaskBadge {
        label: label.to_string(),
        tone,
    })
}

pub fn agent_breathing_intent(
    state: Option<AgentTaskState>,
    attention: Option<CardAttentionState>,
) -> BreathingIntent {
    match state {
        Some(AgentTaskState::Thinking) | Some(AgentTaskState::RunningTool { .. }) => {
            BreathingIntent::Inactive
        }
        Some(AgentTaskState::WaitingApproval { .. }) => BreathingIntent::Warm,
        Some(AgentTaskState::Done) => match attention {
            Some(CardAttentionState {
                kind: CardAttentionKind::AgentTaskDone,
                viewed: false,
            }) => BreathingIntent::Cool,
            Some(CardAttentionState {
                kind: CardAttentionKind::AgentTaskDone,
                viewed: true,
            })
            | None => BreathingIntent::Inactive,
        },
        Some(AgentTaskState::Ready)
        | Some(AgentTaskState::Failed { .. })
        | Some(AgentTaskState::Ended)
        | None => BreathingIntent::Inactive,
    }
}

pub fn agent_chase_intent(state: Option<AgentTaskState>) -> bool {
    matches!(
        state,
        Some(AgentTaskState::Thinking) | Some(AgentTaskState::RunningTool { .. })
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use canvas::{AgentProfile, AgentSpec, CanvasRect};

    fn agent_breathing_state(
        state: Option<AgentTaskState>,
        attention: Option<CardAttentionState>,
    ) -> BreathingState {
        agent_breathing_intent(state, attention).breathing_state()
    }

    /// AgentCard chrome 使用 Agent profile 作为标题。
    ///
    /// 数据构造（含关键数值推导）：
    ///   Claude node: id=1, kind=Agent(profile=Claude)
    ///   Codex node:  id=2, kind=Agent(profile=Codex)
    ///   OpenCode node: id=3, kind=Agent(profile=OpenCode)
    ///   profile display title 分别为 "Claude" / "Codex" / "OpenCode"
    ///
    /// 执行过程：
    ///   1. 构造三个 CanvasNode
    ///   2. 调用 AgentCard.chrome
    ///
    /// 预期结果：
    ///   - 正断言：Claude/Codex/OpenCode title 分别正确
    ///   - 负断言：Claude title 不应退化为 "Agent" 或 "Terminal"
    #[test]
    fn test_agent_card_chrome_uses_profile_title() {
        let runtime = Runtime::new();
        let claude = CanvasNode {
            id: 1,
            kind: CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Claude,
                alias: None,
            }),
            bounds: CanvasRect::new(0.0, 0.0, 100.0, 80.0),
            z_index: 1,
            selected: false,
        };
        let codex = CanvasNode {
            id: 2,
            kind: CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Codex,
                alias: None,
            }),
            bounds: CanvasRect::new(0.0, 0.0, 100.0, 80.0),
            z_index: 2,
            selected: false,
        };
        let opencode = CanvasNode {
            id: 3,
            kind: CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::OpenCode,
                alias: None,
            }),
            bounds: CanvasRect::new(0.0, 0.0, 100.0, 80.0),
            z_index: 3,
            selected: false,
        };

        let claude_chrome = CARD.chrome(&claude, &runtime);
        let codex_chrome = CARD.chrome(&codex, &runtime);
        let opencode_chrome = CARD.chrome(&opencode, &runtime);

        assert_eq!(
            claude_chrome.title, "Claude",
            "Claude Agent card 标题应显示 Claude"
        );
        assert_eq!(
            codex_chrome.title, "Codex",
            "Codex Agent card 标题应显示 Codex"
        );
        assert_eq!(
            opencode_chrome.title, "OpenCode",
            "OpenCode Agent card 标题应显示 OpenCode"
        );
        assert_ne!(
            claude_chrome.title, "Agent",
            "Claude Agent card 不应退化为泛化 Agent 标题"
        );
        assert_ne!(
            claude_chrome.title, "Terminal",
            "Claude Agent card 不应退化为 Terminal 标题"
        );
    }

    /// AgentCard chrome 应优先使用 alias，缺失 alias 时回退 profile。
    ///
    /// 数据构造（含关键数值推导）：
    ///   node_id=3，profile=Codex，alias=Alan
    ///   runtime 为空；chrome 标题只依赖 CanvasNodeKind::Agent
    ///
    /// 执行过程：
    ///   1. 构造带 alias 的 Agent node
    ///   2. 调用 AgentCard.chrome
    ///   3. 检查 title
    ///
    /// 预期结果：
    ///   - 正断言：title 为 Alan
    ///   - 负断言：title 不应是 Codex 或泛化 Agent
    #[test]
    fn test_agent_card_chrome_prefers_alias_over_profile_title() {
        let runtime = Runtime::new();
        let node = CanvasNode {
            id: 3,
            kind: CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Codex,
                alias: Some("Alan".to_string()),
            }),
            bounds: CanvasRect::new(0.0, 0.0, 100.0, 80.0),
            z_index: 3,
            selected: false,
        };

        let chrome = CARD.chrome(&node, &runtime);

        assert_eq!(chrome.title, "Alan", "Agent alias 应优先作为 card 标题");
        assert_ne!(chrome.title, "Codex", "带 alias 时不应显示 profile title");
        assert_ne!(chrome.title, "Agent", "带 alias 时不应退化为泛化 Agent");
    }

    /// AgentCard chrome 会把 runtime task state 映射成短 badge。
    ///
    /// 数据构造（含关键数值推导）：
    ///   runtime 添加 node_id=1 的 Codex Agent
    ///   task update: PreToolUse 等价状态 RunningTool(tool_name=shell)
    ///   badge 文案 V1 固定为短标签 "Tool"，避免标题栏溢出
    ///
    /// 执行过程：
    ///   1. 在 runtime.document 中添加 Agent node
    ///   2. apply_agent_task_update 写入 RunningTool
    ///   3. 调用 AgentCard.chrome 读取 badge
    ///
    /// 预期结果：
    ///   - 正断言：badge 存在且 label 为 Tool
    ///   - 负断言：badge 不应显示 Terminal，也不应退化为 None
    #[test]
    fn test_agent_card_chrome_shows_task_badge() {
        let mut runtime = Runtime::new();
        let node_id = runtime.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Codex,
                alias: None,
            }),
            CanvasRect::new(0.0, 0.0, 100.0, 80.0),
        );
        let changed = runtime.apply_agent_task_update(runtime::AgentTaskUpdate {
            node_id,
            profile: AgentProfile::Codex,
            provider_event: "PreToolUse".to_string(),
            to: AgentTaskState::RunningTool {
                tool_name: Some("shell".to_string()),
            },
        });
        let node = runtime
            .document
            .node(node_id)
            .expect("Agent node 应存在于 runtime document")
            .clone();

        let chrome = CARD.chrome(&node, &runtime);
        let badge = chrome
            .task_badge
            .expect("RunningTool 状态应渲染 task badge");

        assert!(changed, "Agent task update 应改变 runtime task state");
        assert_eq!(badge.label, "Tool", "RunningTool 应显示短标签 Tool");
        assert_ne!(
            badge.label, "Terminal",
            "Agent task badge 不应显示 Terminal 文案"
        );
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
            kind: runtime::CardAttentionKind::AgentTaskDone,
            viewed: false,
        });
        let viewed_done = Some(CardAttentionState {
            kind: runtime::CardAttentionKind::AgentTaskDone,
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
            kind: runtime::CardAttentionKind::AgentTaskDone,
            viewed: false,
        });
        let viewed_done = Some(CardAttentionState {
            kind: runtime::CardAttentionKind::AgentTaskDone,
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
}
