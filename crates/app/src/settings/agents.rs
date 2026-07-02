//! Project-scoped Agent catalog settings state.

mod state;

pub(crate) use state::AgentDefinitionsState;

#[cfg(test)]
#[path = "../../tests/unit/agent_settings_tests.rs"]
mod tests;
