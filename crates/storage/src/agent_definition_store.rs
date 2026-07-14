use crate::{
    current_unix_timestamp, migrations::open_project_connection, StorageError, StorageResult,
};
use canvas::AgentProfile;
use rusqlite::{params, OptionalExtension};
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentDefinition {
    pub id: i64,
    pub name: String,
    pub runtime_profile: AgentProfile,
    pub workflow_description: String,
    pub enabled: bool,
    pub sort_order: i64,
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
        let connection = open_project_connection(&self.path)?;
        let now = current_unix_timestamp()?;
        connection.execute(
            "INSERT INTO agent_definitions \
             (name, runtime_profile, workflow_description, enabled, sort_order, created_at_unix, updated_at_unix) \
             VALUES (?1, ?2, ?3, 1, 0, ?4, ?4)",
            params![
                input.name,
                encode_agent_profile(input.runtime_profile),
                input.workflow_description,
                now,
            ],
        )?;
        let id = connection.last_insert_rowid();
        self.get(id)?
            .ok_or(StorageError::AgentDefinitionNotFound(id))
    }

    pub fn list(&self) -> StorageResult<Vec<AgentDefinition>> {
        let connection = open_project_connection(&self.path)?;
        let mut statement = connection.prepare(
            "SELECT id, name, runtime_profile, workflow_description, enabled, sort_order \
             FROM agent_definitions \
             ORDER BY sort_order ASC, name COLLATE NOCASE ASC",
        )?;
        let rows = statement.query_map([], decode_agent_definition_row)?;
        let mut definitions = Vec::new();
        for row in rows {
            let (id, name, runtime_profile, workflow_description, enabled, sort_order) = row?;
            definitions.push(AgentDefinition {
                id,
                name,
                runtime_profile: decode_agent_profile(&runtime_profile)?,
                workflow_description,
                enabled: enabled != 0,
                sort_order,
            });
        }
        Ok(definitions)
    }

    pub fn update(&self, input: &AgentDefinitionUpdate) -> StorageResult<AgentDefinition> {
        let connection = open_project_connection(&self.path)?;
        let changed = connection.execute(
            "UPDATE agent_definitions SET \
             name = ?1, runtime_profile = ?2, workflow_description = ?3, enabled = ?4, \
             sort_order = ?5, updated_at_unix = ?6 \
             WHERE id = ?7",
            params![
                input.name,
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
        self.get(input.id)?
            .ok_or(StorageError::AgentDefinitionNotFound(input.id))
    }

    pub fn delete(&self, id: i64) -> StorageResult<bool> {
        let connection = open_project_connection(&self.path)?;
        Ok(connection.execute("DELETE FROM agent_definitions WHERE id = ?1", params![id])? > 0)
    }

    fn get(&self, id: i64) -> StorageResult<Option<AgentDefinition>> {
        let connection = open_project_connection(&self.path)?;
        let row = connection
            .query_row(
                "SELECT id, name, runtime_profile, workflow_description, enabled, sort_order \
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
                    ))
                },
            )
            .optional()?;
        match row {
            Some((id, name, runtime_profile, workflow_description, enabled, sort_order)) => {
                Ok(Some(AgentDefinition {
                    id,
                    name,
                    runtime_profile: decode_agent_profile(&runtime_profile)?,
                    workflow_description,
                    enabled: enabled != 0,
                    sort_order,
                }))
            }
            None => Ok(None),
        }
    }
}

fn decode_agent_definition_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(i64, String, String, String, i64, i64)> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

fn encode_agent_profile(profile: AgentProfile) -> &'static str {
    match profile {
        AgentProfile::Claude => "claude",
        AgentProfile::Codex => "codex",
        AgentProfile::Cursor => "cursor",
        AgentProfile::OpenCode => "opencode",
    }
}

fn decode_agent_profile(profile: &str) -> StorageResult<AgentProfile> {
    match profile {
        "claude" => Ok(AgentProfile::Claude),
        "codex" => Ok(AgentProfile::Codex),
        "cursor" => Ok(AgentProfile::Cursor),
        "opencode" => Ok(AgentProfile::OpenCode),
        other => Err(StorageError::InvalidAgentProfile(other.to_string())),
    }
}

#[cfg(test)]
#[path = "../tests/unit/agent_definition_store_tests.rs"]
mod tests;
