use crate::{RecentStore, SettingsStore, StorageError, StorageResult};
use std::path::{Path, PathBuf};

const CURRENT_APP_DATA_DIR_NAME: &str = "Norma OS";
const LEGACY_APP_DATA_DIR_NAME: &str = "Multisoul";
const APP_DB_FILENAME: &str = "app.sqlite";

#[derive(Clone, Debug)]
pub struct AppStore {
    path: PathBuf,
}

impl AppStore {
    pub fn new(path: PathBuf) -> Self {
        AppStore { path }
    }

    pub fn open_default() -> StorageResult<Self> {
        Ok(Self::new(migrate_default_app_db_path()?))
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
    Ok(app_data_paths_for_home(&home).current_db)
}

pub fn default_app_data_dir() -> StorageResult<PathBuf> {
    let home = dirs::home_dir().ok_or(StorageError::MissingHomeDirectory)?;
    Ok(app_data_paths_for_home(&home).current_dir)
}

pub fn legacy_app_data_dir() -> StorageResult<PathBuf> {
    let home = dirs::home_dir().ok_or(StorageError::MissingHomeDirectory)?;
    Ok(app_data_paths_for_home(&home).legacy_dir)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AppDataPaths {
    current_dir: PathBuf,
    legacy_dir: PathBuf,
    current_db: PathBuf,
    legacy_db: PathBuf,
}

fn app_data_paths_for_home(home: &Path) -> AppDataPaths {
    let app_support = home.join("Library").join("Application Support");
    let current_dir = app_support.join(CURRENT_APP_DATA_DIR_NAME);
    let legacy_dir = app_support.join(LEGACY_APP_DATA_DIR_NAME);
    AppDataPaths {
        current_db: current_dir.join(APP_DB_FILENAME),
        legacy_db: legacy_dir.join(APP_DB_FILENAME),
        current_dir,
        legacy_dir,
    }
}

fn migrate_default_app_db_path() -> StorageResult<PathBuf> {
    let home = dirs::home_dir().ok_or(StorageError::MissingHomeDirectory)?;
    migrate_default_app_db_path_from_home(&home)
}

fn migrate_default_app_db_path_from_home(home: &Path) -> StorageResult<PathBuf> {
    let paths = app_data_paths_for_home(home);
    if paths.current_db.exists() || !paths.legacy_db.exists() {
        return Ok(paths.current_db);
    }
    if let Some(parent) = paths.current_db.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&paths.legacy_db, &paths.current_db)?;
    Ok(paths.current_db)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AS-1: app data 默认路径应迁移到 Norma OS 目录，旧 Multisoul 目录只作为兼容来源。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   home = temp/home
    ///   current dir = home/Library/Application Support/Norma OS
    ///   legacy dir = home/Library/Application Support/Multisoul
    ///   db filename = app.sqlite
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. app_data_paths_for_home(home)
    ///   2. 读取 current_db 与 legacy_db
    ///   3. 检查新旧路径的目录名和文件名
    ///
    /// 预期结果：
    ///   - 正断言：current_db 位于 Norma OS/app.sqlite
    ///   - 正断言：legacy_db 位于 Multisoul/app.sqlite
    ///   - 负断言：current_db 不应继续位于旧 Multisoul 目录
    #[test]
    fn test_app_data_paths_use_norma_os_with_multisoul_legacy_path() {
        let temp = tempfile::tempdir().expect("test should create temp dir");
        let home = temp.path().join("home");

        let paths = app_data_paths_for_home(&home);

        assert_eq!(
            paths.current_dir,
            home.join("Library")
                .join("Application Support")
                .join("Norma OS"),
            "current app data dir 应位于 Norma OS"
        );
        assert_eq!(
            paths.current_db,
            home.join("Library")
                .join("Application Support")
                .join("Norma OS")
                .join("app.sqlite"),
            "current app db path 应位于 Norma OS 数据目录"
        );
        assert_eq!(
            paths.legacy_db,
            home.join("Library")
                .join("Application Support")
                .join("Multisoul")
                .join("app.sqlite"),
            "legacy app db path 应保留旧 Multisoul 数据目录用于迁移"
        );
        assert_ne!(
            paths.current_db, paths.legacy_db,
            "current app db path 不应继续指向旧 Multisoul 数据目录"
        );
    }

    /// AS-2: 新 Norma OS 数据库不存在但旧 Multisoul 数据库存在时，应复制旧库完成迁移。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   home = temp/home
    ///   legacy db = home/Library/Application Support/Multisoul/app.sqlite，内容为 b"legacy-db"
    ///   current db = home/Library/Application Support/Norma OS/app.sqlite，初始不存在
    ///   copied bytes = 9（"legacy-db" 的字节数）
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 创建 legacy db 文件并写入 b"legacy-db"
    ///   2. migrate_default_app_db_path_from_home(home)
    ///   3. 读取 current db 与 legacy db 内容
    ///
    /// 预期结果：
    ///   - 正断言：返回路径为 current db
    ///   - 正断言：current db 内容等于 b"legacy-db"
    ///   - 正断言：legacy db 仍存在，避免破坏用户旧数据
    ///   - 负断言：迁移后不应只返回 legacy db
    #[test]
    fn test_migrate_default_app_db_path_copies_legacy_db_without_deleting_it() {
        let temp = tempfile::tempdir().expect("test should create temp dir");
        let home = temp.path().join("home");
        let paths = app_data_paths_for_home(&home);
        std::fs::create_dir_all(
            paths
                .legacy_db
                .parent()
                .expect("legacy db should have parent"),
        )
        .expect("test should create legacy app data dir");
        std::fs::write(&paths.legacy_db, b"legacy-db").expect("test should write legacy db");

        let migrated =
            migrate_default_app_db_path_from_home(&home).expect("legacy app db migration 应成功");
        let current_bytes = std::fs::read(&paths.current_db).expect("current db 应已复制出来");
        let legacy_bytes = std::fs::read(&paths.legacy_db).expect("legacy db 应继续保留");

        assert_eq!(
            migrated, paths.current_db,
            "迁移后应返回 Norma OS current app db path"
        );
        assert_eq!(
            current_bytes, b"legacy-db",
            "current app db 应复制旧 Multisoul app.sqlite 内容"
        );
        assert_eq!(
            legacy_bytes, b"legacy-db",
            "迁移不应删除或清空旧 Multisoul app.sqlite"
        );
        assert_ne!(
            migrated, paths.legacy_db,
            "迁移成功后不应继续把旧 Multisoul db 当作默认路径"
        );
    }
}
