//! Agent card creation actions for toolbar, menu, and shortcuts.

use super::{AppScreen, MultisoulApp, NewClaudeAgent, NewCodexAgent, NewOpenCodeAgent};
use crate::canvas::master_layout::term_grid_size_for_card;
use crate::canvas::spawn_focus;
use canvas::{AgentProfile, NodeType};
use gpui::{ClickEvent, Context, Window};

impl MultisoulApp {
    pub(crate) fn on_new_claude_agent(
        &mut self,
        _: &NewClaudeAgent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.screen, AppScreen::Canvas) {
            self.workspace_state.error = Some("Open a project before creating agents.".into());
            cx.notify();
            return;
        }
        self.spawn_agent_card(AgentProfile::Claude, window, cx);
    }

    pub(crate) fn on_new_codex_agent(
        &mut self,
        _: &NewCodexAgent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.screen, AppScreen::Canvas) {
            self.workspace_state.error = Some("Open a project before creating agents.".into());
            cx.notify();
            return;
        }
        self.spawn_agent_card(AgentProfile::Codex, window, cx);
    }

    pub(crate) fn on_new_opencode_agent(
        &mut self,
        _: &NewOpenCodeAgent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.screen, AppScreen::Canvas) {
            self.workspace_state.error = Some("Open a project before creating agents.".into());
            cx.notify();
            return;
        }
        self.spawn_agent_card(AgentProfile::OpenCode, window, cx);
    }

    pub(crate) fn spawn_agent_card(
        &mut self,
        profile: AgentProfile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = self
            .visible_canvas_rect(window)
            .inset(self.layout_gap_canvas());
        let (cols, rows) =
            term_grid_size_for_card(NodeType::Agent, bounds, self.runtime.document.viewport.zoom);
        match self.runtime.spawn_agent(profile, bounds, cols, rows) {
            Ok(node_id) => {
                tracing::info!(?node_id, ?profile, "spawned agent node");
                spawn_focus::activate_spawned_node(&mut self.runtime, node_id);
                self.motion.spawn_node(node_id, bounds);
                self.apply_layout_for_current_mode(window);
                self.mark_canvas_dirty();
                self.mark_session_dirty();
            }
            Err(e) => {
                tracing::error!(error = %e, ?profile, "failed to spawn agent");
            }
        }
        cx.notify();
    }

    pub(crate) fn on_toolbar_new_claude_agent(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.spawn_agent_card(AgentProfile::Claude, window, cx);
    }

    pub(crate) fn on_toolbar_new_codex_agent(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.spawn_agent_card(AgentProfile::Codex, window, cx);
    }

    pub(crate) fn on_toolbar_new_opencode_agent(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.spawn_agent_card(AgentProfile::OpenCode, window, cx);
    }
}
