use crate::{current_unix_timestamp, migrations::open_app_connection, StorageResult};
use rusqlite::{params, OptionalExtension, Row, TransactionBehavior};
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewOrchestrationTaskRoute {
    pub task_ref: String,
    pub operation_id: String,
    pub request_fingerprint: String,
    pub source_workspace_id: String,
    pub source_definition_id: i64,
    pub source_provider: String,
    pub source_node_id_at_dispatch: u64,
    pub target_workspace_id: String,
    pub target_definition_id: i64,
    pub parent_task_ref: Option<String>,
    pub root_task_ref: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrchestrationTaskRouteCompletion<'a> {
    pub operation_id: &'a str,
    pub target_task_ref: &'a str,
    pub target_definition_id: i64,
    pub target_name: &'a str,
    pub target_node_id_at_dispatch: u64,
    pub state: &'a str,
    pub tracking: &'a str,
    pub result_json: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestrationTaskRoute {
    pub task_ref: String,
    pub operation_id: String,
    pub request_fingerprint: String,
    pub source_workspace_id: String,
    pub source_definition_id: i64,
    pub source_provider: String,
    pub source_node_id_at_dispatch: u64,
    pub target_workspace_id: String,
    pub parent_task_ref: Option<String>,
    pub root_task_ref: String,
    pub target_task_ref: Option<String>,
    pub target_definition_id: Option<i64>,
    pub target_name: Option<String>,
    pub target_node_id_at_dispatch: Option<u64>,
    pub state: String,
    pub tracking: String,
    pub result_json: Option<String>,
    pub task_result_json: Option<String>,
    pub error_code: Option<String>,
    pub created_at_unix: u64,
    pub updated_at_unix: u64,
    pub terminal_at_unix: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrchestrationTaskReservation {
    New(OrchestrationTaskRoute),
    Replay(OrchestrationTaskRoute),
    Conflict,
    Rejected(OrchestrationLineageRejection),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrchestrationLineageRejection {
    SelfDispatch,
    Cycle,
    DescendantBudgetExceeded { limit: usize },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrchestrationTaskOperationReservation {
    New,
    Replay(Option<String>),
    Conflict,
}

#[derive(Clone, Debug)]
pub struct OrchestrationTaskRouteStore {
    path: PathBuf,
}

impl OrchestrationTaskRouteStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn reserve(
        &self,
        route: &NewOrchestrationTaskRoute,
    ) -> StorageResult<OrchestrationTaskReservation> {
        let mut connection = open_app_connection(&self.path)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = select_by_operation(&transaction, &route.operation_id)?;
        if let Some(existing) = existing {
            return if existing.request_fingerprint == route.request_fingerprint {
                Ok(OrchestrationTaskReservation::Replay(existing))
            } else {
                Ok(OrchestrationTaskReservation::Conflict)
            };
        }
        if let Some(rejection) = validate_lineage(&transaction, route)? {
            return Ok(OrchestrationTaskReservation::Rejected(rejection));
        }

        let now = current_unix_timestamp()?;
        transaction.execute(
            "INSERT INTO orchestration_task_routes (
                task_ref, operation_id, request_fingerprint,
                source_workspace_id, source_definition_id, source_provider,
                source_node_id_at_dispatch, target_workspace_id,
                target_definition_id, parent_task_ref, root_task_ref,
                state, tracking, created_at_unix, updated_at_unix
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                       'accepted', 'checking', ?12, ?12)",
            params![
                route.task_ref,
                route.operation_id,
                route.request_fingerprint,
                route.source_workspace_id,
                route.source_definition_id,
                route.source_provider,
                route.source_node_id_at_dispatch,
                route.target_workspace_id,
                route.target_definition_id,
                route.parent_task_ref,
                route.root_task_ref,
                now
            ],
        )?;
        let stored = select_by_operation(&transaction, &route.operation_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        transaction.commit()?;
        Ok(OrchestrationTaskReservation::New(stored))
    }

    pub fn complete(&self, completion: OrchestrationTaskRouteCompletion<'_>) -> StorageResult<()> {
        let connection = open_app_connection(&self.path)?;
        let now = current_unix_timestamp()?;
        let terminal_at = is_terminal_state(completion.state).then_some(now);
        let changed = connection.execute(
            "UPDATE orchestration_task_routes
             SET target_task_ref = ?1, target_definition_id = ?2,
                 target_name = ?3, target_node_id_at_dispatch = ?4,
                 state = ?5, tracking = ?6,
                 result_json = ?7, updated_at_unix = ?8, terminal_at_unix = ?9
             WHERE operation_id = ?10",
            params![
                completion.target_task_ref,
                completion.target_definition_id,
                completion.target_name,
                completion.target_node_id_at_dispatch,
                completion.state,
                completion.tracking,
                completion.result_json,
                now,
                terminal_at,
                completion.operation_id,
            ],
        )?;
        if changed == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows.into());
        }
        Ok(())
    }

    pub fn update_status(&self, task_ref: &str, state: &str, tracking: &str) -> StorageResult<()> {
        self.update_status_with_result(task_ref, state, tracking, None)
    }

    pub fn update_status_with_result(
        &self,
        task_ref: &str,
        state: &str,
        tracking: &str,
        task_result_json: Option<&str>,
    ) -> StorageResult<()> {
        let connection = open_app_connection(&self.path)?;
        let now = current_unix_timestamp()?;
        let terminal_at = is_terminal_state(state).then_some(now);
        let changed = connection.execute(
            "UPDATE orchestration_task_routes
             SET state = ?1, tracking = ?2,
                 task_result_json = COALESCE(?3, task_result_json),
                 updated_at_unix = ?4, terminal_at_unix = COALESCE(terminal_at_unix, ?5)
             WHERE task_ref = ?6",
            params![
                state,
                tracking,
                task_result_json,
                now,
                terminal_at,
                task_ref
            ],
        )?;
        if changed == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows.into());
        }
        Ok(())
    }

    pub fn complete_error(
        &self,
        operation_id: &str,
        error_code: &str,
        result_json: &str,
    ) -> StorageResult<()> {
        let connection = open_app_connection(&self.path)?;
        let now = current_unix_timestamp()?;
        let changed = connection.execute(
            "UPDATE orchestration_task_routes
             SET state = 'failed', tracking = 'checking', result_json = ?1,
                 error_code = ?2, updated_at_unix = ?3, terminal_at_unix = ?3
             WHERE operation_id = ?4",
            params![result_json, error_code, now, operation_id],
        )?;
        if changed == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows.into());
        }
        Ok(())
    }

    pub fn reserve_operation(
        &self,
        operation_id: &str,
        request_fingerprint: &str,
    ) -> StorageResult<OrchestrationTaskOperationReservation> {
        let mut connection = open_app_connection(&self.path)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = transaction
            .query_row(
                "SELECT request_fingerprint, result_json
                 FROM orchestration_task_operations WHERE operation_id = ?1",
                [operation_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()?;
        if let Some((existing_fingerprint, result_json)) = existing {
            return if existing_fingerprint == request_fingerprint {
                Ok(OrchestrationTaskOperationReservation::Replay(result_json))
            } else {
                Ok(OrchestrationTaskOperationReservation::Conflict)
            };
        }
        let now = current_unix_timestamp()?;
        transaction.execute(
            "INSERT INTO orchestration_task_operations (
                operation_id, request_fingerprint, created_at_unix, updated_at_unix
             ) VALUES (?1, ?2, ?3, ?3)",
            params![operation_id, request_fingerprint, now],
        )?;
        transaction.commit()?;
        Ok(OrchestrationTaskOperationReservation::New)
    }

    pub fn complete_operation(&self, operation_id: &str, result_json: &str) -> StorageResult<()> {
        let connection = open_app_connection(&self.path)?;
        let now = current_unix_timestamp()?;
        let changed = connection.execute(
            "UPDATE orchestration_task_operations
             SET result_json = ?1, updated_at_unix = ?2
             WHERE operation_id = ?3",
            params![result_json, now, operation_id],
        )?;
        if changed == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows.into());
        }
        Ok(())
    }

    pub fn get(&self, task_ref: &str) -> StorageResult<Option<OrchestrationTaskRoute>> {
        let connection = open_app_connection(&self.path)?;
        select_by_task_ref(&connection, task_ref)
    }

    pub fn find_by_target_task(
        &self,
        workspace_id: &str,
        target_task_ref: &str,
    ) -> StorageResult<Option<OrchestrationTaskRoute>> {
        let connection = open_app_connection(&self.path)?;
        connection
            .query_row(
                &format!(
                    "{} WHERE target_workspace_id = ?1 AND target_task_ref = ?2
                     ORDER BY created_at_unix DESC LIMIT 1",
                    route_select_prefix()
                ),
                params![workspace_id, target_task_ref],
                decode_route,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn reconcile_non_terminal_as_interrupted(&self) -> StorageResult<usize> {
        let connection = open_app_connection(&self.path)?;
        let now = current_unix_timestamp()?;
        connection
            .execute(
                "UPDATE orchestration_task_routes
                 SET state = 'interrupted', updated_at_unix = ?1, terminal_at_unix = ?1
                 WHERE state NOT IN ('done', 'failed', 'cancelled', 'interrupted')",
                [now],
            )
            .map_err(Into::into)
    }

    pub fn interrupt_target_workspace(&self, workspace_id: &str) -> StorageResult<usize> {
        let connection = open_app_connection(&self.path)?;
        let now = current_unix_timestamp()?;
        connection
            .execute(
                "UPDATE orchestration_task_routes
                 SET state = 'interrupted', updated_at_unix = ?1, terminal_at_unix = ?1
                 WHERE target_workspace_id = ?2
                   AND state NOT IN ('done', 'failed', 'cancelled', 'interrupted')",
                params![now, workspace_id],
            )
            .map_err(Into::into)
    }
}

fn validate_lineage(
    connection: &rusqlite::Connection,
    route: &NewOrchestrationTaskRoute,
) -> StorageResult<Option<OrchestrationLineageRejection>> {
    if route.source_workspace_id == route.target_workspace_id
        && route.source_definition_id == route.target_definition_id
    {
        return Ok(Some(OrchestrationLineageRejection::SelfDispatch));
    }
    let Some(parent_task_ref) = route.parent_task_ref.as_deref() else {
        return Ok(None);
    };
    let mut cursor = Some(parent_task_ref.to_string());
    let mut remaining_hops = agent::MAX_UNFINISHED_DESCENDANTS.saturating_add(1);
    while let Some(task_ref) = cursor {
        if remaining_hops == 0 {
            return Ok(Some(OrchestrationLineageRejection::Cycle));
        }
        remaining_hops = remaining_hops.saturating_sub(1);
        let ancestor = select_by_task_ref(connection, &task_ref)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        let target_matches_source = route.target_workspace_id == ancestor.source_workspace_id
            && route.target_definition_id == ancestor.source_definition_id;
        let target_matches_target = route.target_workspace_id == ancestor.target_workspace_id
            && ancestor.target_definition_id == Some(route.target_definition_id);
        if target_matches_source || target_matches_target {
            return Ok(Some(OrchestrationLineageRejection::Cycle));
        }
        cursor = ancestor.parent_task_ref;
    }
    let unfinished_descendants = connection.query_row(
        "SELECT COUNT(*) FROM orchestration_task_routes
         WHERE root_task_ref = ?1 AND parent_task_ref IS NOT NULL
           AND state NOT IN ('done', 'failed', 'cancelled', 'interrupted')",
        [&route.root_task_ref],
        |row| row.get::<_, usize>(0),
    )?;
    if unfinished_descendants >= agent::MAX_UNFINISHED_DESCENDANTS {
        return Ok(Some(
            OrchestrationLineageRejection::DescendantBudgetExceeded {
                limit: agent::MAX_UNFINISHED_DESCENDANTS,
            },
        ));
    }
    Ok(None)
}

fn select_by_operation(
    connection: &rusqlite::Connection,
    operation_id: &str,
) -> StorageResult<Option<OrchestrationTaskRoute>> {
    select_route(connection, "operation_id", operation_id)
}

fn select_by_task_ref(
    connection: &rusqlite::Connection,
    task_ref: &str,
) -> StorageResult<Option<OrchestrationTaskRoute>> {
    select_route(connection, "task_ref", task_ref)
}

fn select_route(
    connection: &rusqlite::Connection,
    column: &str,
    value: &str,
) -> StorageResult<Option<OrchestrationTaskRoute>> {
    let sql = format!("{} WHERE {column} = ?1", route_select_prefix());
    connection
        .query_row(&sql, [value], decode_route)
        .optional()
        .map_err(Into::into)
}

fn route_select_prefix() -> &'static str {
    "SELECT task_ref, operation_id, request_fingerprint,
            source_workspace_id, source_definition_id, source_provider,
            source_node_id_at_dispatch, target_workspace_id,
            parent_task_ref, root_task_ref, target_task_ref,
            target_definition_id, target_name, target_node_id_at_dispatch,
            state, tracking, result_json, task_result_json, error_code,
            created_at_unix, updated_at_unix, terminal_at_unix
     FROM orchestration_task_routes"
}

