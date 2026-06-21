use crate::{RecentStore, SettingsStore, StorageError, StorageResult};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct AppStore {
    path: PathBuf,
}

impl AppStore {
    pub fn new(path: PathBuf) -> Self {
        AppStore { path }
    }

    pub fn open_default() -> StorageResult<Self> {
        Ok(Self::new(default_app_db_path()?))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn migrate(&self) -> StorageResult<()> {
        let _connection = crate::migrations::open_app_connection(&self.path)?;
        Ok(())
    }

    pub fn recent_store(&self) -> RecentStore {
        RecentStore::new(self.path.clone())
    }

    pub fn settings_store(&self) -> SettingsStore {
        SettingsStore::new(self.path.clone())
    }
}

pub fn default_app_db_path() -> StorageResult<PathBuf> {
    let home = dirs::home_dir().ok_or(StorageError::MissingHomeDirectory)?;
    Ok(home
        .join("Library")
        .join("Application Support")
        .join("Multisoul")
        .join("app.sqlite"))
}
