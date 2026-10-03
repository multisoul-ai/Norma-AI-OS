use super::*;
use crate::migrations::open_app_connection;
use agent::MemoryPreferences;
use rusqlite::{params, OptionalExtension, TransactionBehavior};

const PREFERENCES: &str = "norma.memory.preferences";
const EPOCH: &str = "norma.memory.recall_epoch";

fn state(connection: &rusqlite::Connection) -> Result<(MemoryPreferences, u64), AdapterError> {
    let value = |key| {
        connection
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get::<_, String>(0)
            })
            .optional()
            .map_err(failure)
    };
    let preferences = value(PREFERENCES)?
        .map(|text| serde_json::from_str(&text).map_err(failure))
        .transpose()?
        .unwrap_or_default();
    let epoch = value(EPOCH)?
        .map(|text| text.parse::<u64>().map_err(failure))
        .transpose()?
        .unwrap_or(0);
    Ok((preferences, epoch))
}

impl MemoryAccess {
    pub fn recall_state(&self) -> Result<(MemoryPreferences, u64), AdapterError> {
        let mut connection = open_app_connection(self.paths.app_db_path()).map_err(failure)?;
        let transaction = connection.transaction().map_err(failure)?;
        state(&transaction)
    }
    pub fn preferences(&self) -> Result<MemoryPreferences, AdapterError> {
        self.recall_state().map(|state| state.0)
    }
    pub fn recall_epoch(&self) -> Result<u64, AdapterError> {
        self.recall_state().map(|state| state.1)
    }
    pub fn set_preferences(&mut self, preferences: MemoryPreferences) -> Result<(), AdapterError> {
        self.update_preferences(|value| *value = preferences)
            .map(|_| ())
    }
    pub fn set_recall_enabled(&mut self, enabled: bool) -> Result<MemoryPreferences, AdapterError> {
        self.update_preferences(|value| value.recall = enabled)
    }
    pub fn set_save_enabled(&mut self, enabled: bool) -> Result<MemoryPreferences, AdapterError> {
        self.update_preferences(|value| value.save = enabled)
    }
    fn update_preferences(
        &mut self,
        update: impl FnOnce(&mut MemoryPreferences),
    ) -> Result<MemoryPreferences, AdapterError> {
        let mut connection = open_app_connection(self.paths.app_db_path()).map_err(failure)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(failure)?;
        let (previous, mut epoch) = state(&transaction)?;
        let mut preferences = previous;
        update(&mut preferences);
        if previous.recall && !preferences.recall {
            epoch = epoch
                .checked_add(1)
                .ok_or_else(|| failure("记忆撤销代次已达上限"))?;
        }
        for (key, value) in [
            (
                PREFERENCES,
                serde_json::to_string(&preferences).map_err(failure)?,
            ),
            (EPOCH, epoch.to_string()),
        ] {
            transaction.execute("INSERT INTO settings(key, value, updated_at_unix) VALUES(?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at_unix = excluded.updated_at_unix", params![key, value, crate::current_unix_timestamp().map_err(failure)?]).map_err(failure)?;
        }
        transaction.commit().map_err(failure)?;
        if !preferences.recall {
            self.baselines.clear();
            self.read_entries.clear();
            self.continuations.clear();
        }
        Ok(preferences)
    }
}
