use crate::{
    AgentDispatchTaskState, AgentDispatchTaskStore, NewAgentDispatchTask,
    NewOrchestrationAgentDispatchTask, StorageError, StorageResult,
};
use std::io;
use std::sync::mpsc::{self, Sender, SyncSender};

enum AgentDispatchWrite {
    Create(NewAgentDispatchTask),
    CreateOrchestration(NewOrchestrationAgentDispatchTask),
    FinishCurrent {
        definition_id: i64,
        state: AgentDispatchTaskState,
        error: Option<String>,
    },
    FinishTask {
        runtime_task_id: u64,
        state: AgentDispatchTaskState,
        error: Option<String>,
    },
    SubmitNextQueued {
        definition_id: i64,
    },
    FailNextQueued {
        definition_id: i64,
        error: String,
    },
    InterruptDefinition {
        definition_id: i64,
    },
    PersistCancellation {
        runtime_task_id: u64,
        running: bool,
    },
    MarkIncompleteInterrupted,
    Flush(SyncSender<()>),
}

/// 串行消费 Agent 委派审计命令，使 SQLite 永远不在 GPUI/runtime 前台路径执行。
#[derive(Clone, Debug)]
pub struct BackgroundAgentDispatchStore {
    sender: Sender<AgentDispatchWrite>,
}

impl BackgroundAgentDispatchStore {
    pub fn new(store: AgentDispatchTaskStore) -> Self {
        Self::new_after(None, store)
    }

    pub fn new_after(
        previous: Option<BackgroundAgentDispatchStore>,
        store: AgentDispatchTaskStore,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("norma-agent-dispatch-store".to_string())
            .spawn(move || {
                if let Some(previous) = previous {
                    previous.mark_incomplete_interrupted();
                    if let Err(error) = previous.flush() {
                        tracing::error!(%error, "failed to complete previous Agent dispatch handoff");
                    }
                }
                if let Err(error) = store.mark_incomplete_interrupted() {
                    tracing::error!(%error, "failed to recover background Agent dispatch tasks");
                }
                while let Ok(write) = receiver.recv() {
                    match write {
                        AgentDispatchWrite::Create(task) => {
                            if let Err(error) = store.create(&task) {
                                tracing::error!(%error, "failed to persist background Agent dispatch");
                            }
                        }
                        AgentDispatchWrite::CreateOrchestration(task) => {
                            if let Err(error) = store.create_orchestration(&task) {
                                tracing::error!(
                                    %error,
                                    task_ref = %task.task_ref,
                                    "failed to persist orchestration Agent dispatch"
                                );
                            }
                        }
                        AgentDispatchWrite::FinishCurrent {
                            definition_id,
                            state,
                            error,
                        } => {
                            if let Err(error) =
                                store.finish_current(definition_id, state, error.as_deref())
                            {
                                tracing::error!(%error, "failed to persist background Agent terminal state");
                            }
                        }
                        AgentDispatchWrite::FinishTask {
                            runtime_task_id,
                            state,
                            error,
                        } => {
                            if let Err(error) =
                                store.finish_task(runtime_task_id, state, error.as_deref())
                            {
                                tracing::error!(%error, "failed to persist exact Agent terminal state");
                            }
                        }
                        AgentDispatchWrite::SubmitNextQueued { definition_id } => {
                            if let Err(error) = store.submit_next_queued(definition_id) {
                                tracing::error!(%error, "failed to persist background Agent queue advance");
                            }
                        }
                        AgentDispatchWrite::FailNextQueued {
                            definition_id,
                            error,
                        } => {
                            if let Err(storage_error) =
                                store.fail_next_queued(definition_id, &error)
                            {
                                tracing::error!(%storage_error, "failed to persist background Agent submission failure");
                            }
                        }
                        AgentDispatchWrite::InterruptDefinition { definition_id } => {
                            if let Err(error) = store.interrupt_definition(definition_id) {
                                tracing::error!(%error, "failed to persist background Agent interruption");
                            }
                        }
                        AgentDispatchWrite::PersistCancellation {
                            runtime_task_id,
                            running,
                        } => {
                            if let Err(error) =
                                store.persist_cancellation(runtime_task_id, running)
                            {
                                tracing::error!(%error, "failed to persist Agent task cancellation");
                            }
                        }
                        AgentDispatchWrite::MarkIncompleteInterrupted => {
                            if let Err(error) = store.mark_incomplete_interrupted() {
                                tracing::error!(%error, "failed to interrupt background Agent tasks");
                            }
                        }
                        AgentDispatchWrite::Flush(done) => {
                            let _ = done.send(());
                        }
                    }
                }
            })
            .expect("Agent dispatch persistence worker should spawn");
        Self { sender }
    }

    pub fn create(&self, task: NewAgentDispatchTask) {
        let _ = self.sender.send(AgentDispatchWrite::Create(task));
    }

    pub fn create_orchestration(&self, task: NewOrchestrationAgentDispatchTask) {
        let _ = self
            .sender
            .send(AgentDispatchWrite::CreateOrchestration(task));
    }

    pub fn finish_current(
        &self,
        definition_id: i64,
        state: AgentDispatchTaskState,
        error: Option<String>,
    ) {
        let _ = self.sender.send(AgentDispatchWrite::FinishCurrent {
            definition_id,
            state,
            error,
        });
    }

    pub fn submit_next_queued(&self, definition_id: i64) {
        let _ = self
            .sender
            .send(AgentDispatchWrite::SubmitNextQueued { definition_id });
    }

    pub fn finish_task(
        &self,
        runtime_task_id: u64,
        state: AgentDispatchTaskState,
        error: Option<String>,
    ) {
        let _ = self.sender.send(AgentDispatchWrite::FinishTask {
            runtime_task_id,
            state,
            error,
        });
    }

    pub fn fail_next_queued(&self, definition_id: i64, error: String) {
        let _ = self.sender.send(AgentDispatchWrite::FailNextQueued {
            definition_id,
            error,
        });
    }

    pub fn interrupt_definition(&self, definition_id: i64) {
        let _ = self
            .sender
            .send(AgentDispatchWrite::InterruptDefinition { definition_id });
    }

    pub fn persist_cancellation(&self, runtime_task_id: u64, running: bool) {
        let _ = self.sender.send(AgentDispatchWrite::PersistCancellation {
            runtime_task_id,
            running,
        });
    }

    pub fn mark_incomplete_interrupted(&self) {
        let _ = self
            .sender
            .send(AgentDispatchWrite::MarkIncompleteInterrupted);
    }

    pub fn flush(&self) -> StorageResult<()> {
        let (done, completed) = mpsc::sync_channel(0);
        self.sender
            .send(AgentDispatchWrite::Flush(done))
            .map_err(|_| disconnected_worker())?;
        completed.recv().map_err(|_| disconnected_worker())
    }
}

fn disconnected_worker() -> StorageError {
    StorageError::Io(io::Error::new(
        io::ErrorKind::BrokenPipe,
        "Agent dispatch persistence worker disconnected",
    ))
}
