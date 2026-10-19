#[cfg(test)]
#[path = "../../../tests/unit/gpt_live_task_facts_tests.rs"]
mod tests;
use super::session::CallCommand;
use std::collections::HashSet;
use storage::{
    AgentDispatchTaskState, AgentDispatchTaskStore, LiveOperationStore, LiveTaskFact, StorageResult,
};
use tokio_util::sync::CancellationToken;

fn collect_facts(
    store: &LiveOperationStore,
    project: &str,
    tasks: &AgentDispatchTaskStore,
    cursor: &mut i64,
) -> StorageResult<Vec<LiveTaskFact>> {
    let watches = store.task_watches(project, *cursor)?;
    if watches.is_empty() {
        *cursor = 0;
    }
    for watch in watches {
        *cursor = watch.id;
        let Some(task) = tasks.get_by_task_ref(&watch.task_ref)? else {
            continue;
        };
        let state = match task.state {
            AgentDispatchTaskState::Done => "succeeded",
            AgentDispatchTaskState::Failed => "failed",
            AgentDispatchTaskState::Cancelled => "cancelled",
            AgentDispatchTaskState::Interrupted => "interrupted",
            _ => continue,
        };
        let content = format!("真实任务终态：task_ref={}，node={}，状态={}。这是持久任务事实，append ACK仅表示服务端接收，不代表完整播报。",task.task_ref,task.node_id,state);
        store.queue_task_fact(&watch, state, &content)?;
    }
    store.pending_task_facts(project)
}

pub(super) struct TaskReporter {
    pub database: std::path::PathBuf,
    pub project: String,
    pub tasks: AgentDispatchTaskStore,
    pub commands: async_channel::Sender<CallCommand>,
    pub acknowledgements: async_channel::Receiver<String>,
    pub enabled: tokio::sync::watch::Receiver<bool>,
    pub cancellation: CancellationToken,
}
async fn wait_for_database<T>(
    cancellation: &CancellationToken,
    future: impl std::future::Future<Output = T>,
) -> Option<T> {
    tokio::select! {biased; _=cancellation.cancelled()=>None, result=future=>Some(result)}
}

impl TaskReporter {
    fn publish(&self, facts: Vec<LiveTaskFact>, in_flight: &mut HashSet<String>) -> bool {
        for fact in facts {
            // Re-read after IO and before each admission; a query snapshot cannot authorize speech.
            if self.cancellation.is_cancelled() || !*self.enabled.borrow() {
                break;
            }
            if in_flight.contains(&fact.event_id) {
                continue;
            }
            let event_id = fact.event_id.clone();
            let command = CallCommand::AppendResult {
                event_id: fact.event_id,
                kind: live_voice::AppendKind::Commentary,
                delegation_id: None,
                content: fact.content,
            };
            match self.commands.try_send(command) {
                Ok(()) => {
                    in_flight.insert(event_id);
                }
                Err(async_channel::TrySendError::Full(_)) => break,
                Err(async_channel::TrySendError::Closed(_)) => return false,
            }
        }
        true
    }

    pub async fn run(mut self) {
        let store = LiveOperationStore::new(self.database.clone());
        let mut cursor = 0;
        let mut in_flight = HashSet::new();
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = self.cancellation.cancelled() => return,
                changed = self.enabled.changed() => {if changed.is_err() {return;}},
                ack = self.acknowledgements.recv() => {
                    let Ok(event_id) = ack else {return;};
                    in_flight.remove(&event_id);
                    let (store,project)=(store.clone(),self.project.clone());
                    let Some(saved)=wait_for_database(&self.cancellation,tokio::task::spawn_blocking(move ||store.acknowledge_task_fact(&project,&event_id))).await else {return;};
                    if !matches!(saved, Ok(Ok(_))) {
                        tracing::warn!("Live task fact ACK could not be saved; facts remain pending reconciliation");
                    }
                }
                _ = tick.tick(), if *self.enabled.borrow() => {
                    let (store,project,tasks) = (store.clone(),self.project.clone(),self.tasks.clone());
                    let snapshot = wait_for_database(&self.cancellation,tokio::task::spawn_blocking(move || {
                        let facts=collect_facts(&store,&project,&tasks,&mut cursor);
                        (cursor,facts)
                    })).await;
                    let Some(snapshot)=snapshot else {return;};
                    let facts = match snapshot {
                        Ok((next,Ok(facts))) => {cursor=next;facts},
                        _ => {tracing::warn!("Live task facts unavailable; retrying from persistent state");continue;}
                    };
                    if !self.publish(facts,&mut in_flight) {return;}
                }
            }
        }
    }
}
