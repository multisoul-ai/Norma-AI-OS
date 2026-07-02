//! Agent card renderer.

use crate::cards::chrome::{CardChrome, CardChromeTaskBadge, CardChromeTaskTone};
use crate::cards::term::card::term_resize;
use crate::cards::term::view::{render_terminal_node, TerminalNodeRenderOptions};
use crate::cards::{Card, CardBodyContext};
use crate::motion::breathing::BreathingIntent;
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
        // Title format: "<profile/alias>" or "<profile/alias> — <task_title>"
        let profile_name = match &node.kind {
            CanvasNodeKind::Agent(agent) => agent
                .alias
                .as_deref()
                .unwrap_or_else(|| agent.profile.display_title())
                .to_string(),
            _ => "Agent".to_string(),
        };
        let title = match runtime.agent_task_title(node.id) {
            Some(task_title) => format!("{profile_name} — {task_title}"),
            None => profile_name,
        };
        let state = runtime.agent_task_state(node.id);
        CardChrome {
            title,
            refreshable: false,
            task_badge: agent_task_badge(state),
        }
    }

    fn body(&self, node: &CanvasNode, context: CardBodyContext<'_>) -> AnyElement {
        let selection = context
            .text_selection
            .filter(|selection| selection.node_id == node.id && !selection.is_empty())
            .map(|selection| selection.range());
        render_terminal_node(
            context.runtime.node_view(node.id),
            TerminalNodeRenderOptions {
                theme: context.theme,
                zoom: context.zoom,
                bottom_radius_px: context.bottom_radius_px,
                selection,
                focused: context.focused,
                reduce_motion: context.reduce_motion,
                cursor_phase: context.terminal_cursor_phase,
            },
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
#[path = "../../../tests/unit/cards/agent/card_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/cards/agent/breathing_tests.rs"]
mod breathing_tests;
