use super::support::{draft_matches_definition, next_default_agent_name, DEFAULT_AGENT_WORKFLOW};
use super::{AgentDefinitionDraft, AgentDefinitionField, AgentDefinitionsState};
use canvas::AgentProfile;

impl AgentDefinitionsState {
    #[cfg(test)]
    pub(crate) fn begin_new_definition(&mut self) -> bool {
        if !self.ensure_clean_for_navigation() {
            return false;
        }
        self.reset_draft();
        self.status = None;
        true
    }

    pub(crate) fn select_definition(&mut self, id: i64) -> bool {
        if self.selected_definition_id == Some(id) {
            self.status = None;
            return true;
        }
        if !self.ensure_clean_for_navigation() {
            return false;
        }
        if self.load_draft_from_definition(id) {
            self.status = None;
            true
        } else {
            self.status = Some("Could not load that agent.".to_string());
            false
        }
    }

    pub(crate) fn begin_card_edit(&mut self, id: i64) -> bool {
        if self.selected_definition_id != Some(id) && !self.ensure_clean_for_navigation() {
            return false;
        }
        if self.selected_definition_id != Some(id) && !self.load_draft_from_definition(id) {
            self.status = Some("Could not load that agent.".to_string());
            return false;
        }
        self.editing_card_id = Some(id);
        self.status = None;
        true
    }

    pub(crate) fn cancel_card_edit(&mut self) -> bool {
        let did_revert = self.revert_draft();
        self.editing_card_id = None;
        did_revert
    }

    pub(crate) fn create_new_card(&mut self) -> bool {
        if !self.ensure_clean_for_navigation() {
            return false;
        }
        let Some(store) = self.store.as_ref() else {
            self.status = Some("Open a project before editing Agents.".to_string());
            return false;
        };

        let name = next_default_agent_name(&self.definitions);
        match store.create(&storage::NewAgentDefinition {
            name,
            runtime_profile: AgentProfile::Codex,
            workflow_description: DEFAULT_AGENT_WORKFLOW.to_string(),
        }) {
            Ok(created) => {
                let created_id = created.id;
                self.reload();
                self.select_after_reload(Some(created_id));
                self.editing_card_id = Some(created_id);
                self.status = None;
                true
            }
            Err(error) => {
                self.status = Some(format!("Could not create project agent: {error}"));
                false
            }
        }
    }

    pub(crate) fn revert_draft(&mut self) -> bool {
        self.cancel_edit();
        match self.selected_definition_id {
            Some(id) => {
                if self.load_draft_from_definition(id) {
                    self.editing_card_id = None;
                    self.status = None;
                    true
                } else {
                    self.status = Some("Could not revert that agent.".to_string());
                    false
                }
            }
            None => {
                self.reset_draft();
                self.editing_card_id = None;
                self.status = None;
                true
            }
        }
    }

    pub(crate) fn is_dirty(&self) -> bool {
        let draft = self.draft_with_pending_edit();
        match draft.id {
            Some(id) => self
                .definitions
                .iter()
                .find(|definition| definition.id == id)
                .is_none_or(|definition| !draft_matches_definition(&draft, definition)),
            None => draft != AgentDefinitionDraft::new(),
        }
    }

    pub(crate) fn request_delete_definition(&mut self, id: i64) -> bool {
        if self.is_dirty() {
            self.status = Some("Save or revert the current Agent before deleting.".to_string());
            return false;
        }
        if self.delete_confirmation_id != Some(id) {
            self.delete_confirmation_id = Some(id);
            self.status = Some("Click Delete again to remove this Agent.".to_string());
            return false;
        }
        self.delete_confirmation_id = None;
        self.delete_definition(id)
    }

    pub(crate) fn prepare_create_card_from_definition(&mut self, id: i64) -> bool {
        if self.is_dirty() {
            self.status =
                Some("Save or revert the current Agent before creating a card.".to_string());
            return false;
        }
        self.select_definition(id)
    }

    pub(crate) fn is_delete_confirmation_pending(&self, id: i64) -> bool {
        self.delete_confirmation_id == Some(id)
    }

    pub(super) fn ensure_clean_for_navigation(&mut self) -> bool {
        if self.is_dirty() {
            self.status = Some("Save or revert the current Agent before switching.".to_string());
            false
        } else {
            true
        }
    }

    pub(super) fn select_after_reload(&mut self, preferred_id: Option<i64>) {
        let selected_id = preferred_id
            .filter(|id| {
                self.definitions
                    .iter()
                    .any(|definition| definition.id == *id)
            })
            .or_else(|| self.definitions.first().map(|definition| definition.id));

        if let Some(id) = selected_id {
            let _ = self.load_draft_from_definition(id);
        } else {
            self.reset_draft();
        }
        self.status = None;
    }

    pub(super) fn next_selection_after_delete(&self, deleted_id: i64) -> Option<i64> {
        let Some(index) = self
            .definitions
            .iter()
            .position(|definition| definition.id == deleted_id)
        else {
            return self.selected_definition_id;
        };
        self.definitions
            .iter()
            .skip(index + 1)
            .find(|definition| definition.id != deleted_id)
            .or_else(|| {
                self.definitions
                    .iter()
                    .take(index)
                    .rev()
                    .find(|definition| definition.id != deleted_id)
            })
            .map(|definition| definition.id)
    }

    pub(super) fn draft_with_pending_edit(&self) -> AgentDefinitionDraft {
        let mut draft = self.draft.clone();
        if let Some(field) = self.editing {
            match field {
                AgentDefinitionField::Name => draft.name = self.edit_buffer.clone(),
                AgentDefinitionField::WorkflowDescription => {
                    draft.workflow_description = self.edit_buffer.clone();
                }
            }
        }
        draft
    }

    pub(super) fn clear_delete_confirmation(&mut self) {
        self.delete_confirmation_id = None;
    }
}
