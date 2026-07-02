//! Project-scoped Agent catalog settings state.

mod actions;
mod components;
mod state;
mod view;

#[cfg(test)]
pub(crate) use state::{AgentDefinitionDraft, AgentVoiceContext};
pub(crate) use state::{AgentDefinitionField, AgentDefinitionsState};
pub(crate) use view::agents_settings;

#[cfg(test)]
#[path = "../../tests/unit/agent_settings_actions_tests.rs"]
mod action_tests;
#[cfg(test)]
#[path = "../../tests/unit/agent_settings_pending_edit_tests.rs"]
mod pending_edit_tests;
#[cfg(test)]
#[path = "../../tests/unit/agent_settings_store_binding_tests.rs"]
mod store_binding_tests;
#[cfg(test)]
#[path = "../../tests/unit/agent_settings_tests.rs"]
mod tests;
