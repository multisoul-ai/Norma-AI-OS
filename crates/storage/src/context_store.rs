//! Immutable facts and their working-state checkpoint commit in one transaction.
//! All methods do IO and must be called off the GPUI foreground.
use crate::{migrations::open_app_connection, StorageError, StorageResult};
use agent::context::*;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::path::PathBuf;

#[derive(Clone)]
pub struct ContextStore {
    path: PathBuf,
}

impl ContextStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn find(
        &self,
        scope: &ContextScope,
        id: &str,
    ) -> StorageResult<Option<RecordedContextEvent>> {
        let record = read_event(&open_app_connection(&self.path)?, id)?;
        if record
            .as_ref()
            .is_some_and(|record| !record.reference.scope.readable_from(scope))
        {
            return Err(invalid("scope_mismatch"));
        }
        Ok(record)
    }

    /// Caller supplies authenticated source and scope. Success means durable commit.
    pub fn append(
        &self,
        event: &ContextEvent,
        expected_revision: Option<u64>,
    ) -> StorageResult<RecordedContextEvent> {
        let payload = serde_json::to_string(event)?;
        // A UTF-8 input byte may need six JSON bytes (e.g. \u0001). The domain
        // still enforces 32 KiB of original text; allow that expansion plus the
        // existing 64 KiB bound for source/scope/reference metadata.
        let encoded_limit = if matches!(event.payload, ContextPayload::UserInput { .. }) {
            32 * 1024 * 6 + 64 * 1024
        } else {
            64 * 1024
        };
        if !event.scope.is_valid()
            || event.event_id.trim().is_empty()
            || event.event_id.len() > 256
            || event.source.stream_id.trim().is_empty()
            || event.source.stream_id.len() > 256
            || event.source.sequence == 0
            || event.source.sequence > i64::MAX as u64
            || payload.len() > encoded_limit
        {
            return Err(invalid("invalid_event"));
        }
        let scope = serde_json::to_string(&event.scope)?;
        let mut conn = open_app_connection(&self.path)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(previous) = read_event(&tx, &event.event_id)? {
            return if previous.event == *event {
                Ok(previous)
            } else {
                Err(invalid("idempotency_conflict"))
            };
        }
        let origin = serde_json::to_value(event.source.origin)?;
        let latest: u64 = tx.query_row(
            "SELECT COALESCE(MAX(json_extract(payload,'$.source.sequence')),0) FROM shared_context_events WHERE scope=?1 AND json_extract(payload,'$.source.origin')=?2 AND json_extract(payload,'$.source.stream_id')=?3",
            params![scope, origin.as_str(), event.source.stream_id], |row| row.get(0))?;
        if event.source.sequence <= latest {
            return Err(invalid("out_of_order"));
        }
        for reference in &event.related {
            read_reference(&tx, &event.scope, reference)?;
        }
        let mut snapshot = read_snapshot(&tx, &event.scope, 0)?;
        let modifies_state = matches!(
            event.payload,
            ContextPayload::Constraint { .. }
                | ContextPayload::Goal { .. }
                | ContextPayload::RevokeConstraint { .. }
        );
        if (modifies_state && expected_revision.is_none())
            || expected_revision.is_some_and(|expected| expected != snapshot.revision)
        {
            return Err(invalid("stale_snapshot"));
        }
        let revision = snapshot
            .revision
            .checked_add(1)
            .filter(|value| *value <= i64::MAX as u64)
            .ok_or_else(|| invalid("revision_overflow"))?;
        let reference = ContextRef {
            scope: event.scope.clone(),
            event_id: event.event_id.clone(),
            revision,
        };
        let record = RecordedContextEvent {
            reference: reference.clone(),
            event: event.clone(),
        };
        snapshot.state = project_event(&snapshot.state, &record).map_err(invalid)?;
        tx.execute("INSERT INTO shared_context_events(event_id,scope,revision,payload) VALUES(?1,?2,?3,?4)", params![event.event_id, scope, revision, payload])?;
        tx.execute("INSERT INTO shared_context_state(scope,revision,payload) VALUES(?1,?2,?3) ON CONFLICT(scope) DO UPDATE SET revision=excluded.revision,payload=excluded.payload", params![scope, revision, serde_json::to_string(&snapshot.state)?])?;
        tx.commit()?;
        Ok(RecordedContextEvent {
            reference,
            event: event.clone(),
        })
    }

    /// One read transaction prevents a checkpoint from being paired with newer events.
    pub fn snapshot(&self, scope: &ContextScope, limit: usize) -> StorageResult<ContextSnapshot> {
        if !scope.is_valid() {
            return Err(invalid("invalid_scope"));
        }
        let mut conn = open_app_connection(&self.path)?;
        let tx = conn.transaction()?;
        read_snapshot(&tx, scope, limit.min(100))
    }

    pub fn read(
        &self,
        reader: &ContextScope,
        reference: &ContextRef,
    ) -> StorageResult<RecordedContextEvent> {
        if !reference.scope.readable_from(reader) {
            return Err(invalid("scope_mismatch"));
        }
        read_reference(&open_app_connection(&self.path)?, reader, reference)
    }
}

