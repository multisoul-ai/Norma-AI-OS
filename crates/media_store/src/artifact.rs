use crate::MediaError;
use std::path::{Component, Path};

/// Original file metadata authenticated by the application's signed catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaArtifact {
    relative_path: String,
    url: String,
    bytes: u64,
    sha256: String,
}

impl MediaArtifact {
    pub fn new(
        relative_path: impl Into<String>,
        url: impl Into<String>,
        bytes: u64,
        sha256: impl Into<String>,
    ) -> Result<Self, MediaError> {
        let relative_path = relative_path.into();
        let url = url.into();
        let sha256 = sha256.into();
        if relative_path.is_empty()
            || relative_path.contains('\\')
            || relative_path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
            || Path::new(&relative_path)
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
            || Path::new(&relative_path).extension().is_none()
        {
            return Err(MediaError::InvalidArtifact("unsafe relative path".into()));
        }
        if sha256.len() != 64
            || !sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(MediaError::InvalidArtifact("invalid SHA-256".into()));
        }
        let parsed = url::Url::parse(&url)
            .map_err(|error| MediaError::InvalidArtifact(error.to_string()))?;
        if parsed.scheme() != "https"
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.fragment().is_some()
        {
            return Err(MediaError::InvalidArtifact(
                "download URL must use authenticated HTTPS".into(),
            ));
        }
        Ok(Self {
            relative_path,
            url,
            bytes,
            sha256,
        })
    }

    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub(crate) fn cache_filename(&self) -> String {
        let extension = Path::new(&self.relative_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("media");
        format!("{}.{}", self.sha256, extension)
    }
}
