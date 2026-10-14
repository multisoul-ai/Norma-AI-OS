//! workspace — zero-GPUI workspace metadata and recent-project persistence.

pub mod connector_credentials;
pub mod git_clone;
pub mod host_authority;
pub use host_authority::HostAuthority;
pub mod path_display;
pub mod project;
pub mod recent;
#[cfg(unix)]
pub mod review;

pub use path_display::{compact_path, display_name_for_path};
pub use recent::{RecentWorkspaces, WorkspaceRecord, WorkspaceSource, MAX_RECENT_WORKSPACES};