fn read_reference(
    conn: &Connection,
    reader: &ContextScope,
    reference: &ContextRef,
) -> StorageResult<RecordedContextEvent> {
    if !reference.scope.readable_from(reader) {
        return Err(invalid("scope_mismatch"));
    }
    let record = read_event(conn, &reference.event_id)?.ok_or_else(|| invalid("unavailable"))?;
    if record.reference != *reference {
        return Err(invalid("stale_snapshot"));
    }
    Ok(record)
}

fn read_snapshot(
    conn: &Connection,
    scope: &ContextScope,
    limit: usize,
) -> StorageResult<ContextSnapshot> {
    let key = serde_json::to_string(scope)?;
    let row: Option<(u64, String)> = conn
        .query_row(
            "SELECT revision,payload FROM shared_context_state WHERE scope=?1",
            [&key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (revision, state) = match row {
        Some((revision, json)) => (revision, serde_json::from_str(&json)?),
        None => (0, WorkingState::default()),
    };
    let mut query = conn.prepare("SELECT revision,payload FROM shared_context_events WHERE scope=?1 ORDER BY revision DESC LIMIT ?2")?;
    let rows = query.query_map(params![key, limit as i64], |row| {
        Ok((row.get::<_, u64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut events = Vec::new();
    for row in rows {
        let (revision, payload) = row?;
        events.push(recorded(revision, &payload)?);
    }
    events.reverse();
    Ok(ContextSnapshot {
        scope: scope.clone(),
        revision,
        state,
        events,
    })
}

fn read_event(conn: &Connection, id: &str) -> StorageResult<Option<RecordedContextEvent>> {
    let row = conn
        .query_row(
            "SELECT revision,payload FROM shared_context_events WHERE event_id=?1",
            [id],
            |row| Ok((row.get::<_, u64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    row.map(|(revision, json)| recorded(revision, &json))
        .transpose()
}

fn recorded(revision: u64, payload: &str) -> StorageResult<RecordedContextEvent> {
    let event: ContextEvent = serde_json::from_str(payload)?;
    Ok(RecordedContextEvent {
        reference: ContextRef {
            scope: event.scope.clone(),
            event_id: event.event_id.clone(),
            revision,
        },
        event,
    })
}

fn invalid(code: &str) -> StorageError {
    StorageError::InvalidContext(code.into())
}

pub(crate) fn migrate(conn: &Connection) -> StorageResult<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS shared_context_state(scope TEXT PRIMARY KEY NOT NULL,revision INTEGER NOT NULL,payload TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS shared_context_events(event_id TEXT PRIMARY KEY NOT NULL,scope TEXT NOT NULL,revision INTEGER NOT NULL,payload TEXT NOT NULL,UNIQUE(scope,revision));
        CREATE INDEX IF NOT EXISTS shared_context_source ON shared_context_events(scope,json_extract(payload,'$.source.origin'),json_extract(payload,'$.source.stream_id'));")?;
    Ok(())
}
