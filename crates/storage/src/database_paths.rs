use crate::{StorageError, StorageResult};
use std::path::{Path, PathBuf};

const APP_DB_FILENAME: &str = "app.sqlite";
const PRODUCTION_APP_DATA_DIR_NAME: &str = "Norma OS";
const TEST_APP_DATA_DIR_NAME: &str = "Norma OS Test";
const PRODUCTION_PROJECT_DB_FILENAME: &str = "project.sqlite";
const TEST_PROJECT_DB_FILENAME: &str = "project.test.sqlite";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DatabaseMode {
    Production,
    Test,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatabasePaths {
    mode: DatabaseMode,
    app_db_path: PathBuf,
    project_db_filename: &'static str,
}

impl DatabasePaths {
    pub fn production() -> StorageResult<Self> {
        let home = dirs::home_dir().ok_or(StorageError::MissingHomeDirectory)?;
        Ok(Self::production_for_home(&home))
    }

    pub fn production_for_home(home: &Path) -> Self {
        Self::for_home(
            DatabaseMode::Production,
            home,
            PRODUCTION_APP_DATA_DIR_NAME,
            PRODUCTION_PROJECT_DB_FILENAME,
        )
    }

    pub fn local_test_for_home(home: &Path) -> Self {
        Self::for_home(
            DatabaseMode::Test,
            home,
            TEST_APP_DATA_DIR_NAME,
            TEST_PROJECT_DB_FILENAME,
        )
    }

    pub fn local_test() -> StorageResult<Self> {
        let home = dirs::home_dir().ok_or(StorageError::MissingHomeDirectory)?;
        Ok(Self::local_test_for_home(&home))
    }

    pub fn isolated_test(data_root: &Path) -> Self {
        Self::isolated_test_database(&data_root.join(APP_DB_FILENAME))
    }

    pub fn isolated_test_database(app_db_path: &Path) -> Self {
        Self {
            mode: DatabaseMode::Test,
            app_db_path: app_db_path.to_path_buf(),
            project_db_filename: TEST_PROJECT_DB_FILENAME,
        }
    }

    pub fn mode(&self) -> DatabaseMode {
        self.mode
    }

    pub fn app_db_path(&self) -> &Path {
        &self.app_db_path
    }

    pub fn project_db_path(&self, workspace: &Path) -> PathBuf {
        workspace.join(".multisoul").join(self.project_db_filename)
    }

    fn for_home(
        mode: DatabaseMode,
        home: &Path,
        app_data_dir_name: &str,
        project_db_filename: &'static str,
    ) -> Self {
        let app_db_path = home
            .join("Library")
            .join("Application Support")
            .join(app_data_dir_name)
            .join(APP_DB_FILENAME);
        Self {
            mode,
            app_db_path,
            project_db_filename,
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/database_paths_tests.rs"]
mod tests;
