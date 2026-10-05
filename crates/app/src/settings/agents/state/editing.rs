use super::*;

impl AgentDefinitionsState {
    pub(crate) fn begin_edit(&mut self, field: AgentDefinitionField) {
        if self.editing.is_some_and(|editing| editing != field) {
            self.commit_pending_edit();
        }
        self.edit_buffer = self.field_value(field).to_string();
        self.editing = Some(field);
        self.status = None;
        self.clear_delete_dialog();
    }

    pub(super) fn cancel_edit(&mut self) {
        self.editing = None;
        self.edit_buffer.clear();
        self.status = None;
    }

    pub(super) fn commit_pending_edit(&mut self) {
        if let Some(field) = self.editing {
            self.commit_edit(field);
        }
    }

    pub(super) fn commit_edit(&mut self, field: AgentDefinitionField) {
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

    pub(super) fn field_value(&self, field: AgentDefinitionField) -> &str {
        match field {
            AgentDefinitionField::Name => &self.draft.name,
            AgentDefinitionField::WorkflowDescription => &self.draft.workflow_description,
        }
    }
}
