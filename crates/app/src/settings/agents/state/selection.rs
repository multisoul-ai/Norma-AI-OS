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
            self.drawer_open = true;
            return true;
        }
        if !self.ensure_clean_for_navigation() {
            return false;
        }
        if self.load_draft_from_definition(id) {
            self.status = None;
            self.drawer_open = true;
            true
        } else {
            self.status = Some(ui::i18n::LocalizedText::new(
                "settings.could_not_load_that_agent",
            ));
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

        self.revert_draft()
    }

    pub(crate) fn create_new_card(&mut self) -> bool {
        if !self.ensure_clean_for_navigation() {
            return false;
        }
        if self.store.is_none() {
            self.status = Some(ui::i18n::LocalizedText::new(
                "settings.open_a_project_before_editing_agents",
            ));
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
                    self.status = Some(ui::i18n::LocalizedText::new(
                        "settings.could_not_revert_that_agent",
                    ));
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
            self.status = Some(ui::i18n::LocalizedText::new(
                "settings.save_or_discard_changes_before_deleting_this_agent",
            ));
            return false;
        }
        let Some(definition) = self
            .definitions
            .iter()
            .find(|definition| definition.id == id)
        else {
            self.status = Some(ui::i18n::LocalizedText::new(
                "settings.could_not_find_that_agent_2",
            ));
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
            self.status = Some(ui::i18n::LocalizedText::new(
                "settings.delete_confirmation_is_no_longer_active",
            ));
            return false;
        }
        self.delete_dialog = None;
        self.delete_definition(id)
    }

    pub(crate) fn prepare_create_card_from_definition(&mut self, id: i64) -> bool {
        if self.is_dirty() {
            self.status = Some(ui::i18n::LocalizedText::new(
                "settings.save_or_discard_the_current_agent_before_creating_a_card",
            ));
            return false;
        }
        self.select_definition(id)
    }

    pub(super) fn ensure_clean_for_navigation(&mut self) -> bool {
        if self.is_dirty() {
            self.status = Some(ui::i18n::LocalizedText::new(
                "settings.save_or_discard_the_current_agent_before_switching",
            ));
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
            self.status = Some(ui::i18n::LocalizedText::new(
                "settings.all_30_built_in_agent_names_are_in_use_enter_a",
            ));
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
        self.drawer_open = true;
        self.selected_definition_id = None;
        self.selection_before_create = previous_selection;
        self.status = None;
    }
}
