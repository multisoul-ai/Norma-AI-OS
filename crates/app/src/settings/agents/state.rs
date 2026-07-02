use canvas::AgentProfile;
use storage::{AgentDefinition, AgentDefinitionStore};

pub(crate) const MAX_AGENT_NAME_CHARS: usize = 80;
pub(crate) const MAX_WORKFLOW_DESCRIPTION_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AgentDefinitionField {
    Name,
    WorkflowDescription,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentDefinitionDraft {
    pub(crate) id: Option<i64>,
    pub(crate) name: String,
    pub(crate) runtime_profile: AgentProfile,
    pub(crate) workflow_description: String,
    pub(crate) enabled: bool,
    pub(crate) sort_order: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentVoiceContext {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) runtime_profile: AgentProfile,
    pub(crate) workflow_description: String,
}

#[derive(Clone, Debug)]
pub(crate) struct AgentDefinitionsState {
    definitions: Vec<AgentDefinition>,
    store: Option<AgentDefinitionStore>,
    draft: AgentDefinitionDraft,
    status: Option<String>,
}

impl Default for AgentDefinitionsState {
    fn default() -> Self {
        Self {
            definitions: Vec::new(),
            store: None,
            draft: AgentDefinitionDraft::new(),
            status: None,
        }
    }
}

impl AgentDefinitionDraft {
    pub(crate) fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
            enabled: true,
            sort_order: 0,
        }
    }
}

impl AgentDefinitionsState {
    pub(crate) fn bind_project_store(&mut self, store: AgentDefinitionStore) {
        self.store = Some(store);
        self.reload();
    }

    pub(crate) fn clear_project_store(&mut self) {
        self.store = None;
        self.definitions.clear();
        self.draft = AgentDefinitionDraft::new();
        self.status = None;
    }

    pub(crate) fn definitions(&self) -> &[AgentDefinition] {
        &self.definitions
    }

    pub(crate) fn draft(&self) -> &AgentDefinitionDraft {
        &self.draft
    }

    pub(crate) fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub(crate) fn has_project_store(&self) -> bool {
        self.store.is_some()
    }

    pub(crate) fn set_draft_name(&mut self, value: impl Into<String>) {
        self.draft.name = value.into();
    }

    pub(crate) fn set_draft_workflow_description(&mut self, value: impl Into<String>) {
        self.draft.workflow_description = value.into();
    }

    pub(crate) fn set_draft_runtime_profile(&mut self, profile: AgentProfile) {
        self.draft.runtime_profile = profile;
    }

    pub(crate) fn toggle_draft_enabled(&mut self) {
        self.draft.enabled = !self.draft.enabled;
    }

    pub(crate) fn reset_draft(&mut self) {
        self.draft = AgentDefinitionDraft::new();
    }

    pub(crate) fn load_draft_from_definition(&mut self, id: i64) -> bool {
        let Some(definition) = self
            .definitions
            .iter()
            .find(|definition| definition.id == id)
        else {
            return false;
        };

        self.draft = AgentDefinitionDraft {
            id: Some(definition.id),
            name: definition.name.clone(),
            runtime_profile: definition.runtime_profile,
            workflow_description: definition.workflow_description.clone(),
            enabled: definition.enabled,
            sort_order: definition.sort_order,
        };
        true
    }

    pub(crate) fn set_status(&mut self, status: impl Into<String>) {
        self.status = Some(status.into());
    }

    pub(crate) fn clear_status(&mut self) {
        self.status = None;
    }

    pub(crate) fn validate_draft(&self) -> Result<(), String> {
        validate_agent_definition_text(&self.draft.name, &self.draft.workflow_description)
    }

    pub(crate) fn voice_context(&self) -> Vec<AgentVoiceContext> {
        self.definitions
            .iter()
            .filter(|definition| definition.enabled)
            .map(|definition| AgentVoiceContext {
                id: definition.id,
                name: definition.name.clone(),
                runtime_profile: definition.runtime_profile,
                workflow_description: definition.workflow_description.clone(),
            })
            .collect()
    }

    fn reload(&mut self) {
        let Some(store) = self.store.as_ref() else {
            self.definitions.clear();
            return;
        };

        match store.list() {
            Ok(definitions) => {
                self.definitions = definitions;
                self.status = None;
            }
            Err(error) => {
                self.definitions.clear();
                self.status = Some(format!("Could not load project agents: {error}"));
            }
        }
    }
}

pub(crate) fn validate_agent_definition_text(
    name: &str,
    workflow_description: &str,
) -> Result<(), String> {
    let name = name.trim();
    let workflow = workflow_description.trim();

    if name.is_empty() {
        return Err("Agent name is required".to_string());
    }
    if name.chars().count() > MAX_AGENT_NAME_CHARS {
        return Err("Agent name must be 80 characters or fewer".to_string());
    }
    if workflow.is_empty() {
        return Err("Workflow description is required".to_string());
    }
    if workflow.chars().count() > MAX_WORKFLOW_DESCRIPTION_CHARS {
        return Err("Workflow description must be 2000 characters or fewer".to_string());
    }

    Ok(())
}
