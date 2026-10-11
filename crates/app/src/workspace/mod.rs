pub(crate) mod clone_project;
pub mod desktop;
pub(crate) mod directory;
#[cfg(test)]
#[path = "../../tests/unit/workspace_directory_projection_tests.rs"]
mod directory_projection_tests;
pub mod flow;
pub(crate) mod legacy_agent_migration;
mod norma_route;
pub mod persistence;
pub(crate) mod project_picker;
pub(crate) mod sessions;
pub mod state;
pub mod windowing;

pub(crate) mod roles;
