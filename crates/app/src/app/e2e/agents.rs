use crate::app::MultisoulApp;
#[cfg(feature = "e2e-test-support")]
use gpui::{Context, Window};
#[cfg(feature = "e2e-test-support")]
use norma::protocol::{CardCommand, CardCommandResult, WireRequest, WireResponse};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentDefinitionE2eSnapshot {
    pub id: i64,
    pub name: String,
    pub profile: &'static str,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentSettingsE2eSnapshot {
    pub definitions: Vec<AgentDefinitionE2eSnapshot>,
    pub visible_definition_names: Vec<String>,
    pub search_query: String,
    pub draft_name: String,
    pub draft_profile: &'static str,
    pub draft_workflow: String,
    pub draft_enabled: bool,
    pub creating_new: bool,
    pub dirty: bool,
}

#[cfg(feature = "e2e-test-support")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresentationE2eSnapshot {
    pub master_id: Option<u64>,
    pub selected_node_ids: Vec<u64>,
    pub canvas_focused: bool,
    pub notification_source: Option<String>,
    pub notification_count: usize,
    pub preview_states: Vec<(u64, &'static str)>,
}

impl MultisoulApp {
    pub fn current_canvas_context_e2e(&self) -> Option<agent::CurrentCanvasContext> {
        self.current_canvas_context_for_provider()
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn execute_agent_card_command_for_e2e(
        &mut self,
        window: &mut Window,
        command: CardCommand,
    ) -> Result<CardCommandResult, String> {
        crate::app::agent_runtime::execute_agent_card_command(self, window, command)
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn register_external_agent_root_for_e2e(
        &mut self,
        node_id: canvas::NodeId,
        task_title: Option<String>,
    ) -> bool {
        self.register_external_agent_root(node_id, task_title)
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn install_presentation_authorization_for_e2e(
        &mut self,
        path: std::path::PathBuf,
        provider: &str,
        token: &str,
    ) -> Result<(), String> {
        let store = storage::OrchestrationAuthorizationStore::new(path);
        store
            .authorize(provider, token, env!("CARGO_PKG_VERSION"))
            .map_err(|error| error.to_string())?;
        self.orchestration_authorizations = storage::OrchestrationAuthorizationCache::new(
            store.list().map_err(|error| error.to_string())?,
        );
        Ok(())
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn execute_norma_wire_request_for_e2e(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        request: WireRequest,
    ) -> WireResponse {
        self.execute_card_command(request, Some(window), cx)
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn presentation_e2e_snapshot(&self) -> PresentationE2eSnapshot {
        let notification = self.presentation_notification.as_ref();
        PresentationE2eSnapshot {
            master_id: self.master_layout.master_id(),
            selected_node_ids: self.runtime.selection.iter().collect(),
            canvas_focused: matches!(self.runtime.focus.mode(), runtime::InputMode::CanvasSelect),
            notification_source: notification.map(|notification| notification.source_name.clone()),
            notification_count: notification
                .map(|notification| notification.count)
                .unwrap_or(0),
            preview_states: self
                .runtime
                .document
                .nodes()
                .iter()
                .filter_map(|node| {
                    let canvas::CanvasNodeKind::Preview(_) = &node.kind else {
                        return None;
                    };
                    let state = match self.previews.state_for(node.id) {
                        Some(crate::preview::PreviewLoadState::Loading) => "loading",
                        Some(crate::preview::PreviewLoadState::Ready(_)) => "ready",
                        Some(crate::preview::PreviewLoadState::Error(_)) => "error",
                        None => "missing",
                    };
                    Some((node.id, state))
                })
                .collect(),
        }
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn report_browser_location_for_e2e(
        &mut self,
        node_id: u64,
        url: String,
        cx: &mut Context<Self>,
    ) -> bool {
        self.browsers.report_location_for_e2e(node_id, url);
        self.drain_browser_events(cx)
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn execute_norma_adapter_command_for_e2e(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        command: agent::AdapterCommand,
    ) -> Result<agent::AdapterResult, String> {
        crate::app::agent_runtime::execute_norma_adapter_command(self, window, cx, command)
            .map_err(|error| error.to_string())
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn execute_voice_norma_adapter_command_for_e2e(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        command: agent::AdapterCommand,
    ) -> Result<agent::AdapterResult, String> {
        crate::app::agent_runtime::execute_norma_adapter_command_with_source(
            self,
            Some(window),
            cx,
            command,
            agent::AgentInputSource::Voice,
        )
        .map_err(|error| error.to_string())
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn execute_background_norma_adapter_command_for_e2e(
        &mut self,
        cx: &mut Context<Self>,
        command: agent::AdapterCommand,
    ) -> Result<agent::AdapterResult, String> {
        crate::app::agent_runtime::execute_norma_adapter_command_with_source(
            self,
            None,
            cx,
            command,
            agent::AgentInputSource::Voice,
        )
        .map_err(|error| error.to_string())
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn enqueue_agent_hook_event_for_e2e(
        &self,
        event: norma::agent_hook::AgentHookEvent,
    ) -> Result<(), String> {
        self.agent_hook_server.enqueue_event_for_e2e(event)
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn observe_agent_hook_events_for_e2e(
        &mut self,
    ) -> crossbeam_channel::Receiver<norma::agent_hook::AgentHookEvent> {
        self.agent_hook_server.observe_events_for_e2e()
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn agent_submission_waiting_for_e2e(&self, node_id: canvas::NodeId) -> bool {
        self.pending_agent_submissions.is_waiting(node_id)
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn agent_task_count_for_e2e(&self) -> usize {
        self.agent_task_coordinator
            .as_ref()
            .map(|coordinator| coordinator.tasks().len())
            .unwrap_or(0)
    }

    pub fn agent_settings_e2e_snapshot(&self) -> AgentSettingsE2eSnapshot {
        let profile_name = |profile| match profile {
            canvas::AgentProfile::Claude => "claude",
            canvas::AgentProfile::Codex => "codex",
            canvas::AgentProfile::Cursor => "cursor",
            canvas::AgentProfile::Grok => "grok",
            canvas::AgentProfile::OpenCode => "opencode",
            canvas::AgentProfile::InfCode => "infcode",
            canvas::AgentProfile::Kimi => "kimi",
            canvas::AgentProfile::ZCode => "zcode",
            canvas::AgentProfile::Pi => "pi",
            canvas::AgentProfile::Hermes => "hermes",
        };
        AgentSettingsE2eSnapshot {
            definitions: self
                .agent_definitions
                .definitions()
                .iter()
                .map(|definition| AgentDefinitionE2eSnapshot {
                    id: definition.id,
                    name: definition.name.clone(),
                    profile: profile_name(definition.runtime_profile),
                    enabled: definition.enabled,
                })
                .collect(),
            visible_definition_names: self
                .agent_definitions
                .visible_definitions()
                .into_iter()
                .map(|definition| definition.name.clone())
                .collect(),
            search_query: self.agent_definitions.search_query().to_string(),
            draft_name: self.agent_definitions.draft().name.clone(),
            draft_profile: profile_name(self.agent_definitions.draft().runtime_profile),
            draft_workflow: self.agent_definitions.draft().workflow_description.clone(),
            draft_enabled: self.agent_definitions.draft().enabled,
            creating_new: self.agent_definitions.is_creating_new(),
            dirty: self.agent_definitions.is_dirty(),
        }
    }
}
