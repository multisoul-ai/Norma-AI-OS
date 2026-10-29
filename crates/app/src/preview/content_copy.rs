//! Translate workspace semantics without rewriting paths or raw diagnostics.
use ui::i18n::{text, LocalizedText};
use workspace::content::{ChangeStatus, ContentError};

pub(crate) fn content_error(error: ContentError) -> LocalizedText {
    let key = match error {
        ContentError::ProjectMissing => "drawers.content_project_missing",
        ContentError::PathOutsideProject => "drawers.content_path_outside_project",
        ContentError::GitInternalPath => "drawers.content_git_internal_path",
        ContentError::LinkOutsideProject => "drawers.content_link_outside_project",
        ContentError::BareRepository => "drawers.content_bare_repository",
        ContentError::WorktreeRootRequired => "drawers.content_worktree_root_required",
        ContentError::SearchLimit => "drawers.content_search_limit",
        ContentError::NotRegularFile => "drawers.content_not_regular_file",
        ContentError::FileTooLarge => "drawers.content_file_too_large",
        ContentError::BinaryFile => "drawers.content_binary_file",
        ContentError::InvalidUtf8 => "drawers.content_invalid_utf8",
        ContentError::TextTooLarge => "drawers.content_text_too_large",
        ContentError::ReadOnly => "drawers.content_read_only",
        ContentError::ParentMissing => "drawers.content_parent_missing",
        ContentError::FileChanged => "drawers.content_file_changed",
        ContentError::InvalidBranch => "drawers.content_invalid_branch",
        ContentError::InvalidFilename => "drawers.content_invalid_filename",
        ContentError::InvalidPath => "drawers.content_invalid_path",
        ContentError::DiffTooLarge => "drawers.content_diff_too_large",
        ContentError::BinaryDiff => "drawers.content_binary_diff",
        ContentError::RepositoryChanged => "drawers.content_repository_changed",
        ContentError::RepositoryBusy => "drawers.content_repository_busy",
        ContentError::CommitMessageRequired => "drawers.content_commit_message_required",
        ContentError::NothingStaged => "drawers.content_nothing_staged",
        ContentError::InvalidCommitPath => "drawers.content_invalid_commit_path",
        ContentError::FilesRequired => "drawers.content_files_required",
        ContentError::FileStatusChanged => "drawers.content_file_status_changed",
        ContentError::DiscardTrackedOnly => "drawers.content_discard_tracked_only",
        ContentError::RootMissing => "drawers.content_root_missing",
        ContentError::GitOutputLimit => "drawers.content_git_output_limit",
        ContentError::GitTimeout => "drawers.content_git_timeout",
        ContentError::AtomicUnsupported => "drawers.content_atomic_unsupported",
        ContentError::GitFailed(detail) => {
            return LocalizedText::new("drawers.content_git_failed").with_arg("error", detail)
        }
        ContentError::SaveConflictRestored { recovery } => {
            return LocalizedText::new("drawers.content_save_restored")
                .with_arg("path", recovery.display())
        }
        ContentError::SaveConflictRecovery { recovery, detail } => {
            return LocalizedText::new("drawers.content_save_recovery")
                .with_arg("path", recovery.display())
                .with_arg("error", detail)
        }
        ContentError::Diagnostic(detail) => return detail.into(),
    };
    LocalizedText::new(key)
}

pub(crate) fn change_status(status: ChangeStatus) -> &'static str {
    text(match status {
        ChangeStatus::Unchanged => "drawers.content_unchanged",
        ChangeStatus::Added => "drawers.content_added",
        ChangeStatus::Deleted => "drawers.content_deleted",
        ChangeStatus::Modified => "drawers.content_modified",
        ChangeStatus::Renamed => "drawers.content_renamed",
        ChangeStatus::Copied => "drawers.content_copied",
        ChangeStatus::Ignored => "drawers.content_ignored",
        ChangeStatus::TypeChanged => "drawers.content_type_changed",
        ChangeStatus::Unreadable => "drawers.content_unreadable",
        ChangeStatus::Conflicted => "drawers.content_conflicted",
    })
}

#[cfg(test)]
#[path = "../../tests/unit/content_i18n_tests.rs"]
mod tests;
