use std::path::PathBuf;

pub type StorageResult<T> = Result<T, StorageError>;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("could not determine user home directory for Application Support")]
    MissingHomeDirectory,
    #[error("workspace path is not an existing directory: {0}")]
    InvalidWorkspacePath(PathBuf),
    #[error("invalid workspace source stored in sqlite: {0}")]
    InvalidWorkspaceSource(String),
    #[error("invalid session checkpoint kind stored in sqlite: {0}")]
    InvalidSessionKind(String),
    #[error("invalid agent profile stored in sqlite: {0}")]
    InvalidAgentProfile(String),
    #[error("invalid session terminal size stored in sqlite: {field}={value}")]
    InvalidSessionSize { field: &'static str, value: i64 },
    #[error("storage io failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage sqlite failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("storage json failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("system time is before unix epoch: {0}")]
    Time(#[from] std::time::SystemTimeError),
}
