//! Project-scoped Agent catalog settings state.

mod actions;
mod state;
mod view;

pub(crate) use state::{AgentDefinitionField, AgentDefinitionsState};
#[cfg(test)]
pub(crate) use state::{AgentDefinitionDraft, AgentVoiceContext};
pub(crate) use view::agents_settings;

#[cfg(test)]
#[path = "../../tests/unit/agent_settings_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "../../tests/unit/agent_settings_actions_tests.rs"]
mod action_tests;
