use super::thread_message::{append, uuid};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// An explicit continuation of a previously dispatched task, never a new Agent task.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadFollowup {
    pub operation_id: String,
    pub collaboration_workspace_id: String,
    pub thread_id: String,
    pub source_operation_id: String,
    pub source_message_id: String,
    pub parent_operation_id: String,
    pub task_ref: String,
    pub text: String,
}

impl ThreadFollowup {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [
                &self.operation_id,
                &self.collaboration_workspace_id,
                &self.thread_id,
                &self.source_operation_id,
                &self.source_message_id,
                &self.parent_operation_id,
            ]
            .into_iter()
            .all(|id| uuid(id)),
            "invalid follow-up identity"
        );
        ensure!(
            self.operation_id != self.source_operation_id
                && self.operation_id != self.parent_operation_id,
            "follow-up requires its own operation"
        );
        ensure!(
            !self.task_ref.is_empty() && self.task_ref.len() <= 160,
            "invalid original task"
        );
        // Reuse the existing Provider message budget; control bytes cannot become terminal input.
        ensure!(
            !self.text.trim().is_empty()
                && self.text.len() <= 32 * 1024
                && !self
                    .text
                    .chars()
                    .any(|c| c.is_control() && c != '\n' && c != '\t'),
            "invalid follow-up text"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= 64 * 1024,
            "follow-up payload too large"
        );
        Ok(())
    }

    pub fn fingerprint(&self, machine: &str) -> Result<String> {
        self.validate()?;
        let mut hash = Sha256::new();
        hash.update(b"Norma thread followup v1\0");
        for value in [
            machine,
            &self.operation_id,
            &self.collaboration_workspace_id,
            &self.thread_id,
            &self.source_operation_id,
            &self.source_message_id,
            &self.parent_operation_id,
            &self.task_ref,
            &self.text,
        ] {
            append(&mut hash, value);
        }
        Ok(hex::encode(hash.finalize()))
    }
}
