use crate::{current_unix_timestamp, migrations::open_project_connection, StorageResult};
use canvas::NodeId;
use rusqlite::params;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct VoiceHistoryEntry {
    pub transcript: String,
    pub raw_transcript: String,
    pub refined_transcript: Option<String>,
    pub planner_json: String,
    pub confidence: f64,
    pub risk: String,
    pub status: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub target_node_id: Option<NodeId>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VoiceHistoryRecord {
    pub id: i64,
    pub transcript: String,
    pub raw_transcript: String,
    pub refined_transcript: Option<String>,
    pub planner_json: String,
    pub confidence: f64,
    pub risk: String,
    pub status: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub target_node_id: Option<NodeId>,
    pub created_at_unix: u64,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct VoiceHistoryStore {
    path: PathBuf,
}

impl VoiceHistoryStore {
    pub fn new(path: PathBuf) -> Self {
        VoiceHistoryStore { path }
    }

    pub fn save_entry(&self, entry: &VoiceHistoryEntry) -> StorageResult<i64> {
        let connection = open_project_connection(&self.path)?;
        connection.execute(
            r#"
            INSERT INTO voice_history (
                transcript, planner_json, confidence, risk, status, provider, model,
                target_node_id, created_at_unix, error, raw_transcript, refined_transcript
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                entry.transcript,
                entry.planner_json,
                entry.confidence,
                entry.risk,
                entry.status,
                entry.provider,
                entry.model,
                entry.target_node_id,
                current_unix_timestamp()?,
                entry.error,
                entry.raw_transcript,
                entry.refined_transcript,
            ],
        )?;
        Ok(connection.last_insert_rowid())
    }

    pub fn load_recent(&self, limit: usize) -> StorageResult<Vec<VoiceHistoryRecord>> {
        let connection = open_project_connection(&self.path)?;
        let mut statement = connection.prepare(
            r#"
            SELECT id, transcript, planner_json, confidence, risk, status, provider, model,
                   target_node_id, created_at_unix, error, raw_transcript, refined_transcript
            FROM voice_history
            ORDER BY id DESC
            LIMIT ?1
            "#,
        )?;
        let rows = statement.query_map([limit as i64], |row| {
            Ok(VoiceHistoryRecord {
                id: row.get(0)?,
                transcript: row.get(1)?,
                planner_json: row.get(2)?,
                confidence: row.get(3)?,
                risk: row.get(4)?,
                status: row.get(5)?,
                provider: row.get(6)?,
                model: row.get(7)?,
                target_node_id: row.get(8)?,
                created_at_unix: row.get(9)?,
                error: row.get(10)?,
                raw_transcript: row.get(11)?,
                refined_transcript: row.get(12)?,
            })
        })?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }
}

#[cfg(test)]
#[path = "../tests/unit/voice_history_store_tests.rs"]
mod tests;
