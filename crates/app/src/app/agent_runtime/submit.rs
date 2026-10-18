use super::{
    contextualized_agent_commands, start_adapter_pump, start_event_pump, ProjectAgentRuntime,
};
use crate::app::MultisoulApp;
use agent::{AgentCommand, AgentEvent};
use gpui::{Context, Window};

impl MultisoulApp {
    pub(crate) fn submit_compact_agent_text(
        &mut self,
        node_id: canvas::NodeId,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let is_agent = self
            .runtime
            .document
            .node(node_id)
            .is_some_and(|node| matches!(node.kind, canvas::CanvasNodeKind::Agent(_)));
        if !is_agent {
            return Err("Target Agent card no longer exists".to_string());
        }
        super::command_support::type_agent_task(&mut self.runtime, node_id, text)
            .map_err(|error| error.to_string())?;
        super::command_support::submit_agent_enter(&mut self.runtime, node_id)
            .map_err(|error| error.to_string())?;
        cx.notify();
        Ok(())
    }

    pub(crate) fn submit_project_agent(
        &mut self,
        command: AgentCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<async_channel::Receiver<Result<Vec<AgentEvent>, String>>, String> {
        let config = self
            .voice_settings
            .values()
            .agent_provider_config()
            .map_err(|error| error.to_string())?;
        let root = self.workspace_state.current_root.clone();
        let project_id = root
            .as_ref()
            .map(|root| root.display().to_string())
            .unwrap_or_default();
        let must_replace = self
            .agent_runtime
            .as_ref()
            .is_none_or(|runtime| !runtime.matches_project(&project_id));
        if must_replace {
            let (runtime, adapter, events) =
                ProjectAgentRuntime::spawn(project_id, config, self.database_paths.clone(), root)?;
            let runtime_id = runtime.id();
            self.dialogue
                .set_dialogue_stream_cancellation(runtime.dialogue_cancel_handle());
            self.agent_runtime = Some(runtime);
            start_adapter_pump(adapter, runtime_id, window, cx);
            start_event_pump(events, runtime_id, cx);
        } else if self
            .agent_runtime
            .as_ref()
            .is_some_and(|runtime| !runtime.uses_config(&config))
        {
            let runtime = self
                .agent_runtime
                .as_ref()
                .expect("Norma runtime should exist while switching models");
            let _ = runtime.submit(AgentCommand::SwitchModel(config));
        }
        let roster = self.agent_roster_for_provider();
        let canvas = self.current_canvas_context_for_provider();
        let desktop = self.desktop_context_for_provider(cx);
        let mut commands = contextualized_agent_commands(roster, canvas, desktop, command);
        let requested = commands
            .pop()
            .expect("contextualized command batch should include the requested command");
        let runtime = self
            .agent_runtime
            .as_ref()
            .expect("Norma runtime should exist after initialization");
        for context in commands {
            let _ = runtime.submit(context);
        }
        Ok(runtime.submit(requested))
    }
}
