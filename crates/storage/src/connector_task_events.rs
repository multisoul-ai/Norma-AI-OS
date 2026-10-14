//! Persist genuine task state transitions; old snapshots deliberately have no invented history.
use crate::StorageResult;
use rusqlite::Connection;
pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS agent_dispatch_task_events(sequence INTEGER PRIMARY KEY AUTOINCREMENT,task_id INTEGER NOT NULL,occurred_at_unix INTEGER NOT NULL,state TEXT NOT NULL,FOREIGN KEY(task_id) REFERENCES agent_dispatch_tasks(id) ON DELETE CASCADE);
        CREATE INDEX IF NOT EXISTS connector_task_event_history ON agent_dispatch_task_events(task_id,sequence);
        CREATE TRIGGER IF NOT EXISTS connector_task_created AFTER INSERT ON agent_dispatch_tasks BEGIN INSERT INTO agent_dispatch_task_events(task_id,occurred_at_unix,state) VALUES(NEW.id,NEW.created_at_unix,NEW.state); END;
        CREATE TRIGGER IF NOT EXISTS connector_task_state_changed AFTER UPDATE OF state ON agent_dispatch_tasks WHEN OLD.state <> NEW.state BEGIN INSERT INTO agent_dispatch_task_events(task_id,occurred_at_unix,state) VALUES(NEW.id,NEW.updated_at_unix,NEW.state); END;")?;
    Ok(())
}
