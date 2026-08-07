//! App-layer workspace state: recent list loading, current root, and user-facing errors.

use std::path::{Path, PathBuf};
use storage::RecentStore;
use workspace::{RecentWorkspaces, WorkspaceSource};

pub struct WorkspaceState {
    pub recent: RecentWorkspaces,
    pub current_root: Option<PathBuf>,
    pub error: Option<String>,
    storage: RecentStore,
}

impl WorkspaceState {
    pub fn load_from_storage(storage: RecentStore) -> Self {
        match storage.load() {
            Ok(recent) => WorkspaceState {
                recent,
                current_root: None,
                error: None,
                storage,
            },
            Err(error) => WorkspaceState {
                recent: RecentWorkspaces::default(),
                current_root: None,
                error: Some(format!("Could not load recent projects: {error}")),
                storage,
            },
        }
    }

    pub fn open_workspace(
        &mut self,
        path: &Path,
        source: WorkspaceSource,
    ) -> Result<PathBuf, String> {
        if !path.is_dir() {
            let message = format!("Project folder does not exist: {}", path.display());
            self.error = Some(message.clone());
            return Err(message);
        }

        match self.storage.record_opened(path, source) {
            Ok(recent) => {
                let root = path
                    .canonicalize()
                    .map_err(|error| format!("Could not resolve project path: {error}"))?;
                self.recent = recent;
                self.current_root = Some(root.clone());
                self.error = None;
                Ok(root)
            }
            Err(error) => {
                let message = format!("Could not open project: {error}");
                self.error = Some(message.clone());
                Err(message)
            }
        }
    }

