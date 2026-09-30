use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A visible message replica. Reception alone never grants execution intent.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadMessage {
    pub operation_id: String,
    pub collaboration_workspace_id: String,
    pub thread_id: String,
    pub message_id: String,
    pub origin_device_id: String,
    pub project_ids: Vec<String>,
    pub text: String,
}

impl ThreadMessage {
    pub fn validate(&self) -> Result<()> {
        for id in [
            &self.operation_id,
            &self.collaboration_workspace_id,
            &self.thread_id,
            &self.message_id,
            &self.origin_device_id,
        ] {
            ensure!(uuid(id), "invalid thread identity");
        }
        ensure!(
            (1..=32).contains(&self.project_ids.len())
                && self.project_ids.iter().all(|id| !id.is_empty()
                    && id.len() <= 128
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)))
                && self.project_ids.windows(2).all(|pair| pair[0] < pair[1]),
            "invalid project scope"
        );
        ensure!(
            !self.text.trim().is_empty() && self.text.len() <= 64 * 1024,
            "invalid thread text"
        );
        // Leave >=64 KiB of the 256 KiB encrypted envelope for routing and authentication metadata.
        ensure!(
            serde_json::to_vec(self)?.len() <= 192 * 1024,
            "thread payload too large"
        );
        Ok(())
    }

    pub fn fingerprint(&self, machine: &str) -> Result<String> {
        self.validate()?;
        let mut hash = Sha256::new();
        hash.update(b"Norma thread message v1\0");
        for value in [
            machine,
            &self.operation_id,
            &self.collaboration_workspace_id,
            &self.thread_id,
            &self.message_id,
            &self.origin_device_id,
        ] {
            append(&mut hash, value);
        }
        hash.update((self.project_ids.len() as u32).to_be_bytes());
        for project in &self.project_ids {
            append(&mut hash, project);
        }
        append(&mut hash, &self.text);
        Ok(hex::encode(hash.finalize()))
    }
}

pub(super) fn append(hash: &mut Sha256, text: &str) {
    hash.update((text.len() as u32).to_be_bytes());
    hash.update(text.as_bytes());
}

pub(super) fn uuid(text: &str) -> bool {
    text.len() == 36
        && text.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
