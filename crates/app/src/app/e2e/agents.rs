use crate::app::MultisoulApp;

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
    pub fn agent_settings_e2e_snapshot(&self) -> AgentSettingsE2eSnapshot {
        let profile_name = |profile| match profile {
            canvas::AgentProfile::Claude => "claude",
            canvas::AgentProfile::Codex => "codex",
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