fn decode_route(row: &Row<'_>) -> rusqlite::Result<OrchestrationTaskRoute> {
    let task_ref = row.get::<_, String>(0)?;
    Ok(OrchestrationTaskRoute {
        task_ref: task_ref.clone(),
        operation_id: row.get(1)?,
        request_fingerprint: row.get(2)?,
        source_workspace_id: row.get(3)?,
        source_definition_id: row.get(4)?,
        source_provider: row.get(5)?,
        source_node_id_at_dispatch: row.get(6)?,
        target_workspace_id: row.get(7)?,
        parent_task_ref: row.get(8)?,
        root_task_ref: row.get::<_, Option<String>>(9)?.unwrap_or(task_ref),
        target_task_ref: row.get(10)?,
        target_definition_id: row.get(11)?,
        target_name: row.get(12)?,
        target_node_id_at_dispatch: row.get(13)?,
        state: row.get(14)?,
        tracking: row.get(15)?,
        result_json: row.get(16)?,
        task_result_json: row.get(17)?,
        error_code: row.get(18)?,
        created_at_unix: row.get(19)?,
        updated_at_unix: row.get(20)?,
        terminal_at_unix: row.get(21)?,
    })
}

fn is_terminal_state(state: &str) -> bool {
    matches!(state, "done" | "failed" | "cancelled" | "interrupted")
}

#[cfg(test)]
#[path = "../tests/unit/orchestration_task_route_store_tests.rs"]
mod tests;
