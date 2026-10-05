mod editing;
mod lifecycle;
mod search;
mod selection;
mod support;

pub(crate) use self::search::AgentStatusFilter;

#[cfg(test)]
use self::support::no_command_modifiers;
use self::support::{display_value, readable_agent_store_error, validate_agent_definition_text};
use canvas::AgentProfile;
#[cfg(test)]
use gpui::Keystroke;
use storage::{AgentDefinition, AgentDefinitionStore};

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
pub(crate) enum AgentDeleteDialog {
    Confirm { id: i64, name: String },
    Blocked { id: i64, name: String },
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
    selection_before_create: Option<i64>,
    creating_new: bool,
    delete_dialog: Option<AgentDeleteDialog>,
    editing: Option<AgentDefinitionField>,
    edit_buffer: String,
    search_query: String,
    status_filter: AgentStatusFilter,
    status: Option<ui::i18n::LocalizedText>,
}

impl Default for AgentDefinitionsState {
    fn default() -> Self {
        Self {
            definitions: Vec::new(),
            store: None,
            draft: AgentDefinitionDraft::new(),
            selected_definition_id: None,
            selection_before_create: None,
            creating_new: false,
            delete_dialog: None,
            editing: None,
            edit_buffer: String::new(),
            search_query: String::new(),
            status_filter: AgentStatusFilter::All,
            status: None,
        }
    }
}

impl AgentDefinitionsState {
    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn use_queue_visual_preview(&mut self) -> AgentDefinition {
        let definition = AgentDefinition {
            id: 1,
            name: "Adam".to_string(),
            speech_alias: None,
            runtime_profile: AgentProfile::Codex,
            workflow_description: "负责工程实现与验证。".to_string(),
            enabled: true,
            sort_order: 0,
            origin: storage::AgentDefinitionOrigin::Explicit,
        };
        self.definitions = vec![definition.clone()];
        self.store = None;
        definition
    }

    pub(crate) fn bind_project_store(&mut self, store: AgentDefinitionStore) {
        self.store = Some(store);
        self.reset_draft();
        self.reload();
        self.select_after_reload(None);
    }

    pub(crate) fn definitions(&self) -> &[AgentDefinition] {
        &self.definitions
    }

    pub(crate) fn definition_by_address(&self, address: &str) -> Option<&AgentDefinition> {
        let normalized = agent::AgentName::parse(address.trim().trim_matches(|character: char| {
            matches!(character, ',' | '，' | ':' | '：' | '。' | '!' | '！')
        }))
        .ok()?;
        self.definitions.iter().find(|definition| {
            if agent::AgentName::parse(&definition.name)
                .is_ok_and(|name| name.match_key() == normalized.match_key())
            {
                return true;
            }
            definition.speech_alias.as_deref() == Some(normalized.match_key())
        })
    }

    pub(crate) fn draft(&self) -> &AgentDefinitionDraft {
        &self.draft
    }

    pub(crate) fn selected_definition_id(&self) -> Option<i64> {
        self.selected_definition_id
    }

    pub(crate) fn is_creating_new(&self) -> bool {
        self.creating_new
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
        self.clear_delete_dialog();
    }

    #[cfg(test)]
    pub(crate) fn set_draft_workflow_description(&mut self, value: impl Into<String>) {
        self.draft.workflow_description = value.into();
        self.clear_delete_dialog();
    }

    pub(crate) fn set_draft_runtime_profile(&mut self, profile: AgentProfile) {
        self.draft.runtime_profile = profile;
        self.clear_delete_dialog();
    }

    pub(crate) fn toggle_draft_enabled(&mut self) {
        self.draft.enabled = !self.draft.enabled;
        self.clear_delete_dialog();
    }

    pub(crate) fn reset_draft(&mut self) {
        self.draft = AgentDefinitionDraft::new();
        self.selected_definition_id = None;
        self.selection_before_create = None;
        self.creating_new = false;
        self.delete_dialog = None;
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
        self.selection_before_create = None;
        self.creating_new = false;
        self.delete_dialog = None;
        self.editing = None;
        self.edit_buffer.clear();
        true
    }

    pub(crate) fn save_draft(&mut self) -> bool {
        self.commit_pending_edit();

        if let Err(message) =
            validate_agent_definition_text(&self.draft.name, &self.draft.workflow_description)
        {
            self.status = Some(message.into());
            return false;
        }
        let Some(store) = self.store.as_ref() else {
            self.status = Some(ui::i18n::LocalizedText::new(
                "settings.open_a_project_before_editing_agents",
            ));
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
                self.creating_new = false;
                self.editing = None;
                self.edit_buffer.clear();
                self.reload();
                self.select_after_reload(Some(saved_id));
                true
            }
            Err(error) => {
                self.status = Some(readable_agent_store_error(&error).into());
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
            self.status = Some(ui::i18n::LocalizedText::new(
                "settings.open_a_project_before_editing_agents",
            ));
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
                self.status = Some(ui::i18n::LocalizedText::new(
                    "settings.could_not_delete_that_agent",
                ));
                false
            }
            Err(error) => {
                self.status = Some(ui::i18n::message(
                    "settings.could_not_delete_project_agent_error",
                    &[("error", (error).to_string())],
                ));
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
            self.status = Some(ui::i18n::LocalizedText::new(
                "settings.could_not_find_that_agent",
            ));
            return false;
        };
        let Some(store) = self.store.as_ref() else {
            self.status = Some(ui::i18n::LocalizedText::new(
                "settings.open_a_project_before_editing_agents",
            ));
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
                self.status = Some(ui::i18n::message(
                    "settings.could_not_update_project_agent_error",
                    &[("error", (error).to_string())],
                ));
                false
            }
        }
    }

    pub(crate) fn clear_status(&mut self) {
        self.status = None;
    }

    pub(crate) fn set_status(&mut self, message: impl Into<ui::i18n::LocalizedText>) {
        self.status = Some(message.into());
    }

    #[cfg(test)]
    pub(crate) fn validate_draft(&self) -> Result<(), String> {
        validate_agent_definition_text(&self.draft.name, &self.draft.workflow_description)
            .map_err(|message| message.to_string())
    }

    #[cfg(test)]
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
                self.clear_delete_dialog();
                true
            }
            "space" if no_command_modifiers(keystroke) => {
                self.edit_buffer.push(' ');
                self.clear_delete_dialog();
                true
            }
            _ if no_command_modifiers(keystroke) => {
                if let Some(text) = keystroke.key_char.as_deref() {
                    self.edit_buffer.push_str(text);
                    self.clear_delete_dialog();
                }
                true
            }
            _ => true,
        }
    }

    pub(super) fn is_editing(&self, field: AgentDefinitionField) -> bool {
        self.editing == Some(field)
    }

    pub(super) fn edit_buffer(&self) -> &str {
        &self.edit_buffer
    }

    pub(crate) fn replace_edit_buffer(
        &mut self,
        field: AgentDefinitionField,
        value: impl Into<String>,
    ) -> bool {
        if self.editing != Some(field) {
            return false;
        }
        self.edit_buffer = value.into();
        self.clear_delete_dialog();
        true
    }

    pub(crate) fn commit_active_edit(&mut self, field: AgentDefinitionField) -> bool {
        if self.editing != Some(field) {
            return false;
        }
        self.commit_edit(field);
        true
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

    pub(crate) fn reload(&mut self) {
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
                self.status = Some(ui::i18n::message(
                    "settings.could_not_load_project_agents_error",
                    &[("error", (error).to_string())],
                ));
            }
        }
    }
}
