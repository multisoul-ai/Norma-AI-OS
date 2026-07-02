//! Project-scoped Agent catalog settings state.

mod actions;
mod state;
mod view;

pub(crate) use state::{
    AgentDefinitionDraft, AgentDefinitionField, AgentDefinitionsState, AgentVoiceContext,
};
pub(crate) use view::agents_settings;

#[cfg(test)]
#[path = "../../tests/unit/agent_settings_tests.rs"]
mod tests;
