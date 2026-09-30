use super::thread_message::{append, uuid};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A phone-decided task. Route and authorization revision belong to attempts, not this identity.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadTask {
    pub operation_id: String,
    pub collaboration_workspace_id: String,
    pub thread_id: String,
    pub source_operation_id: String,
    pub source_message_id: String,
    pub project_id: String,
    pub agent_definition_id: i64,
    pub task: String,
    pub context_version: u64,
    pub context: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision_of: Option<ThreadTaskRevision>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadTaskRevision {
    pub operation_id: String,
    pub task_ref: String,
    pub fingerprint: String,
}

impl ThreadTask {
    pub fn validate(&self) -> Result<()> {
        if let Some(revision) = &self.revision_of {
            ensure!(
                uuid(&revision.operation_id)
                    && revision.operation_id != self.operation_id
                    && !revision.task_ref.is_empty()
                    && revision.task_ref.len() <= 160
                    && revision.fingerprint.len() == 64
                    && revision.fingerprint.bytes().all(|b| b.is_ascii_hexdigit()),
                "invalid original delivery"
            );
        }
        ensure!(
            [
                &self.operation_id,
                &self.collaboration_workspace_id,
                &self.thread_id,
                &self.source_operation_id,
                &self.source_message_id
            ]
            .into_iter()
            .all(|id| uuid(id)),
            "invalid task identity"
        );
        ensure!(
            self.operation_id != self.source_operation_id,
            "task must have its own operation"
        );
        ensure!(
            !self.project_id.is_empty()
                && self.project_id.len() <= 128
                && self
                    .project_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
            "invalid task project"
        );
        ensure!(
            self.agent_definition_id > 0 && (1..=i64::MAX as u64).contains(&self.context_version),
            "invalid task target or context version"
        );
        // The existing command message limit is 32 KiB; include shared context in that budget.
        ensure!(
            !self.task.trim().is_empty()
                && self.task.len() + self.context.len() <= 32 * 1024
                && !self
                    .task
                    .chars()
                    .chain(self.context.chars())
                    .any(|c| c.is_control() && c != '\n' && c != '\t'),
            "invalid task content"
        );
        // Escaped JSON is stored inside a binding; cap it before the outer relay frame is assembled.
        ensure!(
            serde_json::to_vec(self)?.len() <= 64 * 1024,
            "task payload too large"
        );
        Ok(())
    }

    pub fn fingerprint(&self, machine: &str) -> Result<String> {
        self.validate()?;
        let mut hash = Sha256::new();
        hash.update(b"Norma thread task v1\0");
        for value in [
            machine,
            &self.operation_id,
            &self.collaboration_workspace_id,
            &self.thread_id,
            &self.source_operation_id,
            &self.source_message_id,
            &self.project_id,
        ] {
            append(&mut hash, value);
        }
        hash.update((self.agent_definition_id as u64).to_be_bytes());
        hash.update(self.context_version.to_be_bytes());
        append(&mut hash, &self.task);
        append(&mut hash, &self.context);
        if let Some(revision) = &self.revision_of {
            append(&mut hash, "Norma thread revision v1");
            append(&mut hash, &revision.operation_id);
            append(&mut hash, &revision.task_ref);
            append(&mut hash, &revision.fingerprint);
        }
        Ok(hex::encode(hash.finalize()))
    }
}
