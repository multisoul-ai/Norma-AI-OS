use crate::{
    current_unix_timestamp, migrations::open_project_connection, StorageError, StorageResult,
};
use rusqlite::{params, OptionalExtension};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoredConversationAvailability {
    Available,
    Unavailable,
    Stale,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredAgentConversationTail {
    pub definition_id: i64,
    pub node_id: u64,
    pub session_id: String,
    pub agent_name: String,
    pub profile: String,
    pub last_user_message: Option<String>,
    pub last_agent_message: Option<String>,
    pub updated_at_unix: i64,
    pub availability: StoredConversationAvailability,
    pub user_message_truncated: bool,
    pub agent_message_truncated: bool,
}

#[derive(Clone, Debug)]
pub struct AgentConversationTailStore {
    path: PathBuf,
}

impl AgentConversationTailStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn upsert(&self, tail: &StoredAgentConversationTail) -> StorageResult<()> {
        let connection = open_project_connection(&self.path)?;
        let now = current_unix_timestamp()? as i64;
        connection.execute(
            "INSERT INTO agent_conversation_tails (
                definition_id, node_id, session_id, agent_name, profile,
                last_user_message, last_agent_message, updated_at_unix, availability,
                user_message_truncated, agent_message_truncated
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(definition_id, session_id) DO UPDATE SET
                node_id = excluded.node_id,
                agent_name = excluded.agent_name,
                profile = excluded.profile,
                last_user_message = excluded.last_user_message,
                last_agent_message = excluded.last_agent_message,
                updated_at_unix = excluded.updated_at_unix,
                availability = excluded.availability,
                user_message_truncated = excluded.user_message_truncated,
                agent_message_truncated = excluded.agent_message_truncated",
            params![
                tail.definition_id,
                tail.node_id as i64,
                tail.session_id,
                tail.agent_name,
                tail.profile,
                tail.last_user_message,
                tail.last_agent_message,
                if tail.updated_at_unix > 0 {
                    tail.updated_at_unix
                } else {
                    now
                },
                encode_availability(tail.availability),
                tail.user_message_truncated as i64,
                tail.agent_message_truncated as i64,
            ],
        )?;
        Ok(())
    }

    pub fn get(
        &self,
        definition_id: i64,
        session_id: &str,
    ) -> StorageResult<Option<StoredAgentConversationTail>> {
        let connection = open_project_connection(&self.path)?;
        connection
            .query_row(
                "SELECT definition_id, node_id, session_id, agent_name, profile,
                        last_user_message, last_agent_message, updated_at_unix, availability,
                        user_message_truncated, agent_message_truncated
                 FROM agent_conversation_tails
                 WHERE definition_id = ?1 AND session_id = ?2",
                params![definition_id, session_id],
                decode_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list(&self) -> StorageResult<Vec<StoredAgentConversationTail>> {
        let connection = open_project_connection(&self.path)?;
        let mut statement = connection.prepare(
            "SELECT definition_id, node_id, session_id, agent_name, profile,
                    last_user_message, last_agent_message, updated_at_unix, availability,
                    user_message_truncated, agent_message_truncated
             FROM agent_conversation_tails
             ORDER BY updated_at_unix DESC, definition_id ASC",
        )?;
        let rows = statement.query_map([], decode_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete(&self, definition_id: i64, session_id: &str) -> StorageResult<bool> {
        let connection = open_project_connection(&self.path)?;
        let changed = connection.execute(
            "DELETE FROM agent_conversation_tails WHERE definition_id = ?1 AND session_id = ?2",
            params![definition_id, session_id],
        )?;
        Ok(changed > 0)
    }

    pub fn delete_for_definition(&self, definition_id: i64) -> StorageResult<usize> {
        let connection = open_project_connection(&self.path)?;
        let changed = connection.execute(
            "DELETE FROM agent_conversation_tails WHERE definition_id = ?1",
            params![definition_id],
        )?;
        Ok(changed)
    }

    pub fn delete_for_node(&self, node_id: u64) -> StorageResult<usize> {
        let connection = open_project_connection(&self.path)?;
        let changed = connection.execute(
            "DELETE FROM agent_conversation_tails WHERE node_id = ?1",
            params![node_id as i64],
        )?;
        Ok(changed)
    }
}

fn encode_availability(value: StoredConversationAvailability) -> &'static str {
    match value {
        StoredConversationAvailability::Available => "available",
        StoredConversationAvailability::Unavailable => "unavailable",
        StoredConversationAvailability::Stale => "stale",
    }
}

fn decode_availability(value: &str) -> Result<StoredConversationAvailability, StorageError> {
    match value {
        "available" => Ok(StoredConversationAvailability::Available),
        "unavailable" => Ok(StoredConversationAvailability::Unavailable),
        "stale" => Ok(StoredConversationAvailability::Stale),
        other => Err(StorageError::InvalidConversationAvailability(
            other.to_string(),
        )),
    }
}

fn decode_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredAgentConversationTail> {
    let availability = decode_availability(&row.get::<_, String>(8)?)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    Ok(StoredAgentConversationTail {
        definition_id: row.get(0)?,
        node_id: row.get::<_, i64>(1)? as u64,
        session_id: row.get(2)?,
        agent_name: row.get(3)?,
        profile: row.get(4)?,
        last_user_message: row.get(5)?,
        last_agent_message: row.get(6)?,
        updated_at_unix: row.get(7)?,
        availability,
        user_message_truncated: row.get::<_, i64>(9)? != 0,
        agent_message_truncated: row.get::<_, i64>(10)? != 0,
    })
}

#[cfg(test)]
#[path = "../tests/unit/agent_conversation_tail_store_tests.rs"]
mod tests;
