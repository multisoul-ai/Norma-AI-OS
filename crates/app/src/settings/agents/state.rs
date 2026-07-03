mod selection;
mod support;

use self::support::{
    display_value, no_command_modifiers, readable_agent_store_error, validate_agent_definition_text,
};
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

#[cfg(test)]
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
    selected_definition_id: Option<i64>,
    editing_card_id: Option<i64>,
    delete_confirmation_id: Option<i64>,
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
            selected_definition_id: None,
            editing_card_id: None,
            delete_confirmation_id: None,
            editing: None,
            edit_buffer: String::new(),
            status: None,
        }
    }
}

impl AgentDefinitionsState {
    pub(crate) fn bind_project_store(&mut self, store: AgentDefinitionStore) {
        self.store = Some(store);
        self.reset_draft();
        self.reload();
        self.select_after_reload(None);
    }

    pub(crate) fn definitions(&self) -> &[AgentDefinition] {
        &self.definitions
    }

    pub(crate) fn draft(&self) -> &AgentDefinitionDraft {
        &self.draft
    }

    pub(crate) fn selected_definition_id(&self) -> Option<i64> {
        self.selected_definition_id
    }

    pub(crate) fn editing_card_id(&self) -> Option<i64> {
        self.editing_card_id
    }

    pub(crate) fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub(crate) fn has_project_store(&self) -> bool {
        self.store.is_some()
    }

    #[cfg(test)]
    pub(crate) fn set_draft_name(&mut self, value: impl Into<String>) {
        self.draft.name = value.into();
        self.clear_delete_confirmation();
    }

    #[cfg(test)]
    pub(crate) fn set_draft_workflow_description(&mut self, value: impl Into<String>) {
        self.draft.workflow_description = value.into();
        self.clear_delete_confirmation();
    }

    pub(crate) fn set_draft_runtime_profile(&mut self, profile: AgentProfile) {
        self.draft.runtime_profile = profile;
        self.clear_delete_confirmation();
    }

    pub(crate) fn toggle_draft_enabled(&mut self) {
        self.draft.enabled = !self.draft.enabled;
        self.clear_delete_confirmation();
    }

    pub(crate) fn reset_draft(&mut self) {
        self.draft = AgentDefinitionDraft::new();
        self.selected_definition_id = None;
        self.editing_card_id = None;
        self.delete_confirmation_id = None;
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
        self.selected_definition_id = Some(definition.id);
        self.editing_card_id = None;
        self.delete_confirmation_id = None;
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
                .map(|updated| updated.id),
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
                        .map(|updated| updated.id)
                }
                Ok(created) => Ok(created.id),
                Err(error) => Err(error),
            },
        };

        match result {
            Ok(saved_id) => {
                self.editing_card_id = None;
                self.editing = None;
                self.edit_buffer.clear();
                self.reload();
                self.select_after_reload(Some(saved_id));
                true
            }
            Err(error) => {
                self.status = Some(readable_agent_store_error(&error));
                false
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn edit_definition(&mut self, id: i64) -> bool {
        self.select_definition(id)
    }

    pub(crate) fn delete_definition(&mut self, id: i64) -> bool {
        let Some(store) = self.store.as_ref() else {
            self.status = Some("Open a project before editing Agents.".to_string());
            return false;
        };

        match store.delete(id) {
            Ok(true) => {
                let fallback_id = self.next_selection_after_delete(id);
                self.reload();
                self.select_after_reload(fallback_id);
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

    #[cfg(test)]
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
                self.select_after_reload(self.selected_definition_id);
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

    #[cfg(test)]
    pub(crate) fn validate_draft(&self) -> Result<(), String> {
        validate_agent_definition_text(&self.draft.name, &self.draft.workflow_description)
    }

    pub(crate) fn begin_edit(&mut self, field: AgentDefinitionField) {
        self.edit_buffer = self.field_value(field).to_string();
        self.editing = Some(field);
        self.status = None;
        self.clear_delete_confirmation();
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
                self.clear_delete_confirmation();
                true
            }
            "space" if no_command_modifiers(keystroke) => {
                self.edit_buffer.push(' ');
                self.clear_delete_confirmation();
                true
            }
            _ if no_command_modifiers(keystroke) => {
                if let Some(text) = keystroke.key_char.as_deref() {
                    self.edit_buffer.push_str(text);
                    self.clear_delete_confirmation();
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

    #[cfg(test)]
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
