use crate::{current_unix_timestamp, migrations::open_app_connection, StorageError, StorageResult};
use agent::ProjectPlanDraft;
use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct StoredProjectPlan {
    pub sequence: u64,
    pub plan_id: String,
    pub workspace_id: String,
    pub route_id: String,
    pub direction: String,
    pub draft: ProjectPlanDraft,
    pub created_at_unix: u64,
}

#[derive(Debug, Serialize)]
pub struct ProjectPlanPage {
    pub items: Vec<StoredProjectPlan>,
    pub next_cursor: Option<u64>,
}

/// Immutable proposals retain old directions; execution belongs to the existing command ledger.
#[derive(Clone)]
pub struct ProjectPlanStore {
    path: PathBuf,
}

impl ProjectPlanStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn publish(
        &self,
        id: &str,
        workspace: &str,
        route: &str,
        direction: &str,
        draft: &ProjectPlanDraft,
    ) -> StorageResult<StoredProjectPlan> {
        self.publish_inner(id, (workspace, route, None), direction, draft)
    }

    /// Inference based on an older plan cannot replace a newer decision from another client.
    pub fn publish_after(
        &self,
        id: &str,
        source: &StoredProjectPlan,
        direction: &str,
        draft: &ProjectPlanDraft,
    ) -> StorageResult<StoredProjectPlan> {
        self.publish_inner(
            id,
            (
                &source.workspace_id,
                &source.route_id,
                Some(&source.plan_id),
            ),
            direction,
            draft,
        )
    }

    fn publish_inner(
        &self,
        id: &str,
        scope: (&str, &str, Option<&str>),
        direction: &str,
        draft: &ProjectPlanDraft,
    ) -> StorageResult<StoredProjectPlan> {
        let (workspace, route, previous) = scope;
        for value in [id, workspace, route] {
            crate::mobile_access_store::validate_id(value)?;
        }
        if !agent::valid_plan_direction(direction) {
            return Err(invalid("invalid direction"));
        }
        draft.validate().map_err(invalid)?;
        let json = serde_json::to_string(draft)?;
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) = read(&tx, id)? {
            if existing.workspace_id == workspace
                && existing.route_id == route
                && existing.direction == direction
                && existing.draft == *draft
            {
                return Ok(existing);
            }
            return Err(invalid("plan id conflict"));
        }
        if let Some(previous) = previous {
            let latest: Option<String> = tx.query_row("SELECT plan_id FROM project_plans WHERE workspace_id=?1 ORDER BY sequence DESC LIMIT 1", [workspace], |row| row.get(0)).optional()?;
            if latest.as_deref() != Some(previous) {
                return Err(invalid("plan_changed"));
            }
        }
        tx.execute("INSERT INTO project_plans(plan_id,workspace_id,route_id,direction,draft_json,created_at_unix)
            VALUES (?1,?2,?3,?4,?5,?6)",params![id,workspace,route,direction,json,current_unix_timestamp()?])?;
        let plan = read(&tx, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        // Four80KiB records plus envelope fit existing500KiB wire budget. No truncation.
        if serde_json::to_vec(&plan)?.len() > 80 * 1024 {
            return Err(invalid("plan too large"));
        }
        tx.commit()?;
        Ok(plan)
    }

    pub fn get(&self, workspace: &str, id: &str) -> StorageResult<Option<StoredProjectPlan>> {
        crate::mobile_access_store::validate_id(workspace)?;
        crate::mobile_access_store::validate_id(id)?;
        let connection = open_app_connection(&self.path)?;
        // Scope is part of the SQL predicate; an ID is never authorization.
        connection
            .query_row(
                &format!("{SELECT} WHERE workspace_id=?1 AND plan_id=?2"),
                params![workspace, id],
                decode,
            )
            .optional()?
            .map(convert)
            .transpose()
    }

    pub fn list(
        &self,
        workspace: &str,
        before: Option<u64>,
        limit: usize,
    ) -> StorageResult<ProjectPlanPage> {
        crate::mobile_access_store::validate_id(workspace)?;
        if !(1..=4).contains(&limit) || before.is_some_and(|n| n == 0 || n > i64::MAX as u64) {
            return Err(invalid("invalid page"));
        }
        let connection = open_app_connection(&self.path)?;
        let mut query = connection.prepare(&format!("{SELECT} WHERE workspace_id=?1 AND (?2 IS NULL OR sequence<?2) ORDER BY sequence DESC LIMIT ?3"))?;
        let rows = query.query_map(params![workspace, before, (limit + 1) as u64], decode)?;
        let mut items = rows
            .map(|row| convert(row?))
            .collect::<StorageResult<Vec<_>>>()?;
        let more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = if more {
            items.last().map(|item| item.sequence)
        } else {
            None
        };
        Ok(ProjectPlanPage { items, next_cursor })
    }
}

