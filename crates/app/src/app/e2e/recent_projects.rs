use super::MultisoulApp;
use gpui::Context;
use storage::RecentStore;
use workspace::RecentWorkspaces;

impl MultisoulApp {
    /// 用独立 SQLite store 装配启动页最近项目，避免 e2e 接触用户的默认 app.sqlite。
    pub fn set_recent_projects_for_e2e(
        &mut self,
        store: RecentStore,
        recent: RecentWorkspaces,
        cx: &mut Context<Self>,
    ) -> storage::StorageResult<()> {
        store.save(&recent)?;
        self.workspace_state = crate::workspace::state::WorkspaceState::load_from_storage(store);
        self.recent_project_context_path = None;
        cx.notify();
        Ok(())
    }

    /// 启动页 Recent projects 的稳定 e2e 投影，不暴露 WorkspaceState 的私有存储字段。
    pub fn recent_project_paths_for_e2e(&self) -> Vec<std::path::PathBuf> {
        self.workspace_state
            .recent
            .records
            .iter()
            .map(|record| record.path.clone())
            .collect()
    }
}
