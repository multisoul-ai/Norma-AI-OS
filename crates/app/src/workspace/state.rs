//! App-layer workspace state: recent list loading, current root, and user-facing errors.

use std::path::{Path, PathBuf};
use storage::RecentStore;
use workspace::{RecentWorkspaces, WorkspaceSource};

pub struct WorkspaceState {
    pub recent: RecentWorkspaces,
    pub current_root: Option<PathBuf>,
    pub error: Option<ui::i18n::LocalizedText>,
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
                error: Some(ui::i18n::message(
                    "workspace.could_not_load_recent_projects_error",
                    &[("error", (error).to_string())],
                )),
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
            let message = ui::i18n::message(
                "workspace.project_folder_does_not_exist",
                &[("arg0", (path.display()).to_string())],
            );
            self.error = Some(message.clone());
            return Err(message.to_string());
        }

        match self.storage.record_opened(path, source) {
            Ok(recent) => {
                let root = path.canonicalize().map_err(|error| {
                    ui::i18n::format(
                        "workspace.could_not_resolve_project_path_error",
                        &[("error", (error).to_string())],
                    )
                })?;
                self.recent = recent;
                self.current_root = Some(root.clone());
                self.error = None;
                Ok(root)
            }
            Err(error) => {
                let message = ui::i18n::message(
                    "workspace.could_not_open_project_error",
                    &[("error", (error).to_string())],
                );
                self.error = Some(message.clone());
                Err(message.to_string())
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
}
