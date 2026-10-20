//! Agent card renderer.

use crate::canvas::terminal_path_cache::TerminalPathScope;
use crate::cards::chrome::{CardChrome, CardChromeTaskBadge, CardChromeTaskTone};
use crate::cards::term::card::term_resize;
use crate::cards::term::view::{
    render_terminal_node, terminal_content_mode, TerminalNodeRenderOptions,
};
use crate::cards::{Card, CardBodyContext};
use crate::motion::breathing::BreathingIntent;
use canvas::{CanvasNode, CanvasNodeKind, CanvasRect, NodeType};
use gpui::AnyElement;
use norma::mobile::{status_capsule_title, StatusCapsuleKind, TaskProgress};
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
            _ => ui::i18n::text("cards.agent").to_string(),
        };
        let task_title = runtime
            .agent_task_title(node.id)
            .and_then(|task_title| single_line_task_title(&task_title));
        let location = runtime.agent_worktree(node.id).and_then(|worktree| {
            let project = runtime.workspace_root()?.file_name()?.to_str()?;
            let worktree = worktree.file_name()?.to_str()?;
            Some(format!("{project} / {worktree}"))
        });
        let title_detail = match (location, task_title) {
            (Some(location), Some(task)) => Some(format!("{location} · {task}")),
            (Some(location), None) => Some(location),
            (None, task) => task,
        };
        let state = runtime.agent_task_state(node.id);
        let progress = runtime.mobile_task_progress(node.id);
        let preparing = runtime.mobile_agent_is_preparing(node.id);
        CardChrome {
            title,
            title_detail,
            refreshable: false,
            task_badge: agent_task_badge(state, progress, preparing),
        }
    }

    fn body(&self, node: &CanvasNode, context: CardBodyContext<'_>) -> AnyElement {
        let content_mode = terminal_content_mode(context.initialization);
        let selection = context
            .text_selection
            .filter(|selection| selection.node_id == node.id && !selection.is_empty())
            .map(|selection| selection.range());
        let view = content_mode
            .requires_live_view()
            .then(|| context.runtime.node_view(node.id))
            .flatten();
        render_terminal_node(
            view,
            TerminalNodeRenderOptions {
                node_id: node.id,
                terminal_render_cache: context.terminal_render_cache,
                theme: context.theme,
                zoom: context.zoom,
                bottom_radius_px: context.bottom_radius_px,
                selection,
                focused: context.focused,
                reduce_motion: context.reduce_motion,
                initialization: context.initialization,
                cursor_phase: context.terminal_cursor_phase,
                path_scope: TerminalPathScope::for_node(
                    &context.terminal_path_cache,
                    context.runtime,
                    node.id,
                ),
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

fn single_line_task_title(title: &str) -> Option<String> {
    let mut words = title.split_whitespace();
    let first = words.next()?;
    let mut single_line = first.to_string();
    for word in words {
        single_line.push(' ');
        single_line.push_str(word);
    }
    Some(single_line)
}

fn agent_task_badge(
    state: Option<AgentTaskState>,
    progress: Option<TaskProgress>,
    preparing: bool,
) -> Option<CardChromeTaskBadge> {
    let state = state?;
    let (completed, total) = match progress {
        Some(progress) => (Some(progress.completed), Some(progress.total)),
        None => (None, None),
    };
    let settled = matches!(
        state,
        AgentTaskState::Thinking
            | AgentTaskState::RunningTool { .. }
            | AgentTaskState::WaitingApproval { .. }
            | AgentTaskState::Failed { .. }
    );
    if preparing && !settled {
        return Some(CardChromeTaskBadge {
            label: status_capsule_title(StatusCapsuleKind::PendingStart, None, None),
            tone: CardChromeTaskTone::Muted,
        });
    }
    let (label, tone) = match state {
        AgentTaskState::Ready => (
            status_capsule_title(StatusCapsuleKind::PendingStart, None, None),
            CardChromeTaskTone::Muted,
        ),
        AgentTaskState::Thinking | AgentTaskState::RunningTool { .. } => (
            status_capsule_title(StatusCapsuleKind::Running, completed, total),
            CardChromeTaskTone::Active,
        ),
        AgentTaskState::WaitingApproval { .. } => (
            status_capsule_title(StatusCapsuleKind::Waiting, completed, total),
            CardChromeTaskTone::Warning,
        ),
        AgentTaskState::Done => ("Done".to_string(), CardChromeTaskTone::Muted),
        AgentTaskState::Failed { .. } => (
            status_capsule_title(StatusCapsuleKind::Failed, completed, total),
            CardChromeTaskTone::Error,
        ),
        AgentTaskState::Ended => (
            ui::i18n::text("cards.ended").to_string(),
            CardChromeTaskTone::Muted,
        ),
    };
    Some(CardChromeTaskBadge { label, tone })
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
