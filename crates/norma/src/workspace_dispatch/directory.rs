use super::{CardExecutionFingerprint, WorkspaceCardRef};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Stable project identity from the catalog, never a model-supplied filesystem path.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceProjectRef {
    pub workspace_id: String,
    pub project_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceDirectoryProject {
    pub reference: WorkspaceProjectRef,
    pub name: String,
    pub area_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceDirectoryArea {
    pub area_id: String,
    pub project: WorkspaceProjectRef,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "capability", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkspaceSubmissionCapability {
    Supported,
    Unsupported { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceDirectoryRole {
    pub card: WorkspaceCardRef,
    pub project: WorkspaceProjectRef,
    pub name: String,
    pub speech_alias: Option<String>,
    pub enabled: bool,
    pub submission: WorkspaceSubmissionCapability,
    /// Cold persisted cards have no volatile execution identity until preparation.
    pub execution: Option<CardExecutionFingerprint>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceCallKeyStatus {
    Active,
    Retired,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceDirectoryCallKey {
    pub key: String,
    pub card: WorkspaceCardRef,
    pub state: WorkspaceCallKeyStatus,
}

/// Already authorization-filtered, full catalog. Retired keys are validation data;
/// user/model pickers must present only active, enabled, supported roles.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceDirectory {
    pub workspace_id: String,
    pub revision: u64,
    /// Includes authorized area membership, project identities and display names.
    pub catalog_fingerprint: String,
    pub authorization_revision: String,
    pub authorized_area_ids: Vec<String>,
    pub covered_area_ids: Vec<String>,
    pub missing_area_ids: Vec<String>,
    pub registration_complete: bool,
    pub projects: Vec<WorkspaceDirectoryProject>,
    pub areas: Vec<WorkspaceDirectoryArea>,
    pub roles: Vec<WorkspaceDirectoryRole>,
    pub call_keys: Vec<WorkspaceDirectoryCallKey>,
}

impl WorkspaceDirectory {
    pub fn is_complete(&self) -> bool {
        let authorized: HashSet<_> = self.authorized_area_ids.iter().collect();
        let covered: HashSet<_> = self.covered_area_ids.iter().collect();
        !self.catalog_fingerprint.is_empty()
            && self.registration_complete
            && self.missing_area_ids.is_empty()
            && authorized.len() == self.authorized_area_ids.len()
            && covered.len() == self.covered_area_ids.len()
            && authorized == covered
    }

    /// Validate scope and identity without IO; malformed projections fail closed.
    pub fn is_consistent(&self) -> bool {
        let areas: HashSet<_> = self.authorized_area_ids.iter().collect();
        if self
            .covered_area_ids
            .iter()
            .chain(&self.missing_area_ids)
            .any(|id| !areas.contains(id))
        {
            return false;
        }
        let mut projects = HashSet::new();
        let mut project_areas = HashSet::new();
        if self.projects.iter().any(|p| {
            p.reference.workspace_id != self.workspace_id
                || p.reference.project_id.is_empty()
                || !projects.insert(&p.reference)
                || p.area_ids
                    .iter()
                    .any(|id| !areas.contains(id) || !project_areas.insert(id))
        }) {
            return false;
        }
        let mut named_areas = HashSet::new();
        if self.areas.iter().any(|a| {
            !areas.contains(&a.area_id)
                || !named_areas.insert(&a.area_id)
                || !self
                    .projects
                    .iter()
                    .any(|p| p.reference == a.project && p.area_ids.contains(&a.area_id))
        }) || named_areas != project_areas
        {
            return false;
        }
        let mut cards = HashSet::new();
        let mut identities = HashSet::new();
        let mut locations = HashSet::new();
        if self.roles.iter().any(|r| {
            r.card.validate().is_err()
                || r.card.workspace_id != self.workspace_id
                || !areas.contains(&r.card.area_id)
                || !cards.insert(&r.card)
                || !identities.insert(&r.card.card_instance_id)
                || !locations.insert((&r.card.area_id, r.card.node_id))
                || !projects.contains(&r.project)
                || !self
                    .projects
                    .iter()
                    .any(|p| p.reference == r.project && p.area_ids.contains(&r.card.area_id))
                || r.execution.as_ref().is_some_and(|e| e.validate().is_err())
        }) {
            return false;
        }
        let mut keys = HashSet::new();
        if self.call_keys.iter().any(|k| {
            k.key.trim().is_empty()
                || k.key != normalize_workspace_call_key(&k.key)
                || !keys.insert(&k.key)
                || !cards.contains(&k.card)
                || k.state == WorkspaceCallKeyStatus::Active
                    && !self.roles.iter().any(|r| {
                        r.card == k.card
                            && (normalize_workspace_call_key(&r.name) == k.key
                                || r.speech_alias
                                    .as_ref()
                                    .is_some_and(|a| normalize_workspace_call_key(a) == k.key))
                    })
        }) {
            return false;
        }
        self.roles.iter().all(|r| {
            std::iter::once(&r.name)
                .chain(r.speech_alias.iter())
                .all(|n| {
                    self.call_keys.iter().any(|k| {
                        k.card == r.card
                            && k.state == WorkspaceCallKeyStatus::Active
                            && k.key == normalize_workspace_call_key(n)
                    })
                })
        })
    }
}

/// Same trim + ASCII case policy as AgentName; this does not allocate or validate names.
pub fn normalize_workspace_call_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
