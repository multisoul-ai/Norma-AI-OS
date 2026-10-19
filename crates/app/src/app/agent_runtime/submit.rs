use super::{
    contextualized_agent_commands, start_adapter_pump, start_event_pump, ProjectAgentRuntime,
};
use crate::app::MultisoulApp;
use agent::{AdapterError, AdapterResult, AgentCommand, AgentEvent};
use gpui::{Context, Window};

impl MultisoulApp {
    pub(crate) fn submit_tracked_agent_text(
        &mut self,
        node_id: canvas::NodeId,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Result<AdapterResult, AdapterError> {
        let definition_id = self
            .runtime
            .document
            .node(node_id)
            .and_then(|node| match &node.kind {
                canvas::CanvasNodeKind::Agent(spec) => spec.definition_id,
                _ => None,
            })
            .ok_or_else(|| AdapterError::Rejected("目标 Agent 卡片已失效或缺少任务身份".into()))?;
        let agent = self
            .agent_definitions
            .definitions()
            .iter()
            .find(|definition| definition.id == definition_id)
            .map(|definition| definition.name.clone())
            .ok_or_else(|| AdapterError::Rejected("目标 Agent 定义已失效".into()))?;
        let active_node_id = self
            .agent_roster_for_provider()
            .into_iter()
            .find(|entry| entry.definition_id == definition_id)
            .and_then(|entry| entry.active_node_id);
        if active_node_id != Some(node_id) {
            return Err(AdapterError::Rejected(
                "目标 Agent 卡片已变化，请重新查询目标".into(),
            ));
        }
        super::adapter_task::dispatch_agent_task(self, None, cx, agent, text.to_owned(), false)
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
