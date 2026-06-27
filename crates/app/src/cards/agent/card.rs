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
use runtime::{AgentTaskState, NodeResize, Runtime};

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

pub fn agent_breathing_intent(state: Option<AgentTaskState>) -> BreathingIntent {
    match state {
        Some(AgentTaskState::Thinking) | Some(AgentTaskState::RunningTool { .. }) => {
            BreathingIntent::Inactive
        }
        Some(AgentTaskState::WaitingApproval { .. }) => BreathingIntent::Warm,
        Some(AgentTaskState::Done) => BreathingIntent::Cool,
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

    fn agent_breathing_state(state: Option<AgentTaskState>) -> BreathingState {
        agent_breathing_intent(state).breathing_state()
    }

    /// AgentCard chrome 使用 Agent profile 作为标题。
    ///
    /// 数据构造（含关键数值推导）：
    ///   Claude node: id=1, kind=Agent(profile=Claude)
    ///   Codex node:  id=2, kind=Agent(profile=Codex)
    ///   profile display title 分别为 "Claude" / "Codex"
    ///
    /// 执行过程：
    ///   1. 构造两个 CanvasNode
    ///   2. 调用 AgentCard.chrome
    ///
    /// 预期结果：
    ///   - 正断言：Claude/Codex title 分别正确
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

        let claude_chrome = CARD.chrome(&claude, &runtime);
        let codex_chrome = CARD.chrome(&codex, &runtime);

        assert_eq!(
            claude_chrome.title, "Claude",
            "Claude Agent card 标题应显示 Claude"
        );
        assert_eq!(
            codex_chrome.title, "Codex",
            "Codex Agent card 标题应显示 Codex"
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
            agent_breathing_state(Some(AgentTaskState::Thinking)),
            BreathingState::Inactive,
            "Thinking 由 Chase 动画驱动，breathing 应为 Inactive"
        );
        assert_eq!(
            agent_breathing_state(Some(AgentTaskState::RunningTool { tool_name: None })),
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
            agent_breathing_state(Some(AgentTaskState::WaitingApproval { tool_name: None })),
            BreathingState::Warm,
            "WaitingApproval 应映射为暖色呼吸"
        );
        assert!(
            !agent_chase_intent(Some(AgentTaskState::WaitingApproval { tool_name: None })),
            "WaitingApproval 不应激活 Chase"
        );
    }

    /// Done 使用 Cool 呼吸（青蓝紫循环），与 Running 的 Chase 动画形成视觉区分。
    /// Ready/Failed/Ended/None 保持 Inactive（静止）。
    ///
    /// 数据构造：
    ///   Done → Cool 呼吸（视觉上"完成但仍在线"）
    ///   其余终止态 → Inactive（静止）
    ///
    /// 执行过程：
    ///   1. 分别构造 Ready、Done、Failed、Ended、None
    ///   2. 调用 agent_breathing_state 映射
    ///
    /// 预期结果：
    ///   - Done 映射为 Cool 呼吸
    ///   - Ready/Failed/Ended/None 仍为 Inactive
    ///   - Done 不应激活 Chase
    #[test]
    fn test_agent_breathing_state_keeps_terminal_states_static() {
        assert_eq!(
            agent_breathing_state(Some(AgentTaskState::Ready)),
            BreathingState::Inactive,
            "Ready 不应呼吸"
        );
        assert_eq!(
            agent_breathing_state(Some(AgentTaskState::Done)),
            BreathingState::Cool,
            "Done 应切换为 Cool 呼吸，表达完成后的在线感"
        );
        assert_eq!(
            agent_breathing_state(Some(AgentTaskState::Failed {
                message: "boom".to_string()
            })),
            BreathingState::Inactive,
            "Failed 应维持现有错误色静态高亮，不应呼吸"
        );
        assert_eq!(
            agent_breathing_state(Some(AgentTaskState::Ended)),
            BreathingState::Inactive,
            "Ended 不应呼吸"
        );
        assert_eq!(
            agent_breathing_state(None),
            BreathingState::Inactive,
            "缺失 task state 时不应呼吸"
        );
        assert!(
            !agent_chase_intent(Some(AgentTaskState::Done)),
            "Done 不应激活 Chase"
        );
    }

    /// Done 的 breathing intent 为 Cool，让完成态保持青蓝紫呼吸而非绿色闪烁。
    ///
    /// 数据构造：
    ///   AgentTaskState::Done 表示 provider 已完成当前任务。
    ///
    /// 执行过程：
    ///   1. 调用 agent_breathing_intent(Done)
    ///   2. 调用 agent_breathing_state(Done) 验证 Cool 循环呼吸激活
    ///
    /// 预期结果：
    ///   - 正断言：intent 为 BreathingIntent::Cool
    ///   - 正断言：breathing_state 为 Cool（持续循环呼吸）
    ///   - 负断言：Done 不应退化为 BreathingIntent::Inactive
    #[test]
    fn test_agent_breathing_intent_preserves_done_for_completion_settle() {
        assert_eq!(
            agent_breathing_intent(Some(AgentTaskState::Done)),
            BreathingIntent::Cool,
            "Done intent 应使用 Cool 呼吸表达完成后在线状态"
        );
        assert_eq!(
            agent_breathing_state(Some(AgentTaskState::Done)),
            BreathingState::Cool,
            "Done 应持续参与 Cool 循环呼吸"
        );
        assert_ne!(
            agent_breathing_intent(Some(AgentTaskState::Done)),
            BreathingIntent::Inactive,
            "Done 不应退化为 Inactive，否则完成后无呼吸动画"
        );
    }
}
