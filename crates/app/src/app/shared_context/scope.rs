use super::*;
use crate::app::MultisoulApp;

impl MultisoulApp {
    /// Explicit identity of the source session, never the desktop host's current focus.
    pub(crate) fn shared_context_service(&mut self) -> Result<SharedContext, AdapterError> {
        let scope = if let Some(scope) = &self.shared_context_scope {
            scope.clone()
        } else if let (Some(id), Some(root)) =
            (&self.workspace_id, &self.workspace_state.current_root)
        {
            ContextScope {
                workspace_id: id.clone(),
                area_id: "legacy".into(),
                project: ContextProject::Project {
                    project_id: id.clone(),
                    worktree: root.to_string_lossy().into_owned(),
                },
                task_id: None,
                conversation_id: None,
            }
        } else if self.workspace_state.current_root.is_some() {
            return Err(AdapterError::Rejected(
                "context_scope_unavailable：项目身份尚未就绪".into(),
            ));
        } else {
            ContextScope {
                workspace_id: self
                    .desktop_workspace
                    .active_id
                    .clone()
                    .unwrap_or_else(|| "personal".into()),
                area_id: "overview".into(),
                project: ContextProject::Personal,
                task_id: None,
                conversation_id: None,
            }
        };
        if let Some(service) = self
            .shared_context
            .as_ref()
            .filter(|service| service.scope() == &scope)
        {
            return Ok(service.clone());
        }
        let service = SharedContext::new(self.database_paths.app_db_path().to_path_buf(), scope)?;
        self.shared_context = Some(service.clone());
        let load = service.clone();
        super::super::agent_runtime::shared_tokio_runtime()
            .map_err(AdapterError::Failed)?
            .spawn(async move {
                if let Err(error) = load.snapshot().await {
                    tracing::warn!(%error, "shared context initial load failed");
                }
            });
        Ok(service)
    }
}
