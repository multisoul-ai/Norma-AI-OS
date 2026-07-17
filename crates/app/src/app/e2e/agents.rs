use crate::app::MultisoulApp;
#[cfg(feature = "e2e-test-support")]
use gpui::Window;
#[cfg(feature = "e2e-test-support")]
use norma::protocol::{CardCommand, CardCommandResult};

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
    pub draft_name: String,
    pub draft_profile: &'static str,
    pub draft_workflow: String,
    pub creating_new: bool,
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
    pub fn enqueue_agent_hook_event_for_e2e(
        &self,
        event: norma::agent_hook::AgentHookEvent,
    ) -> Result<(), String> {
        self.agent_hook_server.enqueue_event_for_e2e(event)
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn agent_submission_waiting_for_e2e(&self, node_id: canvas::NodeId) -> bool {
        self.pending_agent_submissions.is_waiting(node_id)
    }

    pub fn agent_settings_e2e_snapshot(&self) -> AgentSettingsE2eSnapshot {
        let profile_name = |profile| match profile {
            canvas::AgentProfile::Claude => "claude",
            canvas::AgentProfile::Codex => "codex",
            canvas::AgentProfile::Cursor => "cursor",
            canvas::AgentProfile::OpenCode => "opencode",
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
            draft_name: self.agent_definitions.draft().name.clone(),
            draft_profile: profile_name(self.agent_definitions.draft().runtime_profile),
            draft_workflow: self.agent_definitions.draft().workflow_description.clone(),
            creating_new: self.agent_definitions.is_creating_new(),
        }
    }
}