const SELECT: &str = "SELECT sequence,plan_id,workspace_id,route_id,direction,draft_json,created_at_unix FROM project_plans";
type PlanRow = (u64, String, String, String, String, String, u64);
fn decode(row: &Row<'_>) -> rusqlite::Result<PlanRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
    ))
}
fn convert(row: PlanRow) -> StorageResult<StoredProjectPlan> {
    let (sequence, plan_id, workspace_id, route_id, direction, json, created_at_unix) = row;
    let draft: ProjectPlanDraft = serde_json::from_str(&json)?;
    draft.validate().map_err(invalid)?;
    Ok(StoredProjectPlan {
        sequence,
        plan_id,
        workspace_id,
        route_id,
        direction,
        draft,
        created_at_unix,
    })
}
pub(crate) fn read(connection: &Connection, id: &str) -> StorageResult<Option<StoredProjectPlan>> {
    connection
        .query_row(&format!("{SELECT} WHERE plan_id=?1"), [id], decode)
        .optional()?
        .map(convert)
        .transpose()
}
fn invalid(message: &str) -> StorageError {
    StorageError::InvalidProjectPlan(message.into())
}

pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS project_plans (
        sequence INTEGER PRIMARY KEY AUTOINCREMENT,
        plan_id TEXT UNIQUE NOT NULL,
        workspace_id TEXT NOT NULL REFERENCES workspace_identities(workspace_id),
        route_id TEXT NOT NULL,
        direction TEXT NOT NULL,
        draft_json TEXT NOT NULL,
        created_at_unix INTEGER NOT NULL
    ); CREATE INDEX IF NOT EXISTS idx_project_plans_workspace_sequence ON project_plans(workspace_id,sequence);")?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS project_todo_decisions (
        plan_id TEXT NOT NULL REFERENCES project_plans(plan_id),
        todo_index INTEGER NOT NULL CHECK(todo_index>=0 AND todo_index<8),
        device_id TEXT NOT NULL,
        operation_id TEXT NOT NULL,
        choice_json TEXT NOT NULL,
        created_at_unix INTEGER NOT NULL,
        PRIMARY KEY(plan_id,todo_index),
        UNIQUE(device_id,operation_id),
        FOREIGN KEY(device_id,operation_id) REFERENCES mobile_commands(device_id,operation_id)
    );
    CREATE TABLE IF NOT EXISTS project_todo_retries (
        plan_id TEXT NOT NULL REFERENCES project_plans(plan_id),
        todo_index INTEGER NOT NULL CHECK(todo_index>=0 AND todo_index<8),
        device_id TEXT NOT NULL, operation_id TEXT NOT NULL, choice_json TEXT NOT NULL,
        previous_device_id TEXT NOT NULL, previous_operation_id TEXT NOT NULL,
        created_at_unix INTEGER NOT NULL,
        PRIMARY KEY(device_id,operation_id), UNIQUE(previous_device_id,previous_operation_id),
        FOREIGN KEY(device_id,operation_id) REFERENCES mobile_commands(device_id,operation_id),
        FOREIGN KEY(previous_device_id,previous_operation_id) REFERENCES mobile_commands(device_id,operation_id)
    );
    CREATE INDEX IF NOT EXISTS idx_project_todo_retries_plan ON project_todo_retries(plan_id,todo_index);",
    )?;
    Ok(())
}
