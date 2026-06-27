use crate::{current_unix_timestamp, migrations::open_app_connection, StorageResult};
use rusqlite::{params, OptionalExtension};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(path: PathBuf) -> Self {
        SettingsStore { path }
    }

    pub fn set_string(&self, key: &str, value: &str) -> StorageResult<()> {
        let connection = open_app_connection(&self.path)?;
        connection.execute(
            r#"
            INSERT INTO settings (key, value, updated_at_unix)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at_unix = excluded.updated_at_unix
            "#,
            params![key, value, current_unix_timestamp()?],
        )?;
        Ok(())
    }

    pub fn get_string(&self, key: &str) -> StorageResult<Option<String>> {
        let connection = open_app_connection(&self.path)?;
        Ok(connection
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn delete_string(&self, key: &str) -> StorageResult<()> {
        let connection = open_app_connection(&self.path)?;
        connection.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/unit/settings_store_tests.rs"]
mod tests;
