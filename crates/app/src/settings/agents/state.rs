use canvas::AgentProfile;
use gpui::Keystroke;
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
    editing: Option<AgentDefinitionField>,
    edit_buffer: String,
    status: Option<String>,
}

impl Default for AgentDefinitionsState {
    fn default() -> Self {
        Self {
            definitions: Vec::new(),
            store: None,
            draft: AgentDefinitionDraft::new(),
            editing: None,
            edit_buffer: String::new(),
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

impl AgentDefinitionField {
    pub(super) fn index(self) -> usize {
        match self {
            Self::Name => 0,
            Self::WorkflowDescription => 1,
        }
    }
}

impl AgentDefinitionsState {
    pub(crate) fn bind_project_store(&mut self, store: AgentDefinitionStore) {
        self.store = Some(store);
        self.reset_draft();
        self.reload();
    }

    pub(crate) fn clear_project_store(&mut self) {
        self.store = None;
        self.definitions.clear();
        self.reset_draft();
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
        self.editing = None;
        self.edit_buffer.clear();
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
        self.editing = None;
        self.edit_buffer.clear();
        true
    }

    pub(crate) fn save_draft(&mut self) -> bool {
        self.commit_pending_edit();

        if let Err(message) =
            validate_agent_definition_text(&self.draft.name, &self.draft.workflow_description)
        {
            self.status = Some(message);
            return false;
        }
        let Some(store) = self.store.as_ref() else {
            self.status = Some("Open a project before editing Agents.".to_string());
            return false;
        };

        let name = self.draft.name.trim().to_string();
        let workflow_description = self.draft.workflow_description.trim().to_string();
        let result = match self.draft.id {
            Some(id) => store
                .update(&storage::AgentDefinitionUpdate {
                    id,
                    name,
                    runtime_profile: self.draft.runtime_profile,
                    workflow_description,
                    enabled: self.draft.enabled,
                    sort_order: self.draft.sort_order,
                })
                .map(|_| ()),
            None => match store.create(&storage::NewAgentDefinition {
                name,
                runtime_profile: self.draft.runtime_profile,
                workflow_description,
            }) {
                Ok(created)
                    if created.enabled != self.draft.enabled
                        || created.sort_order != self.draft.sort_order =>
                {
                    store
                        .update(&storage::AgentDefinitionUpdate {
                            id: created.id,
                            name: created.name,
                            runtime_profile: created.runtime_profile,
                            workflow_description: created.workflow_description,
                            enabled: self.draft.enabled,
                            sort_order: self.draft.sort_order,
                        })
                        .map(|_| ())
                }
                Ok(_) => Ok(()),
                Err(error) => Err(error),
            },
        };

        match result {
            Ok(()) => {
                self.draft = AgentDefinitionDraft::new();
                self.editing = None;
                self.edit_buffer.clear();
                self.reload();
                true
            }
            Err(error) => {
                self.status = Some(readable_agent_store_error(&error));
                false
            }
        }
    }

    pub(crate) fn edit_definition(&mut self, id: i64) -> bool {
        if self.load_draft_from_definition(id) {
            self.status = None;
            true
        } else {
            self.status = Some("Could not load that agent into the draft.".to_string());
            false
        }
    }

    pub(crate) fn delete_definition(&mut self, id: i64) -> bool {
        let Some(store) = self.store.as_ref() else {
            self.status = Some("Open a project before editing Agents.".to_string());
            return false;
        };

        match store.delete(id) {
            Ok(true) => {
                if self.draft.id == Some(id) {
                    self.reset_draft();
                }
                self.reload();
                true
            }
            Ok(false) => {
                self.status = Some("Could not delete that agent.".to_string());
                false
            }
            Err(error) => {
                self.status = Some(format!("Could not delete project agent: {error}"));
                false
            }
        }
    }

    pub(crate) fn toggle_enabled(&mut self, id: i64) -> bool {
        let Some(definition) = self
            .definitions
            .iter()
            .find(|definition| definition.id == id)
        else {
            self.status = Some("Could not find that agent.".to_string());
            return false;
        };
        let Some(store) = self.store.as_ref() else {
            self.status = Some("Open a project before editing Agents.".to_string());
            return false;
        };

        match store.update(&storage::AgentDefinitionUpdate {
            id: definition.id,
            name: definition.name.clone(),
            runtime_profile: definition.runtime_profile,
            workflow_description: definition.workflow_description.clone(),
            enabled: !definition.enabled,
            sort_order: definition.sort_order,
        }) {
            Ok(updated) => {
                if self.draft.id == Some(updated.id) {
                    self.draft.enabled = updated.enabled;
                }
                self.reload();
                true
            }
            Err(error) => {
                self.status = Some(format!("Could not update project agent: {error}"));
                false
            }
        }
    }

    pub(crate) fn clear_status(&mut self) {
        self.status = None;
    }

    pub(crate) fn validate_draft(&self) -> Result<(), String> {
        validate_agent_definition_text(&self.draft.name, &self.draft.workflow_description)
    }

    pub(crate) fn begin_edit(&mut self, field: AgentDefinitionField) {
        self.edit_buffer = self.field_value(field).to_string();
        self.editing = Some(field);
        self.status = None;
    }

    pub(crate) fn handle_key_down(&mut self, keystroke: &Keystroke) -> bool {
        let Some(field) = self.editing else {
            return false;
        };

        match keystroke.key.as_ref() {
            "escape" => {
                self.cancel_edit();
                true
            }
            "enter" => {
                self.commit_edit(field);
                true
            }
            "backspace" => {
                self.edit_buffer.pop();
                true
            }
            "space" if no_command_modifiers(keystroke) => {
                self.edit_buffer.push(' ');
                true
            }
            _ if no_command_modifiers(keystroke) => {
                if let Some(text) = keystroke.key_char.as_deref() {
                    self.edit_buffer.push_str(text);
                }
                true
            }
            _ => true,
        }
    }

    pub(super) fn is_editing(&self, field: AgentDefinitionField) -> bool {
        self.editing == Some(field)
    }

    pub(super) fn row_value(&self, field: AgentDefinitionField) -> String {
        let value = if self.editing == Some(field) {
            format!("{}|", self.edit_buffer)
        } else {
            self.field_value(field).to_string()
        };
        display_value(field, &value)
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

    fn cancel_edit(&mut self) {
        self.editing = None;
        self.edit_buffer.clear();
        self.status = None;
    }

    fn commit_pending_edit(&mut self) {
        if let Some(field) = self.editing {
            self.commit_edit(field);
        }
    }

    fn commit_edit(&mut self, field: AgentDefinitionField) {
        match field {
            AgentDefinitionField::Name => self.draft.name = self.edit_buffer.clone(),
            AgentDefinitionField::WorkflowDescription => {
                self.draft.workflow_description = self.edit_buffer.clone();
            }
        }
        self.editing = None;
        self.edit_buffer.clear();
        self.status = None;
    }

    fn field_value(&self, field: AgentDefinitionField) -> &str {
        match field {
            AgentDefinitionField::Name => &self.draft.name,
            AgentDefinitionField::WorkflowDescription => &self.draft.workflow_description,
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

fn display_value(field: AgentDefinitionField, value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        match field {
            AgentDefinitionField::Name => "Set the agent name.".to_string(),
            AgentDefinitionField::WorkflowDescription => {
                "Describe the workflow this agent should follow.".to_string()
            }
        }
    } else {
        value.to_string()
    }
}

fn readable_agent_store_error(error: &storage::StorageError) -> String {
    let message = error.to_string();
    if message.contains("idx_agent_definitions_name_nocase")
        || message.contains("UNIQUE constraint failed")
    {
        "Agent name already exists in this project".to_string()
    } else {
        format!("Could not save project agent: {error}")
    }
}

fn no_command_modifiers(keystroke: &Keystroke) -> bool {
    !keystroke.modifiers.platform && !keystroke.modifiers.control && !keystroke.modifiers.alt
}
