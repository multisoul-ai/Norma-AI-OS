//! Semantic workbench failures. Product copy belongs to the presentation layer.
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentError {
    ProjectMissing,
    PathOutsideProject,
    GitInternalPath,
    LinkOutsideProject,
    BareRepository,
    WorktreeRootRequired,
    SearchLimit,
    NotRegularFile,
    FileTooLarge,
    BinaryFile,
    InvalidUtf8,
    TextTooLarge,
    ReadOnly,
    ParentMissing,
    FileChanged,
    InvalidBranch,
    InvalidFilename,
    InvalidPath,
    DiffTooLarge,
    BinaryDiff,
    RepositoryChanged,
    RepositoryBusy,
    CommitMessageRequired,
    NothingStaged,
    InvalidCommitPath,
    FilesRequired,
    FileStatusChanged,
    DiscardTrackedOnly,
    RootMissing,
    GitOutputLimit,
    GitTimeout,
    AtomicUnsupported,
    GitFailed(String),
    SaveConflictRestored { recovery: PathBuf },
    SaveConflictRecovery { recovery: PathBuf, detail: String },
    Diagnostic(String),
}

impl From<String> for ContentError {
    fn from(value: String) -> Self {
        Self::Diagnostic(value)
    }
}

impl std::fmt::Display for ContentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Diagnostic(detail) | Self::GitFailed(detail) => f.write_str(detail),
            _ => write!(f, "{self:?}"),
        }
    }
}

impl std::error::Error for ContentError {}
