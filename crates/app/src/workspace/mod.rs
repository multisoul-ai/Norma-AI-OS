pub(crate) mod clone_project;
pub mod desktop;
pub(crate) mod directory;
#[cfg(test)]
#[path = "../../tests/unit/workspace_directory_projection_tests.rs"]
mod directory_projection_tests;
pub mod dispatch;
#[cfg(test)]
#[path = "../../tests/unit/workspace_dispatch_service_tests.rs"]
mod dispatch_service_tests;
pub mod flow;
pub(crate) mod legacy_agent_migration;
mod norma_route;
pub mod persistence;
pub(crate) mod project_picker;
pub(crate) mod sessions;
pub mod state;
pub mod windowing;

pub(crate) mod roles;

#[cfg(test)]
#[path = "../../tests/unit/workspace_dispatch_cold_tests.rs"]
mod dispatch_cold_tests;

#[cfg(test)]
#[path = "../../tests/support/workspace_dispatch_app.rs"]
pub(crate) mod dispatch_test_support;

#[cfg(test)]
#[path = "../../tests/unit/workspace_dispatch_journal_tests.rs"]
mod dispatch_journal_tests;

#[cfg(test)]
#[path = "../../tests/unit/workspace_dispatch_cancellation_tests.rs"]
mod dispatch_cancellation_tests;
