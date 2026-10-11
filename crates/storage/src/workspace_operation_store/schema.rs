use crate::StorageResult;
use rusqlite::Connection;

pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS workspace_operations (
        origin_key TEXT NOT NULL,operation_id TEXT NOT NULL,workspace_id TEXT NOT NULL,
        source_json TEXT NOT NULL,authorization_json TEXT NOT NULL,original_request_fingerprint TEXT NOT NULL,request_fingerprint TEXT NOT NULL,accepted_at_ms INTEGER NOT NULL,
        PRIMARY KEY(origin_key,operation_id));
        CREATE TABLE IF NOT EXISTS workspace_operation_assignments (
        assignment_id TEXT PRIMARY KEY NOT NULL,origin_key TEXT NOT NULL,operation_id TEXT NOT NULL,
        workspace_id TEXT NOT NULL,ordinal INTEGER NOT NULL,prepared_json TEXT NOT NULL,
        receipt_json TEXT NOT NULL,input_phase TEXT NOT NULL CHECK(input_phase IN ('queued','armed','body_written','enter_confirmed')),
        accepted_at_ms INTEGER NOT NULL,observed_at_ms INTEGER NOT NULL,
        FOREIGN KEY(origin_key,operation_id) REFERENCES workspace_operations(origin_key,operation_id),
        UNIQUE(origin_key,operation_id,ordinal));
        CREATE TABLE IF NOT EXISTS workspace_operation_events (
        sequence INTEGER PRIMARY KEY AUTOINCREMENT,assignment_id TEXT NOT NULL,
        origin_key TEXT NOT NULL,operation_id TEXT NOT NULL,workspace_id TEXT NOT NULL,
        dedupe_key TEXT UNIQUE NOT NULL,evidence TEXT NOT NULL,receipt_json TEXT NOT NULL,observed_at_ms INTEGER NOT NULL,
        FOREIGN KEY(assignment_id) REFERENCES workspace_operation_assignments(assignment_id));
        CREATE INDEX IF NOT EXISTS workspace_operation_events_scope ON workspace_operation_events(workspace_id,sequence);
        CREATE INDEX IF NOT EXISTS workspace_operation_assignments_scope ON workspace_operation_assignments(workspace_id,assignment_id);")?;
    Ok(())
}
