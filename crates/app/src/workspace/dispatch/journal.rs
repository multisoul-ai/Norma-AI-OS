//! A shared, memory-only writer handle. Sender-loss evidence outlives a deleted session.
use super::*;
use agent::card_instance::CardInstanceEvent;
use gpui::Global;
use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
    time::Duration,
};
use storage::BackgroundWorkspaceOperationJournal;

struct JournalState {
    worker: BackgroundWorkspaceOperationJournal,
    unsent: VecDeque<CardInstanceEvent>,
    authorities: HashMap<String, Weak<AtomicBool>>,
    #[cfg(test)]
    sender_lost: bool,
}
#[derive(Clone)]
pub(crate) struct WorkspaceJournal {
    state: Arc<Mutex<JournalState>>,
    wake: async_channel::Sender<()>,
}
impl WorkspaceJournal {
    pub(crate) fn watch(&self, id: &str, authority: Arc<AtomicBool>) {
        let Ok(mut state) = self.state.lock() else {
            authority.store(false, Ordering::Release);
            return;
        };
        if !state.unsent.is_empty() {
            authority.store(false, Ordering::Release);
        }
        state.worker.watch(id, authority.clone());
        state.authorities.retain(|_, gate| gate.strong_count() > 0);
        state
            .authorities
            .insert(id.into(), Arc::downgrade(&authority));
    }
    /// No SQLite or thread creation. Both frames and detached submissions use this same handle.
    pub(crate) fn enqueue(&self, event: CardInstanceEvent) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        #[cfg(test)]
        let result = if state.sender_lost {
            Err(event)
        } else {
            state.worker.enqueue(event)
        };
        #[cfg(not(test))]
        let result = state.worker.enqueue(event);
        if let Err(event) = result {
            for gate in state.authorities.values().filter_map(Weak::upgrade) {
                gate.store(false, Ordering::Release);
            }
            state.unsent.push_back(event);
            let _ = self.wake.try_send(());
        }
    }
    #[cfg(test)]
    pub(crate) fn simulate_sender_loss(&self) {
        self.state.lock().expect("journal test state").sender_lost = true;
    }
}
#[derive(Default)]
struct WorkspaceJournals(HashMap<PathBuf, WorkspaceJournal>);
impl Global for WorkspaceJournals {}

pub(crate) async fn shared_journal(
    database: &Path,
    cx: &mut gpui::AsyncApp,
) -> Result<WorkspaceJournal, String> {
    if let Some(worker) = cx
        .update(|cx| {
            cx.try_global::<WorkspaceJournals>()
                .and_then(|journals| journals.0.get(database).cloned())
        })
        .map_err(|e| e.to_string())?
    {
        return Ok(worker);
    }
    let worker = new_worker(database, cx).await?;
    let path = database.to_owned();
    cx.update(|cx| {
        let journals = cx.default_global::<WorkspaceJournals>();
        if let Some(existing) = journals.0.get(&path) {
            return existing.clone();
        }
        let (wake, receiver) = async_channel::bounded(1);
        let state = Arc::new(Mutex::new(JournalState {
            worker,
            unsent: VecDeque::new(),
            authorities: HashMap::new(),
            #[cfg(test)]
            sender_lost: false,
        }));
        let journal = WorkspaceJournal {
            state: state.clone(),
            wake,
        };
        journals.0.insert(path.clone(), journal.clone());
        let state = Arc::downgrade(&state);
        cx.spawn(async move |cx| {
            while receiver.recv().await.is_ok() {
                loop {
                    let Some(state) = state.upgrade() else {
                        return;
                    };
                    if state.lock().is_ok_and(|s| s.unsent.is_empty()) {
                        break;
                    }
                    if let Ok(worker) = new_worker(&path, cx).await {
                        if let Ok(mut state) = state.lock() {
                            state.worker = worker;
                            #[cfg(test)]
                            {
                                state.sender_lost = false;
                            }
                            for (id, gate) in &state.authorities {
                                if let Some(gate) = gate.upgrade() {
                                    state.worker.watch(id, gate);
                                }
                            }
                            while let Some(event) = state.unsent.pop_front() {
                                if let Err(event) = state.worker.enqueue(event) {
                                    state.unsent.push_front(event);
                                    break;
                                }
                            }
                            if state.unsent.is_empty() {
                                break;
                            }
                        }
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                }
            }
        })
        .detach();
        journal
    })
    .map_err(|e| e.to_string())
}

async fn new_worker(
    database: &Path,
    cx: &gpui::AsyncApp,
) -> Result<BackgroundWorkspaceOperationJournal, String> {
    let path = database.to_owned();
    cx.background_executor()
        .spawn(async move {
            BackgroundWorkspaceOperationJournal::new(WorkspaceOperationStore::new(path))
                .map_err(|e| e.to_string())
        })
        .await
}
