//! UI-free installation of original media pinned by the signed app catalog.

mod artifact;
mod catalog;
mod installation;
mod transfers;

pub use artifact::MediaArtifact;
pub use catalog::MediaCatalog;
pub use installation::MediaStore;
pub use transfers::{MediaAvailability, MediaTransfers, TransferId};

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("invalid media artifact: {0}")]
    InvalidArtifact(String),
    #[error("media cache path is not a regular file or directory")]
    UnsafeCachePath,
    #[error("media installation was cancelled")]
    Cancelled,
    #[error("media length mismatch: expected {expected}, received {actual}")]
    LengthMismatch { expected: u64, actual: u64 },
    #[error("media SHA-256 does not match the trusted catalog")]
    HashMismatch,
    #[error("media I/O failed: {0}")]
    Io(#[from] std::io::Error),
}
