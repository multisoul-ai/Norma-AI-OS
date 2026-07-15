//! Shared, UI-free installation primitives for curated local model packages.

mod installation;

/// Immutable archive metadata pinned by an application release catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelArtifact {
    url: String,
    sha256: String,
    archive_bytes: u64,
    required_files: Vec<String>,
}

impl ModelArtifact {
    pub fn new(
        url: impl Into<String>,
        sha256: impl Into<String>,
        archive_bytes: u64,
        required_files: Vec<&str>,
    ) -> Self {
        Self {
            url: url.into(),
            sha256: sha256.into(),
            archive_bytes,
            required_files: required_files.into_iter().map(str::to_string).collect(),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn archive_bytes(&self) -> u64 {
        self.archive_bytes
    }

    pub fn required_files(&self) -> &[String] {
        &self.required_files
    }
}

pub use installation::{
    install_archive, install_archive_with_support, remove_installation, validate_installation,
    AvailableModel, InstallationError, InstallationManifest, SupplementalFile,
    INSTALLATION_MANIFEST_FILE,
};

#[cfg(test)]
#[path = "../tests/unit/installation_tests.rs"]
mod installation_tests;
