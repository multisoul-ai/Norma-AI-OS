use super::support::{draft_matches_definition, next_default_agent_name};
use super::{AgentDefinitionDraft, AgentDefinitionField, AgentDefinitionsState, AgentDeleteDialog};
use canvas::AgentProfile;

impl AgentDefinitionsState {
    #[cfg(test)]
    pub(crate) fn begin_new_definition(&mut self) -> bool {
        if !self.ensure_clean_for_navigation() {
            return false;
        }
        self.prepare_new_draft();
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

    pub(crate) fn cancel_card_edit(&mut self) -> bool {
        if self.creating_new {
            let previous_selection = self.selection_before_create;
            self.reset_draft();
            self.select_after_reload(previous_selection);
            return true;
        }
        let did_revert = self.revert_draft();
        did_revert
    }

    pub(crate) fn create_new_card(&mut self) -> bool {
        if !self.ensure_clean_for_navigation() {
            return false;
        }
        if self.store.is_none() {
            self.status = Some("Open a project before editing Agents.".to_string());
            return false;
        }
        self.prepare_new_draft();
        !self.draft.name.is_empty()
    }

    pub(crate) fn revert_draft(&mut self) -> bool {
        self.cancel_edit();
        match self.selected_definition_id {
            Some(id) => {
                if self.load_draft_from_definition(id) {
                    self.status = None;
                    true
                } else {
                    self.status = Some("Could not revert that agent.".to_string());
                    false
                }
            }
            None => {
                self.reset_draft();
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

    pub(crate) fn open_delete_dialog(&mut self, id: i64, blocked: bool) -> bool {
        if self.is_dirty() {
            self.status = Some("Save or discard changes before deleting this Agent.".to_string());
            return false;
        }
        let Some(definition) = self
            .definitions
            .iter()
            .find(|definition| definition.id == id)
        else {
            self.status = Some("Could not find that Agent.".to_string());
            return false;
        };
        self.delete_dialog = Some(if blocked {
            AgentDeleteDialog::Blocked {
                id,
                name: definition.name.clone(),
            }
        } else {
            AgentDeleteDialog::Confirm {
                id,
                name: definition.name.clone(),
            }
        });
        self.status = None;
        true
    }

    pub(crate) fn delete_dialog(&self) -> Option<&AgentDeleteDialog> {
        self.delete_dialog.as_ref()
    }

    pub(crate) fn close_delete_dialog(&mut self) {
        self.delete_dialog = None;
    }

    pub(crate) fn confirm_delete_from_dialog(&mut self, id: i64) -> bool {
        if !matches!(
            self.delete_dialog,
            Some(AgentDeleteDialog::Confirm {
                id: pending_id,
                ..
            }) if pending_id == id
        ) {
            self.status = Some("Delete confirmation is no longer active.".to_string());
            return false;
        }
        self.delete_dialog = None;
        self.delete_definition(id)
    }

    pub(crate) fn prepare_create_card_from_definition(&mut self, id: i64) -> bool {
        if self.is_dirty() {
            self.status =
                Some("Save or discard the current Agent before creating a card.".to_string());
            return false;
        }
        self.select_definition(id)
    }

    pub(super) fn ensure_clean_for_navigation(&mut self) -> bool {
        if self.is_dirty() {
            self.status = Some("Save or discard the current Agent before switching.".to_string());
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

    pub(super) fn clear_delete_dialog(&mut self) {
        self.delete_dialog = None;
    }

    fn prepare_new_draft(&mut self) {
        let name = next_default_agent_name(&self.definitions);
        let previous_selection = self.selected_definition_id;
        self.reset_draft();
        if name.is_empty() {
            self.status =
                Some("All 30 built-in Agent names are in use; enter a custom name.".into());
            return;
        }
        self.draft = AgentDefinitionDraft {
            id: None,
            name,
            runtime_profile: AgentProfile::Codex,
            workflow_description: String::new(),
            enabled: true,
            sort_order: 0,
        };
        self.creating_new = true;
        self.selected_definition_id = None;
        self.selection_before_create = previous_selection;
        self.status = None;
    }
}
