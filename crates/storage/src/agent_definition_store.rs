use crate::{
    current_unix_timestamp, migrations::open_project_connection, StorageError, StorageResult,
};
use agent::AgentName;
use canvas::AgentProfile;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use std::path::PathBuf;

type AgentDefinitionRow = (
    i64,
    String,
    String,
    String,
    i64,
    i64,
    Option<String>,
    String,
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentDefinitionOrigin {
    Explicit,
    QuickCard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentDefinition {
    pub id: i64,
    pub name: String,
    pub speech_alias: Option<String>,
    pub runtime_profile: AgentProfile,
    pub workflow_description: String,
    pub enabled: bool,
    pub sort_order: i64,
    pub origin: AgentDefinitionOrigin,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewAgentDefinition {
    pub name: String,
    pub runtime_profile: AgentProfile,
    pub workflow_description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentDefinitionUpdate {
    pub id: i64,
    pub name: String,
    pub runtime_profile: AgentProfile,
    pub workflow_description: String,
    pub enabled: bool,
    pub sort_order: i64,
}

#[derive(Clone, Debug)]
pub struct AgentDefinitionStore {
    path: PathBuf,
}

impl AgentDefinitionStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn create(&self, input: &NewAgentDefinition) -> StorageResult<AgentDefinition> {
        self.create_with_origin(input, AgentDefinitionOrigin::Explicit)
    }

    pub fn create_quick(&self, input: &NewAgentDefinition) -> StorageResult<AgentDefinition> {
        self.create_with_origin(input, AgentDefinitionOrigin::QuickCard)
    }

    fn create_with_origin(
        &self,
        input: &NewAgentDefinition,
        origin: AgentDefinitionOrigin,
    ) -> StorageResult<AgentDefinition> {
        let mut connection = open_project_connection(&self.path)?;
        let now = current_unix_timestamp()?;
        let (name_key, speech_alias) = agent_name_metadata(&input.name)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        reject_name_conflict(
            &transaction,
            &input.name,
            &name_key,
            speech_alias.as_deref(),
            None,
        )?;
        transaction.execute(
            "INSERT INTO agent_definitions \
             (name, name_key, speech_alias, runtime_profile, workflow_description, enabled, sort_order, origin, created_at_unix, updated_at_unix) \
             VALUES (?1, ?2, ?3, ?4, ?5, 1, 0, ?6, ?7, ?7)",
            params![
                input.name,
                name_key,
                speech_alias,
                encode_agent_profile(input.runtime_profile),
                input.workflow_description,
                encode_agent_definition_origin(origin),
                now,
            ],
        )?;
        let id = transaction.last_insert_rowid();
        transaction.commit()?;
        self.get(id)?
            .ok_or(StorageError::AgentDefinitionNotFound(id))
    }

    pub fn list(&self) -> StorageResult<Vec<AgentDefinition>> {
        let connection = open_project_connection(&self.path)?;
        let mut statement = connection.prepare(
            "SELECT id, name, runtime_profile, workflow_description, enabled, sort_order, speech_alias, origin \
             FROM agent_definitions \
             ORDER BY sort_order ASC, name COLLATE NOCASE ASC",
        )?;
        let rows = statement.query_map([], decode_agent_definition_row)?;
        let mut definitions = Vec::new();
        for row in rows {
            let (
                id,
                name,
                runtime_profile,
                workflow_description,
                enabled,
                sort_order,
                speech_alias,
                origin,
            ) = row?;
            definitions.push(AgentDefinition {
                id,
                name,
                speech_alias,
                runtime_profile: decode_agent_profile(&runtime_profile)?,
                workflow_description,
                enabled: enabled != 0,
                sort_order,
                origin: decode_agent_definition_origin(&origin)?,
            });
        }
        Ok(definitions)
    }

    pub fn update(&self, input: &AgentDefinitionUpdate) -> StorageResult<AgentDefinition> {
        let mut connection = open_project_connection(&self.path)?;
        let (name_key, speech_alias) = agent_name_metadata(&input.name)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        reject_name_conflict(
            &transaction,
            &input.name,
            &name_key,
            speech_alias.as_deref(),
            Some(input.id),
        )?;
        let changed = transaction.execute(
            "UPDATE agent_definitions SET \
             name = ?1, name_key = ?2, speech_alias = ?3, runtime_profile = ?4, \
             workflow_description = ?5, enabled = ?6, sort_order = ?7, updated_at_unix = ?8 \
             WHERE id = ?9",
            params![
                input.name,
                name_key,
                speech_alias,
                encode_agent_profile(input.runtime_profile),
                input.workflow_description,
                if input.enabled { 1 } else { 0 },
                input.sort_order,
                current_unix_timestamp()?,
                input.id,
            ],
        )?;
        if changed == 0 {
            return Err(StorageError::AgentDefinitionNotFound(input.id));
        }
        transaction.commit()?;
        self.get(input.id)?
            .ok_or(StorageError::AgentDefinitionNotFound(input.id))
    }

    pub fn delete(&self, id: i64) -> StorageResult<bool> {
        let mut connection = open_project_connection(&self.path)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "DELETE FROM agent_dispatch_tasks WHERE definition_id = ?1",
            params![id],
        )?;
        let deleted =
            transaction.execute("DELETE FROM agent_definitions WHERE id = ?1", params![id])? > 0;
        transaction.commit()?;
        Ok(deleted)
    }

    pub fn delete_if_quick(&self, id: i64) -> StorageResult<bool> {
        let mut connection = open_project_connection(&self.path)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let origin = transaction
            .query_row(
                "SELECT origin FROM agent_definitions WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if origin.as_deref()
            != Some(encode_agent_definition_origin(
                AgentDefinitionOrigin::QuickCard,
            ))
        {
            transaction.commit()?;
            return Ok(false);
        }
        transaction.execute(
            "DELETE FROM agent_dispatch_tasks WHERE definition_id = ?1",
            params![id],
        )?;
        let deleted = transaction.execute(
            "DELETE FROM agent_definitions WHERE id = ?1 AND origin = ?2",
            params![
                id,
                encode_agent_definition_origin(AgentDefinitionOrigin::QuickCard)
            ],
        )? > 0;
        transaction.commit()?;
        Ok(deleted)
    }

    fn get(&self, id: i64) -> StorageResult<Option<AgentDefinition>> {
        let connection = open_project_connection(&self.path)?;
        let row = connection
            .query_row(
                "SELECT id, name, runtime_profile, workflow_description, enabled, sort_order, speech_alias, origin \
                 FROM agent_definitions WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .optional()?;
        match row {
            Some((
                id,
                name,
                runtime_profile,
                workflow_description,
                enabled,
                sort_order,
                speech_alias,
                origin,
            )) => Ok(Some(AgentDefinition {
                id,
                name,
                speech_alias,
                runtime_profile: decode_agent_profile(&runtime_profile)?,
                workflow_description,
                enabled: enabled != 0,
                sort_order,
                origin: decode_agent_definition_origin(&origin)?,
            })),
            None => Ok(None),
        }
    }
}

fn agent_name_metadata(name: &str) -> StorageResult<(String, Option<String>)> {
    let name_key = AgentName::parse(name)
        .map_err(|_| StorageError::InvalidAgentName(name.to_string()))?
        .match_key()
        .to_string();
    let speech_alias = agent::builtin_agent_names()
        .iter()
        .find(|builtin| builtin.name().eq_ignore_ascii_case(&name_key))
        .map(|builtin| builtin.speech_alias().to_string());
    Ok((name_key, speech_alias))
}

fn reject_name_conflict(
    transaction: &Transaction<'_>,
    requested: &str,
    name_key: &str,
    speech_alias: Option<&str>,
    excluding_id: Option<i64>,
) -> StorageResult<()> {
    let existing = transaction
        .query_row(
            "SELECT name FROM agent_definitions \
             WHERE (?3 IS NULL OR id != ?3) AND (name_key = ?1 OR speech_alias = ?1 \
             OR (?2 IS NOT NULL AND (name_key = ?2 OR speech_alias = ?2))) \
             ORDER BY id ASC LIMIT 1",
            params![name_key, speech_alias, excluding_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if let Some(existing) = existing {
        return Err(StorageError::AgentNameConflict {
            requested: requested.to_string(),
            existing,
        });
    }
    Ok(())
}

fn decode_agent_definition_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentDefinitionRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
    ))
}

fn encode_agent_definition_origin(origin: AgentDefinitionOrigin) -> &'static str {
    match origin {
        AgentDefinitionOrigin::Explicit => "explicit",
        AgentDefinitionOrigin::QuickCard => "quick_card",
    }
}

fn decode_agent_definition_origin(origin: &str) -> StorageResult<AgentDefinitionOrigin> {
    match origin {
        "explicit" => Ok(AgentDefinitionOrigin::Explicit),
        "quick_card" => Ok(AgentDefinitionOrigin::QuickCard),
        other => Err(StorageError::InvalidAgentDefinitionOrigin(
            other.to_string(),
        )),
    }
}

fn encode_agent_profile(profile: AgentProfile) -> &'static str {
    match profile {
        AgentProfile::Claude => "claude",
        AgentProfile::Codex => "codex",
        AgentProfile::Cursor => "cursor",
        AgentProfile::Grok => "grok",
        AgentProfile::OpenCode => "opencode",
    }
}

fn decode_agent_profile(profile: &str) -> StorageResult<AgentProfile> {
    match profile {
        "claude" => Ok(AgentProfile::Claude),
        "codex" => Ok(AgentProfile::Codex),
        "cursor" => Ok(AgentProfile::Cursor),
        "grok" => Ok(AgentProfile::Grok),
        "opencode" => Ok(AgentProfile::OpenCode),
        other => Err(StorageError::InvalidAgentProfile(other.to_string())),
    }
}

#[cfg(test)]
#[path = "../tests/unit/agent_definition_store_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/unit/agent_definition_origin_tests.rs"]
mod origin_tests;

#[cfg(test)]
#[path = "../tests/unit/agent_definition_migration_tests.rs"]
mod migration_tests;
