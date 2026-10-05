//! Desktop definitions live at project scope; audit rows belong to an individual canvas.
use crate::{StorageError, StorageResult};
use rusqlite::Connection;

pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    let transaction = connection.unchecked_transaction()?;
    for table in ["agent_dispatch_tasks", "agent_conversation_tails"] {
        let has_local_reference: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_list(?1) WHERE \"table\"='agent_definitions')",
            [table], |row| row.get(0),
        )?;
        if !has_local_reference {
            continue;
        }
        let schema: String = transaction.query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |r| r.get(0),
        )?;
        // Only the exact canonical constraint is removed; every existing column and index is preserved.
        let reference = "FOREIGN KEY (definition_id) REFERENCES agent_definitions(id)";
        let start = schema.find(reference).ok_or_else(|| {
            StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "unknown desktop audit definition constraint",
            ))
        })?;
        let comma = schema[..start]
            .trim_end()
            .strip_suffix(',')
            .ok_or_else(|| {
                StorageError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "invalid desktop audit constraint separator",
                ))
            })?;
        let new_schema = format!("{}{}", comma, &schema[start + reference.len()..]);
        let indexes = transaction.prepare("SELECT sql FROM sqlite_master WHERE type='index' AND tbl_name=?1 AND sql IS NOT NULL")?
            .query_map([table], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
        let columns = transaction
            .prepare("SELECT name FROM pragma_table_info(?1) ORDER BY cid")?
            .query_map([table], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let columns = columns
            .iter()
            .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(",");
        let old = format!("{table}_before_definition_split");
        transaction.execute_batch(&format!("ALTER TABLE {table} RENAME TO {old}; {new_schema}; INSERT INTO {table} ({columns}) SELECT {columns} FROM {old}; DROP TABLE {old};"))?;
        for index in indexes {
            transaction.execute_batch(&index)?;
        }
    }
    transaction.commit()?;
    Ok(())
}
