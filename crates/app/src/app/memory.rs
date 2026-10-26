//! Background adapter shared by Rig and desktop realtime voice.
use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, MemorySessionContext,
    ToolContext,
};
use futures::future::BoxFuture;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
mod workspace;
#[cfg(test)]
#[path = "../../tests/unit/workspace_memory_scope_tests.rs"]
mod workspace_scope_tests;

#[cfg(test)]
#[path = "../../tests/unit/norma_memory_revision_tests.rs"]
mod revision_tests;
#[cfg(test)]
#[path = "../../tests/unit/norma_memory_adapter_tests.rs"]
mod tests;

struct MemoryState {
    access: storage::MemoryAccess,
    catalog: Option<String>,
    recalled: bool,
    epoch: u64,
    catalog_epoch: u64,
    has_project: bool,
    revisions: Vec<(agent::MemoryScope, agent::MemoryRevision)>,
    workspace: Option<workspace::WorkspaceMemory>,
}
struct MemoryAdapter {
    delegate: Arc<dyn AgentCardAdapter>,
    state: Arc<Mutex<MemoryState>>,
    settings: storage::SettingsStore,
    personal: bool,
}

pub(super) fn adapter(
    delegate: Arc<dyn AgentCardAdapter>,
    paths: storage::DatabasePaths,
    root: Option<PathBuf>,
) -> Arc<dyn AgentCardAdapter> {
    build_adapter(delegate, paths, root, None)
}

pub(super) fn workspace_adapter(
    delegate: Arc<dyn AgentCardAdapter>,
    paths: storage::DatabasePaths,
    roots: Arc<Mutex<Vec<PathBuf>>>,
) -> Arc<dyn AgentCardAdapter> {
    build_adapter(delegate, paths, None, Some(roots))
}

fn build_adapter(
    delegate: Arc<dyn AgentCardAdapter>,
    paths: storage::DatabasePaths,
    root: Option<PathBuf>,
    workspace: Option<Arc<Mutex<Vec<PathBuf>>>>,
) -> Arc<dyn AgentCardAdapter> {
    Arc::new(MemoryAdapter {
        delegate,
        settings: storage::SettingsStore::new(paths.app_db_path().to_path_buf()),
        personal: root.is_none() && workspace.is_none(),
        state: Arc::new(Mutex::new(MemoryState {
            has_project: root.is_some(),
            revisions: Vec::new(),
            workspace: workspace.map(|roots| workspace::WorkspaceMemory::new(paths.clone(), roots)),
            access: storage::MemoryAccess::new(paths, root),
            catalog: None,
            recalled: false,
            epoch: 0,
            catalog_epoch: 0,
        })),
    })
}

impl AgentCardAdapter for MemoryAdapter {
    fn memory_context(&self) -> BoxFuture<'_, Result<Option<MemorySessionContext>, AdapterError>> {
        let state = self.state.clone();
        let settings = self.settings.clone();
        Box::pin(async move {
            super::agent_runtime::shared_tokio_runtime()
                .map_err(AdapterError::Failed)?
                .spawn_blocking(move || {
                    let mut state = state
                        .lock()
                        .map_err(|_| AdapterError::Failed("记忆服务锁损坏".into()))?;
                    let (preferences, epoch) = state.access.recall_state()?;
                    let catalog_epoch = state.access.catalog_epoch()?;
                    let mut revisions = Vec::new();
                    if preferences.recall {
                        for alias in [
                            agent::MemoryScopeAlias::Personal,
                            agent::MemoryScopeAlias::Project,
                            agent::MemoryScopeAlias::Worktree,
                        ] {
                            if alias == agent::MemoryScopeAlias::Personal || state.has_project {
                                let snapshot = state.access.settings_snapshot(alias)?;
                                revisions.push((snapshot.scope().clone(), snapshot.revision()));
                            }
                        }
                    }
                    if preferences.recall {
                        if let Some(workspace) = &mut state.workspace {
                            revisions.extend(workspace.revisions()?);
                        }
                    }
                    let persona = crate::settings::norma::persona::load_profile(&settings)
                        .map_err(AdapterError::Failed)?;
                    if !preferences.recall
                        || state.epoch != epoch
                        || state.catalog_epoch != catalog_epoch
                        || state.revisions != revisions
                    {
                        state.catalog = None;
                    }
                    if preferences.recall && (!state.recalled || state.catalog.is_none()) {
                        state.catalog = Some(state.access.activate()?);
                    }
                    state.recalled = preferences.recall;
                    state.epoch = epoch;
                    state.catalog_epoch = catalog_epoch;
                    state.revisions = revisions.clone();
                    Ok(Some(MemorySessionContext {
                        revisions,
                        persona,
                        catalog: state.catalog.clone().unwrap_or_default(),
                        preferences,
                        recall_epoch: epoch,
                    }))
                })
                .await
                .map_err(|error| AdapterError::Failed(error.to_string()))?
        })
    }
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            if let AdapterCommand::Memory(command) = command {
                let state = self.state.clone();
                let context = context.clone();
                return super::agent_runtime::shared_tokio_runtime()
                    .map_err(AdapterError::Failed)?
                    .spawn_blocking(move || {
                        let mut state = state
                            .lock()
                            .map_err(|_| AdapterError::Failed("记忆服务锁损坏".into()))?;
                        let executed = state.access.execute(command, &context);
                        if executed.as_ref().is_ok_and(|output| output.saved)
                            || matches!(&executed, Err(AdapterError::Unknown(_)))
                        {
                            state.catalog = None;
                        }
                        let output = match executed {
                            Ok(output) => output,
                            Err(error @ AdapterError::Unknown(_)) => {
                                return Ok(AdapterResult::unknown(error.to_string()))
                            }
                            Err(error) => return Err(error),
                        };
                        let mut result = AdapterResult::succeeded(if output.saved {
                            "记忆已保存"
                        } else {
                            "记忆读取完成"
                        });
                        result.memory = Some(output);
                        Ok(result)
                    })
                    .await
                    .map_err(|error| AdapterError::Failed(error.to_string()))?;
            }
            if self.personal && !matches!(command, AdapterCommand::ContextCheckpoint { .. }) {
                return Err(AdapterError::Rejected(
                    "个人会话没有项目或画布工具权限".into(),
                ));
            }
            self.delegate.execute(command, context).await
        })
    }
}
