//! Background adapter shared by Rig and desktop realtime voice.
use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, MemorySessionContext,
    ToolContext,
};
use futures::future::BoxFuture;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

struct MemoryState {
    access: storage::MemoryAccess,
    catalog: Option<String>,
    recalled: bool,
    epoch: u64,
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
    Arc::new(MemoryAdapter {
        delegate,
        settings: storage::SettingsStore::new(paths.app_db_path().to_path_buf()),
        personal: root.is_none(),
        state: Arc::new(Mutex::new(MemoryState {
            access: storage::MemoryAccess::new(paths, root),
            catalog: None,
            recalled: false,
            epoch: 0,
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
                    let persona = crate::settings::norma::persona::load_profile(&settings)
                        .map_err(AdapterError::Failed)?;
                    if !preferences.recall || state.epoch != epoch {
                        state.catalog = None;
                    }
                    if preferences.recall && (!state.recalled || state.catalog.is_none()) {
                        state.catalog = Some(state.access.activate()?);
                    }
                    state.recalled = preferences.recall;
                    state.epoch = epoch;
                    Ok(Some(MemorySessionContext {
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
                        let executed = state
                            .lock()
                            .map_err(|_| AdapterError::Failed("记忆服务锁损坏".into()))?
                            .access
                            .execute(command, &context);
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
            if self.personal {
                return Err(AdapterError::Rejected(
                    "个人会话没有项目或画布工具权限".into(),
                ));
            }
            self.delegate.execute(command, context).await
        })
    }
}
