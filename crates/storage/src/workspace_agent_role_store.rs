//! Workspace role names and their retired call keys are one persistent namespace.
//! Run these SQLite operations off the GPUI foreground and PTY input/output paths.
mod mutation;
mod schema;
mod snapshot;
pub(crate) use schema::migrate;

use crate::{current_unix_timestamp, migrations::open_app_connection, StorageError, StorageResult};
use agent::{workspace_dispatch::WorkspaceCardRef, AgentName};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, Row, TransactionBehavior};
use std::{path::PathBuf, time::Duration};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceAgentRole {
    pub card: WorkspaceCardRef,
    pub name: String,
    pub speech_alias: Option<String>,
    pub enabled: bool,
    pub created_at_unix: u64,
    pub updated_at_unix: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkspaceCallResolution {
    Active(WorkspaceAgentRole),
    Inactive(WorkspaceAgentRole),
    Missing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceCallKeyKind {
    Name,
    SpeechAlias,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceCallKeyState {
    Active,
    Inactive,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceCallKey {
    pub normalized_key: String,
    pub display: String,
    pub kind: WorkspaceCallKeyKind,
    pub state: WorkspaceCallKeyState,
    pub card_instance_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceRoleSnapshot {
    pub workspace_id: String,
    pub revision: u64,
    /// Schema presence does not establish complete registration; T03 verifies coverage.
    pub schema_present: bool,
    pub roles: Vec<WorkspaceAgentRole>,
    pub call_keys: Vec<WorkspaceCallKey>,
}

#[derive(Clone, Debug)]
pub struct WorkspaceAgentRoleStore {
    path: PathBuf,
}

const ROLE_COLUMNS: &str = "workspace_id,area_id,node_id,card_instance_id,name,speech_alias,enabled,created_at_unix,updated_at_unix";

impl WorkspaceAgentRoleStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn create(
        &self,
        card: &WorkspaceCardRef,
        name: &str,
        enabled: bool,
    ) -> StorageResult<WorkspaceAgentRole> {
        validate_card(card)?;
        let name = parse_name(name)?;
        let mut connection = self.write_connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let duplicate: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM workspace_agent_roles WHERE workspace_id=?1 AND
                (card_instance_id=?2 OR (area_id=?3 AND node_id=?4)))",
            params![
                card.workspace_id,
                card.card_instance_id,
                card.area_id,
                card.node_id
            ],
            |row| row.get(0),
        )?;
        if duplicate {
            return Err(StorageError::WorkspaceAgentCardConflict(
                card.card_instance_id.clone(),
            ));
        }
        reject_name_conflicts(&tx, &card.workspace_id, &name, None)?;
        let now = current_unix_timestamp()?;
        tx.execute(
            "INSERT INTO workspace_agent_roles(workspace_id,card_instance_id,area_id,node_id,name,speech_alias,enabled,created_at_unix,updated_at_unix)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?8)",
            params![card.workspace_id,card.card_instance_id,card.area_id,card.node_id,name.display(),name.speech_alias(),enabled,now],
        )?;
        insert_keys(&tx, card, &name)?;
        bump_revision(&tx, &card.workspace_id)?;
        let role = read_role(&tx, card)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        tx.commit()?;
        Ok(role)
    }

    pub fn get(&self, card: &WorkspaceCardRef) -> StorageResult<Option<WorkspaceAgentRole>> {
        validate_card(card)?;
        match self.read_connection()? {
            Some(connection) => read_role(&connection, card),
            None => Ok(None),
        }
    }

    pub fn list(&self, workspace_id: &str) -> StorageResult<Vec<WorkspaceAgentRole>> {
        let Some(connection) = self.read_connection()? else {
            return Ok(Vec::new());
        };
        let mut query = connection.prepare(&format!("SELECT {ROLE_COLUMNS} FROM workspace_agent_roles WHERE workspace_id=?1 ORDER BY area_id,node_id,card_instance_id"))?;
        let rows = query.query_map([workspace_id], decode_role)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn resolve(
        &self,
        workspace_id: &str,
        call_name: &str,
    ) -> StorageResult<WorkspaceCallResolution> {
        let name = match AgentName::parse(call_name) {
            Ok(name) => name,
            Err(_) => return Ok(WorkspaceCallResolution::Missing),
        };
        let Some(mut connection) = self.read_connection()? else {
            return Ok(WorkspaceCallResolution::Missing);
        };
        let tx = connection.transaction()?;
        let binding: Option<(String,String)> = tx.query_row(
            "SELECT card_instance_id,state FROM workspace_agent_call_keys WHERE workspace_id=?1 AND normalized_key=?2",
            params![workspace_id,name.match_key()], |row| Ok((row.get(0)?,row.get(1)?)),
        ).optional()?;
        let Some((card_id, state)) = binding else {
            return Ok(WorkspaceCallResolution::Missing);
        };
        let role = tx.query_row(&format!("SELECT {ROLE_COLUMNS} FROM workspace_agent_roles WHERE workspace_id=?1 AND card_instance_id=?2"),params![workspace_id,card_id],decode_role)?;
        match state.as_str() {
            "active" => Ok(WorkspaceCallResolution::Active(role)),
            "inactive" => Ok(WorkspaceCallResolution::Inactive(role)),
            _ => Err(StorageError::InvalidWorkspaceAgentRole(
                "invalid_call_key_state".into(),
            )),
        }
    }

    fn write_connection(&self) -> StorageResult<Connection> {
        let connection = open_app_connection(&self.path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        Ok(connection)
    }

    fn read_connection(&self) -> StorageResult<Option<Connection>> {
        if !self.path.try_exists()? {
            return Ok(None);
        }
        let connection = Connection::open_with_flags(&self.path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        let present: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workspace_agent_roles')", [], |row| row.get(0))?;
        Ok(present.then_some(connection))
    }
}

fn parse_name(name: &str) -> StorageResult<AgentName> {
    AgentName::parse(name).map_err(|_| StorageError::InvalidAgentName(name.into()))
}

fn validate_card(card: &WorkspaceCardRef) -> StorageResult<()> {
    card.validate()
        .map_err(|error| StorageError::InvalidWorkspaceAgentRole(error.code().into()))?;
    if card.node_id > i64::MAX as u64 {
        return Err(StorageError::InvalidWorkspaceAgentRole(
            "node_id_overflow".into(),
        ));
    }
    Ok(())
}

fn name_keys(name: &AgentName) -> Vec<(&str, &str, &str)> {
    let mut keys = vec![(name.match_key(), name.display(), "name")];
    if let Some(alias) = name.speech_alias() {
        keys.push((alias, alias, "speech_alias"));
    }
    keys
}

fn reject_name_conflicts(
    connection: &Connection,
    workspace: &str,
    name: &AgentName,
    allowed_active_card: Option<&str>,
) -> StorageResult<()> {
    for (key, _, _) in name_keys(name) {
        let existing: Option<(String,String,String)> = connection
            .query_row(
                "SELECT r.name,k.card_instance_id,k.state FROM workspace_agent_call_keys k JOIN workspace_agent_roles r
                ON r.workspace_id=k.workspace_id AND r.card_instance_id=k.card_instance_id
             WHERE k.workspace_id=?1 AND k.normalized_key=?2",
                params![workspace, key],
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
            )
            .optional()?;
        if let Some((existing, owner, state)) = existing {
            if allowed_active_card == Some(owner.as_str()) && state == "active" {
                continue;
            }
            return Err(StorageError::AgentNameConflict {
                requested: name.display().into(),
                existing,
            });
        }
    }
    Ok(())
}

fn insert_keys(
    connection: &Connection,
    card: &WorkspaceCardRef,
    name: &AgentName,
) -> StorageResult<()> {
    for (key, display, kind) in name_keys(name) {
        connection.execute("INSERT INTO workspace_agent_call_keys(workspace_id,normalized_key,card_instance_id,display,kind,state) VALUES(?1,?2,?3,?4,?5,'active')",params![card.workspace_id,key,card.card_instance_id,display,kind])?;
    }
    Ok(())
}

fn bump_revision(connection: &Connection, workspace: &str) -> StorageResult<()> {
    connection.execute("INSERT INTO workspace_agent_role_revisions(workspace_id,revision) VALUES(?1,1) ON CONFLICT(workspace_id) DO UPDATE SET revision=revision+1",[workspace])?;
    Ok(())
}

fn read_role(
    connection: &Connection,
    card: &WorkspaceCardRef,
) -> StorageResult<Option<WorkspaceAgentRole>> {
    Ok(connection.query_row(&format!("SELECT {ROLE_COLUMNS} FROM workspace_agent_roles WHERE workspace_id=?1 AND card_instance_id=?2 AND area_id=?3 AND node_id=?4"),params![card.workspace_id,card.card_instance_id,card.area_id,card.node_id],decode_role).optional()?)
}

fn decode_role(row: &Row<'_>) -> rusqlite::Result<WorkspaceAgentRole> {
    Ok(WorkspaceAgentRole {
        card: WorkspaceCardRef {
            workspace_id: row.get(0)?,
            area_id: row.get(1)?,
            node_id: row.get(2)?,
            card_instance_id: row.get(3)?,
        },
        name: row.get(4)?,
        speech_alias: row.get(5)?,
        enabled: row.get(6)?,
        created_at_unix: row.get(7)?,
        updated_at_unix: row.get(8)?,
    })
}