    pub fn remove_recent_workspace(&mut self, path: &Path) -> Result<bool, String> {
        match self.storage.remove(path) {
            Ok(removed) => {
                if removed {
                    self.recent.remove_path(path);
                }
                self.error = None;
                Ok(removed)
            }
            Err(error) => {
                let message = format!("Could not remove recent project: {error}");
                self.error = Some(message.clone());
                Err(message)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// W-1: open_workspace 记录最近项目、设置 current_root，并清空旧错误。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   temp workspace = temp/opened-workspace，真实目录，因此 path.is_dir() = true
    ///   storage path = temp/state/app.sqlite
    ///   最近项目记录数：初始 0，打开后 1
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. WorkspaceState::load_from_storage(storage) → recent=[]
    ///   2. 预置 error=Some("old")
    ///   3. open_workspace(workspace, Open) → storage.record_opened → recent 写入 SQLite
    ///   4. 查询 current_root / recent / error
    ///
    /// 预期结果：
    ///   - 正断言：返回 canonical root，current_root 与 recent[0].path 都等于 root
    ///   - 正断言：error 被清空
    ///   - 负断言：recent 不应仍为空
    #[test]
    fn test_open_workspace_records_recent_and_sets_current_root() {
        let temp = tempfile::tempdir().expect("test should create temp dir");
        let workspace_path = temp.path().join("opened-workspace");
        std::fs::create_dir(&workspace_path).expect("test should create workspace dir");
        let storage = RecentStore::new(temp.path().join("state").join("app.sqlite"));
        let mut state = WorkspaceState::load_from_storage(storage);
        state.error = Some("old error".into());

        let root = state
            .open_workspace(&workspace_path, WorkspaceSource::Open)
            .expect("open_workspace 应成功");

        assert_eq!(
            state.current_root.as_deref(),
            Some(root.as_path()),
            "open_workspace 应把 current_root 设置为 canonical root"
        );
        assert_eq!(
            state.recent.records[0].path, root,
            "最近项目第一条应记录刚打开的 workspace root"
        );
        assert!(
            state.error.is_none(),
            "成功打开 workspace 后应清空旧错误状态"
        );
        assert!(
            !state.recent.records.is_empty(),
            "成功打开 workspace 后最近项目不应仍为空"
        );
    }

    /// W-2: open_workspace 遇到不存在路径时保留启动页状态所需错误，不写入最近项目。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   missing path = temp/missing-workspace，未创建目录，因此 path.is_dir() = false
    ///   初始 recent count = 0
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. WorkspaceState::load_from_storage(storage)
    ///   2. open_workspace(missing, Open) → 早返回 Err
    ///   3. 查询 recent 与 error
    ///
    /// 预期结果：
    ///   - 正断言：返回 Err 且 error 包含 missing path
    ///   - 正断言：recent 仍为空
    ///   - 负断言：current_root 不应被设置
    #[test]
    fn test_open_missing_workspace_sets_error_without_recent_record() {
        let temp = tempfile::tempdir().expect("test should create temp dir");
        let missing_path = temp.path().join("missing-workspace");
        let storage = RecentStore::new(temp.path().join("app.sqlite"));
        let mut state = WorkspaceState::load_from_storage(storage);

        let result = state.open_workspace(&missing_path, WorkspaceSource::Open);

        assert!(
            result.is_err(),
            "打开不存在目录应返回 Err，让启动页停留并显示错误"
        );
        assert!(
            state
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("missing-workspace"),
            "错误文案应包含缺失目录名，实际: {:?}",
            state.error
        );
        assert!(
            state.recent.records.is_empty(),
            "打开失败不应写入最近项目列表"
        );
        assert!(
            state.current_root.is_none(),
            "打开失败不应设置 current_root，否则 app 会误进入画布"
        );
    }

    /// W-3: 从启动页删除最近项目时，内存列表与 SQLite 都只移除指定项目。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   alpha path = temp/alpha-workspace，真实目录
    ///   beta path  = temp/beta-workspace，真实目录
    ///   初始 recent count = 2；删除目标数 = 1；预期内存与 SQLite 剩余数 = 2 - 1 = 1
    ///   beta 目录会在删除 Recent 记录前移除，模拟启动页中的过期项目
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. state.open_workspace(alpha) 与 state.open_workspace(beta) 写入最近项目。
    ///   2. 删除 beta 文件夹后，state.remove_recent_workspace(beta canonical path) 同步执行
    ///      RecentStore 删除并更新内存。
    ///   3. 重新通过 RecentStore::load 查询 SQLite 持久化结果。
    ///
    /// 预期结果：
    ///   - 正断言：即使 beta 文件夹已不存在，删除后内存与 SQLite 都只保留 alpha，error 被清空。
    ///   - 负断言：beta 不应残留在内存或持久化记录中，重复删除应返回 false。
    #[test]
    fn test_remove_recent_workspace_updates_memory_and_sqlite() {
        let temp = tempfile::tempdir().expect("test should create temp dir");
        let alpha = temp.path().join("alpha-workspace");
        let beta = temp.path().join("beta-workspace");
        std::fs::create_dir(&alpha).expect("test should create alpha workspace");
        std::fs::create_dir(&beta).expect("test should create beta workspace");
        let store = RecentStore::new(temp.path().join("state").join("app.sqlite"));
        let persistence_store = store.clone();
        let mut state = WorkspaceState::load_from_storage(store);

        state
            .open_workspace(&alpha, WorkspaceSource::Open)
            .expect("alpha 应成为最近项目");
        state
            .open_workspace(&beta, WorkspaceSource::Open)
            .expect("beta 应成为最近项目");
        state.error = Some("old error".to_string());
        let beta_canonical = beta
            .canonicalize()
            .expect("beta workspace should canonicalize before it is removed");
        std::fs::remove_dir(&beta)
            .expect("test should remove beta workspace to model a stale recent path");

        let removed = state
            .remove_recent_workspace(&beta_canonical)
            .expect("删除已不存在的 beta Recent 记录应成功");
        let removed_again = state
            .remove_recent_workspace(&beta_canonical)
            .expect("重复删除 beta 应正常返回删除计数");
        let persisted = persistence_store
            .load()
            .expect("删除后 SQLite recent_workspaces 应可读取");
        let alpha_canonical = alpha
            .canonicalize()
            .expect("alpha workspace should canonicalize");

        assert!(removed, "删除存在的 beta 应返回 true");
        assert_eq!(
            state.recent.records.len(),
            1,
            "从两条最近项目删除 beta 后内存列表应只剩一条"
        );
        assert_eq!(
            state.recent.records[0].path, alpha_canonical,
            "删除 beta 不应影响内存中的 alpha 记录"
        );
        assert!(
            !state
                .recent
                .records
                .iter()
                .any(|record| record.path == beta_canonical),
            "beta 删除后不应残留在启动页内存数据源"
        );
        assert_eq!(
            persisted.records, state.recent.records,
            "SQLite 重新加载结果应与启动页内存 recent 保持一致"
        );
        assert!(
            state.error.is_none(),
            "删除成功后应清空旧错误，避免启动页继续显示无关错误"
        );
        assert!(
            !removed_again,
            "重复删除已不存在的 beta 应返回 false，不能误报再次删除成功"
        );
    }
}
