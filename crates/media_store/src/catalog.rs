use crate::{MediaArtifact, MediaError};
use serde::Deserialize;
use std::collections::HashSet;

/// A release-pinned catalog. The host authenticates its source via app signing.
#[derive(Clone, Debug)]
pub struct MediaCatalog {
    resource_version: String,
    entries: Vec<MediaArtifact>,
    bundled: HashSet<String>,
}

#[derive(Deserialize)]
struct CatalogInput {
    schema_version: u32,
    resource_version: String,
    resources: Vec<ArtifactInput>,
}

#[derive(Deserialize)]
struct ArtifactInput {
    path: String,
    url: String,
    bytes: u64,
    sha256: String,
    #[serde(default)]
    bundled: bool,
}

impl MediaCatalog {
    pub fn from_json(json: &str) -> Result<Self, MediaError> {
        let input: CatalogInput = serde_json::from_str(json)
            .map_err(|error| MediaError::InvalidArtifact(error.to_string()))?;
        if input.schema_version != 1 || input.resources.is_empty() {
            return Err(MediaError::InvalidArtifact(
                "unsupported or empty catalog".into(),
            ));
        }
        if !input
            .resource_version
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
            || !input
                .resource_version
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        {
            return Err(MediaError::InvalidArtifact(
                "invalid catalog version".into(),
            ));
        }
        let mut seen = HashSet::new();
        let mut bundled = HashSet::new();
        let mut entries = Vec::with_capacity(input.resources.len());
        for resource in input.resources {
            if !seen.insert(resource.path.clone()) {
                return Err(MediaError::InvalidArtifact("duplicate resource ID".into()));
            }
            if resource.bundled {
                bundled.insert(resource.path.clone());
            }
            entries.push(MediaArtifact::new(
                resource.path,
                resource.url,
                resource.bytes,
                resource.sha256,
            )?);
        }
        Ok(Self {
            resource_version: input.resource_version,
            entries,
            bundled,
        })
    }

    pub fn find(&self, relative_path: &str) -> Option<&MediaArtifact> {
        self.entries
            .iter()
            .find(|entry| entry.relative_path() == relative_path)
    }

    pub fn entries(&self) -> &[MediaArtifact] {
        &self.entries
    }
    pub fn resource_version(&self) -> &str {
        &self.resource_version
    }
    pub fn is_bundled(&self, relative_path: &str) -> bool {
        self.bundled.contains(relative_path)
    }
}
