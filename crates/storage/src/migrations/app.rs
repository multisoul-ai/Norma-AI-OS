use crate::StorageResult;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};

// Advance this revision when a new migration batch needs the cold initialization transaction.
const REVISION: i64 = 1;

pub(super) fn migrate(connection: &Connection) -> StorageResult<()> {
    if initialized(connection)? {
        // Keep guarded legacy checks on warm opens; the marker must not hide later legacy changes.
        return super::migrate_app_schema(connection);
    }
    with_immediate(connection, |connection| {
        if initialized(connection)? {
            return Ok(());
        }
        super::migrate_app_schema(connection)?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS app_migration_state (
                id INTEGER PRIMARY KEY CHECK(id=1), revision INTEGER NOT NULL);",
        )?;
        connection.execute(
            "INSERT INTO app_migration_state(id,revision) VALUES(1,?1)
             ON CONFLICT(id) DO UPDATE SET revision=excluded.revision
             WHERE app_migration_state.revision<excluded.revision",
            [REVISION],
        )?;
        Ok(())
    })
}

fn initialized(connection: &Connection) -> StorageResult<bool> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='app_migration_state')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(false);
    }
    let revision: Option<i64> = connection
        .query_row(
            "SELECT revision FROM app_migration_state WHERE id=1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(revision.is_some_and(|revision| revision >= REVISION))
}

/// Reuse the enclosing cold migration transaction; standalone legacy upgrades still lock first.
pub(crate) fn with_immediate<T>(
    connection: &Connection,
    run: impl FnOnce(&Connection) -> StorageResult<T>,
) -> StorageResult<T> {
    if !connection.is_autocommit() {
        return run(connection);
    }
    let transaction = Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;
    let result = run(&transaction)?;
    transaction.commit()?;
    Ok(result)
}
