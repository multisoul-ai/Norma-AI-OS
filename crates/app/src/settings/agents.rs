//! Project-scoped Agent catalog settings state.

mod actions;
mod components;
mod grouping;
mod state;
mod text_input;
mod view;

#[cfg(test)]
pub(crate) use state::{AgentDefinitionDraft, AgentVoiceContext};
pub(crate) use state::{
    AgentDefinitionField, AgentDefinitionsState, AgentDeleteDialog, AgentStatusFilter,
    MAX_WORKFLOW_DESCRIPTION_CHARS,
};
pub(crate) use view::agents_settings;

#[cfg(test)]
#[path = "../../tests/unit/agent_settings_actions_tests.rs"]
mod action_tests;
#[cfg(test)]
#[path = "../../tests/unit/agent_settings_create_cancel_tests.rs"]
mod create_cancel_tests;
#[cfg(test)]
#[path = "../../tests/unit/agent_settings_drawer_tests.rs"]
mod drawer_tests;
#[cfg(test)]
#[path = "../../tests/unit/agent_settings_pending_edit_tests.rs"]
mod pending_edit_tests;
#[cfg(test)]
#[path = "../../tests/unit/agent_settings_search_tests.rs"]
mod search_tests;
#[cfg(test)]
#[path = "../../tests/unit/agent_settings_selection_tests.rs"]
mod selection_tests;
#[cfg(test)]
#[path = "../../tests/unit/agent_settings_store_binding_tests.rs"]
mod store_binding_tests;
#[cfg(test)]
#[path = "../../tests/unit/agent_settings_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "../../tests/unit/agent_settings_view_contract_tests.rs"]
mod view_contract_tests;
