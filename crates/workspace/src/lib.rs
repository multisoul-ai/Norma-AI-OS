//! workspace — zero-GPUI workspace metadata and recent-project persistence.

pub mod content;
pub mod git_clone;
pub mod ideas;
pub mod path_display;
pub mod project;
pub mod recent;
pub mod review;

pub use path_display::{compact_path, display_name_for_path};
pub use recent::{RecentWorkspaces, WorkspaceRecord, WorkspaceSource, MAX_RECENT_WORKSPACES};

#[cfg(windows)]
pub mod scoped_directory;
