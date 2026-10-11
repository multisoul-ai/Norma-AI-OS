//! Retry-preserving journal worker. Enqueue is memory-only; SQLite runs on its own thread.
use crate::WorkspaceOperationStore;
use agent::card_instance::CardInstanceEvent;
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Arc, Mutex,
    },
    time::Duration,
};

#[derive(Clone, Debug, Default)]
pub struct WorkspaceJournalStatus {
    pub pending_events: usize,
    pub last_error: Option<String>,
}

#[derive(Clone)]
pub struct BackgroundWorkspaceOperationJournal {
    sender: Sender<JournalMessage>,
    status: Arc<Mutex<WorkspaceJournalStatus>>,
}
enum JournalMessage {
    Watch(String, Arc<AtomicBool>),
    Event(CardInstanceEvent),
}

impl BackgroundWorkspaceOperationJournal {
    pub fn new(store: WorkspaceOperationStore) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let status = Arc::new(Mutex::new(WorkspaceJournalStatus::default()));
        let worker_status = status.clone();
        std::thread::Builder::new()
            .name("workspace-operation-journal".into())
            .spawn(move || run(store, receiver, worker_status))?;
        Ok(Self { sender, status })
    }
    pub fn watch(&self, id: &str, authority: Arc<AtomicBool>) {
        if self.status().last_error.is_some() {
            authority.store(false, Ordering::Release);
        }
        if self
            .sender
            .send(JournalMessage::Watch(id.into(), authority.clone()))
            .is_err()
        {
            authority.store(false, Ordering::Release);
        }
    }
    /// On worker loss the caller retains the event; no foreground SQLite fallback is allowed.
    pub fn enqueue(&self, event: CardInstanceEvent) -> Result<(), CardInstanceEvent> {
        self.sender
            .send(JournalMessage::Event(event))
            .map_err(|error| match error.0 {
                JournalMessage::Event(event) => event,
                JournalMessage::Watch(..) => unreachable!("enqueue only sends events"),
            })
    }
    pub fn status(&self) -> WorkspaceJournalStatus {
        self.status
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|_| WorkspaceJournalStatus {
                pending_events: 0,
                last_error: Some("journal_worker_unavailable".into()),
            })
    }
}

fn run(
    store: WorkspaceOperationStore,
    receiver: Receiver<JournalMessage>,
    status: Arc<Mutex<WorkspaceJournalStatus>>,
) {
    let mut pending = VecDeque::new();
    let mut authorities = HashMap::new();
    let mut connected = true;
    loop {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(message) => receive(message, &mut pending, &mut authorities, &status),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => connected = false,
        }
        for _ in 0..128 {
            let Ok(message) = receiver.try_recv() else {
                break;
            };
            receive(message, &mut pending, &mut authorities, &status);
        }
        if !pending.is_empty() {
            let batch: Vec<_> = pending.iter().take(128).cloned().collect();
            match store.record_events(&batch) {
                Ok(()) => {
                    pending.drain(..batch.len());
                    if let Ok(mut state) = status.lock() {
                        state.pending_events = pending.len();
                        state.last_error = None;
                    }
                }
                Err(error) => {
                    for authority in authorities.values() {
                        authority.store(false, Ordering::Release);
                    }
                    if let Ok(mut state) = status.lock() {
                        state.pending_events = pending.len();
                        state.last_error = Some(error.to_string());
                    }
                    // All input gates are already false. Keep this same batch for a later commit;
                    // callbacks and temporary database lock failures must not discard evidence.
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
        if !connected && pending.is_empty() {
            break;
        }
    }
}
fn receive(
    message: JournalMessage,
    pending: &mut VecDeque<CardInstanceEvent>,
    authorities: &mut HashMap<String, Arc<AtomicBool>>,
    status: &Mutex<WorkspaceJournalStatus>,
) {
    match message {
        JournalMessage::Watch(id, authority) => {
            if status.lock().map_or(true, |s| s.last_error.is_some()) {
                authority.store(false, Ordering::Release);
            }
            authorities.insert(id, authority);
        }
        JournalMessage::Event(event) => {
            pending.push_back(event);
            if let Ok(mut state) = status.lock() {
                state.pending_events = pending.len();
            }
        }
    }
}
