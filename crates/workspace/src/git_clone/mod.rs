mod destination;
mod progress;
mod remote;
mod runner;
mod security;

pub use destination::{validate_clone_destination, CloneDestination, CloneDestinationError};
pub use progress::{parse_git_progress_line, CloneProgress, CloneStage};
pub use remote::{CloneTransport, GitHubRemote, GitHubRemoteParseError};
#[cfg(test)]
use runner::classify_git_failure;
pub use runner::{
    CloneCancellation, CloneEvent, CloneFailure, CloneFailureKind, GitCloneRequest, GitCloneRunner,
};
pub use security::{secure_cloned_workspace, ClonedWorkspaceSecurityError};

#[cfg(test)]
#[path = "../../tests/unit/git_clone_remote_tests.rs"]
mod remote_tests;

#[cfg(test)]
#[path = "../../tests/unit/git_clone_destination_tests.rs"]
mod destination_tests;

#[cfg(test)]
#[path = "../../tests/unit/git_clone_progress_tests.rs"]
mod progress_tests;

#[cfg(test)]
#[path = "../../tests/unit/git_clone_security_tests.rs"]
mod security_tests;

#[cfg(test)]
#[path = "../../tests/unit/git_clone_runner_tests.rs"]
mod runner_tests;

#[cfg(test)]
#[path = "../../tests/unit/git_clone_cleanup_tests.rs"]
mod cleanup_tests;

#[cfg(test)]
#[path = "../../tests/unit/git_clone_failure_tests.rs"]
mod failure_tests;
