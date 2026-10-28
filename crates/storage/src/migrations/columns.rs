use crate::StorageResult;
use rusqlite::Connection;

pub(crate) fn ensure_column(
    connection: &Connection,
    table: &'static str,
    column: &'static str,
    definition: &'static str,
) -> StorageResult<()> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name=?2)",
        [table, column],
        |row| row.get(0),
    )?;
    if exists {
        return Ok(());
    }
    if connection.is_autocommit() {
        // A concurrent opener may add the column while we wait. Recheck under the
        // write lock; callers already inside a migration transaction retain it.
        let transaction = rusqlite::Transaction::new_unchecked(
            connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        ensure_column(&transaction, table, column, definition)?;
        transaction.commit()?;
        return Ok(());
    }

    connection.execute(&format!("ALTER TABLE {table} ADD COLUMN {definition}"), [])?;
    Ok(())
}
