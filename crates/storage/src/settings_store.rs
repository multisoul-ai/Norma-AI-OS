use crate::{current_unix_timestamp, migrations::open_app_connection, StorageResult};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(path: PathBuf) -> Self {
        SettingsStore { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
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

    pub fn get_or_insert_string(&self, key: &str, value: &str) -> StorageResult<String> {
        let connection = open_app_connection(&self.path)?;
        connection.execute(
            "INSERT OR IGNORE INTO settings (key, value, updated_at_unix) VALUES (?1, ?2, ?3)",
            params![key, value, current_unix_timestamp()?],
        )?;
        Ok(connection.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )?)
    }

    pub fn delete_string(&self, key: &str) -> StorageResult<()> {
        let connection = open_app_connection(&self.path)?;
        connection.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        Ok(())
    }

    pub fn apply_string_changes(&self, changes: &[(String, Option<String>)]) -> StorageResult<()> {
        let mut connection = open_app_connection(&self.path)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        apply_changes(&transaction, changes)?;
        transaction.commit()?;
        Ok(())
    }

    /// Applies the whole batch only when the guard still matches, returning its resulting value.
    pub fn apply_string_changes_if_current(
        &self,
        key: &str,
        expected: Option<&str>,
        changes: &[(String, Option<String>)],
    ) -> StorageResult<(bool, Option<String>)> {
        let mut connection = open_app_connection(&self.path)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let read = || -> StorageResult<Option<String>> {
            Ok(transaction
                .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                    row.get(0)
                })
                .optional()?)
        };
        let current = read()?;
        if current.as_deref() != expected {
            return Ok((false, current));
        }
        apply_changes(&transaction, changes)?;
        let current = read()?;
        transaction.commit()?;
        Ok((true, current))
    }
}

fn apply_changes(
    transaction: &rusqlite::Transaction<'_>,
    changes: &[(String, Option<String>)],
) -> StorageResult<()> {
    for (key, value) in changes {
        match value {
            Some(value) => {
                transaction.execute(
                    r#"
                        INSERT INTO settings (key, value, updated_at_unix)
                        VALUES (?1, ?2, ?3)
                        ON CONFLICT(key) DO UPDATE SET
                            value = excluded.value,
                            updated_at_unix = excluded.updated_at_unix
                        "#,
                    params![key, value, current_unix_timestamp()?],
                )?;
            }
            None => {
                transaction.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/settings_store_tests.rs"]
mod tests;
