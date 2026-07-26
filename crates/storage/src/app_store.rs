use crate::{DatabaseMode, DatabasePaths, RecentStore, SettingsStore, StorageError, StorageResult};
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

    pub fn open(paths: &DatabasePaths) -> StorageResult<Self> {
        let path = match paths.mode() {
            DatabaseMode::Production => migrate_production_app_db_path(paths.app_db_path())?,
            DatabaseMode::Test => paths.app_db_path().to_path_buf(),
        };
        Ok(Self::new(path))
    }

    pub fn initialize(paths: &DatabasePaths) -> StorageResult<Self> {
        let store = Self::open(paths).map_err(|source| initialization_error(paths, source))?;
        store
            .migrate()
            .map_err(|source| initialization_error(paths, source))?;
        Ok(store)
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

fn initialization_error(paths: &DatabasePaths, source: StorageError) -> StorageError {
    StorageError::DatabaseInitialization {
        mode: paths.mode(),
        path: paths.app_db_path().to_path_buf(),
        source: Box::new(source),
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
    migrate_app_db_path(paths.current_db, paths.legacy_db)
}

fn migrate_production_app_db_path(current_db: &Path) -> StorageResult<PathBuf> {
    let app_support = current_db
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| StorageError::InvalidAppDatabasePath(current_db.to_path_buf()))?;
    let legacy_db = app_support
        .join(LEGACY_APP_DATA_DIR_NAME)
        .join(APP_DB_FILENAME);
    migrate_app_db_path(current_db.to_path_buf(), legacy_db)
}

fn migrate_app_db_path(current_db: PathBuf, legacy_db: PathBuf) -> StorageResult<PathBuf> {
    if current_db.exists() || !legacy_db.exists() {
        return Ok(current_db);
    }
    if let Some(parent) = current_db.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(legacy_db, &current_db)?;
    Ok(current_db)
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

    /// AS-3：Test AppStore 初始化必须创建空测试库，绝不复制旧 Production 数据。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   home = temp/home
    ///   legacy Production DB = Multisoul/app.sqlite，内容为 9 bytes 的 `legacy-db`
    ///   current Production DB = Norma OS/app.sqlite，初始不存在
    ///   Test DB = Norma OS Test/app.sqlite，初始不存在
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 写入 legacy Production 哨兵文件。
    ///   2. 用 `DatabasePaths::local_test_for_home` 打开 AppStore。
    ///   3. 执行 Test DB migration 创建全新 SQLite Schema。
    ///   4. 检查 Test、current Production 与 legacy Production 三个路径。
    ///
    /// 预期结果：
    ///   - 正断言：AppStore 和新建文件都指向 Test DB。
    ///   - 正断言：legacy Production 哨兵保持原样。
    ///   - 负断言：current Production DB 不得被创建，Test DB 不得等于 legacy 内容。
    #[test]
    fn test_app_store_open_with_test_paths_never_copies_legacy_production_db() {
        let temp = tempfile::tempdir().expect("测试应创建隔离 HOME");
        let home = temp.path().join("home");
        let production_paths = app_data_paths_for_home(&home);
        std::fs::create_dir_all(
            production_paths
                .legacy_db
                .parent()
                .expect("legacy Production DB 应有父目录"),
        )
        .expect("测试应创建 legacy Production 数据目录");
        std::fs::write(&production_paths.legacy_db, b"legacy-db")
            .expect("测试应写入 legacy Production 哨兵");
        let test_paths = crate::DatabasePaths::local_test_for_home(&home);

        let store = AppStore::open(&test_paths).expect("Test AppStore 应成功初始化");
        store
            .migrate()
            .expect("Test DB 应成功执行 Schema migration");

        assert_eq!(
            store.path(),
            test_paths.app_db_path(),
            "Test AppStore 必须使用 DatabasePaths 提供的测试路径"
        );
        assert!(
            test_paths.app_db_path().is_file(),
            "Test migration 后必须创建独立的 app.sqlite"
        );
        assert_eq!(
            std::fs::read(&production_paths.legacy_db).expect("legacy Production 哨兵应保持可读"),
            b"legacy-db",
            "Test 初始化不得修改 legacy Production 数据"
        );
        assert!(
            !production_paths.current_db.exists(),
            "Test 初始化不得创建或迁移 current Production DB"
        );
        assert_ne!(
            std::fs::read(test_paths.app_db_path()).expect("Test DB 应可读取"),
            b"legacy-db",
            "Test DB 不得复制 legacy Production 数据"
        );
    }

    /// AS-4：Test 数据库目标不可初始化时必须返回带模式和路径的错误，禁止回退。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   blocked parent = temp/blocked，类型为普通文件而非目录
    ///   Test app DB = temp/blocked/app.sqlite
    ///   Production fallback candidate = temp/production/app.sqlite，初始不存在
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 创建普通文件占用 Test app DB 的父路径。
    ///   2. 用显式 Test DatabasePaths 初始化并迁移 AppStore。
    ///   3. 读取错误文本并检查 Production 候选文件。
    ///
    /// 预期结果：
    ///   - 正断言：初始化返回错误，且错误包含 Test 模式和完整目标路径。
    ///   - 负断言：失败后不得创建 Production fallback 数据库。
    #[test]
    fn test_app_store_initialization_failure_reports_test_path_without_fallback() {
        let temp = tempfile::tempdir().expect("测试应创建隔离根目录");
        let blocked_parent = temp.path().join("blocked");
        std::fs::write(&blocked_parent, b"not-a-directory")
            .expect("测试应创建阻塞父目录的普通文件");
        let test_db = blocked_parent.join("app.sqlite");
        let production_fallback = temp.path().join("production").join("app.sqlite");
        let test_paths = crate::DatabasePaths::isolated_test_database(&test_db);

        let error =
            AppStore::initialize(&test_paths).expect_err("不可写入的 Test 目标必须明确初始化失败");
        let message = error.to_string();

        assert!(
            message.contains("Test"),
            "初始化错误必须包含当前 Test 模式，实际: {message}"
        );
        assert!(
            message.contains(test_db.to_string_lossy().as_ref()),
            "初始化错误必须包含目标数据库完整路径，实际: {message}"
        );
        assert!(
            !production_fallback.exists(),
            "Test 初始化失败后不得创建 Production fallback app.sqlite"
        );
    }
}
