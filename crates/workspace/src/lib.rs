//! workspace — zero-GPUI workspace metadata and recent-project persistence.

pub mod path_display;
pub mod recent;
pub mod storage;

pub use path_display::{compact_path, display_name_for_path};
pub use recent::{RecentWorkspaces, WorkspaceRecord, WorkspaceSource, MAX_RECENT_WORKSPACES};
pub use storage::{default_recent_workspaces_path, RecentStorage, WorkspaceStorageError};
