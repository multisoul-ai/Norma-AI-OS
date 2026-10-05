mod controller;
mod localization;
mod state;
mod view;

#[cfg(feature = "e2e-test-support")]
pub(crate) use state::CloneProjectInternalE2eSnapshot;
pub(crate) use state::CloneProjectUiState;
