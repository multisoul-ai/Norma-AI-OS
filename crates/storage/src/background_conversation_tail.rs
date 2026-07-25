use crate::{AgentConversationTailStore, StorageResult, StoredAgentConversationTail};
use std::io;
use std::sync::mpsc::{self, Sender, SyncSender};

enum ConversationTailWrite {
    Upsert(StoredAgentConversationTail),
    Delete {
        definition_id: i64,
        session_id: String,
    },
    DeleteDefinition {
        definition_id: i64,
    },
    DeleteNode {
        node_id: u64,
    },
    Flush(SyncSender<()>),
}

/// Serializes conversation-tail persistence off the GPUI/runtime foreground path.
#[derive(Clone, Debug)]
pub struct BackgroundConversationTailStore {
    sender: Sender<ConversationTailWrite>,
}

impl BackgroundConversationTailStore {
    pub fn new(store: AgentConversationTailStore) -> Self {
        Self::new_after(None, store)
    }

    pub fn new_after(
        previous: Option<BackgroundConversationTailStore>,
        store: AgentConversationTailStore,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("norma-conversation-tail-store".to_string())
            .spawn(move || {
                if let Some(previous) = previous {
                    if let Err(error) = previous.flush() {
                        tracing::error!(%error, "failed to complete previous conversation tail handoff");
                    }
                }
                while let Ok(write) = receiver.recv() {
                    match write {
                        ConversationTailWrite::Upsert(tail) => {
                            if let Err(error) = store.upsert(&tail) {
                                tracing::error!(%error, "failed to persist conversation tail");
                            }
                        }
                        ConversationTailWrite::Delete {
                            definition_id,
                            session_id,
                        } => {
                            if let Err(error) = store.delete(definition_id, &session_id) {
                                tracing::error!(%error, "failed to delete conversation tail");
                            }
                        }
                        ConversationTailWrite::DeleteDefinition { definition_id } => {
                            if let Err(error) = store.delete_for_definition(definition_id) {
                                tracing::error!(%error, "failed to delete definition conversation tails");
                            }
                        }
                        ConversationTailWrite::DeleteNode { node_id } => {
                            if let Err(error) = store.delete_for_node(node_id) {
                                tracing::error!(%error, "failed to delete node conversation tails");
                            }
                        }
                        ConversationTailWrite::Flush(done) => {
                            let _ = done.send(());
                        }
                    }
                }
            })
            .expect("conversation tail persistence worker should spawn");
        Self { sender }
    }

    pub fn upsert(&self, tail: StoredAgentConversationTail) {
        let _ = self.sender.send(ConversationTailWrite::Upsert(tail));
    }

    pub fn delete(&self, definition_id: i64, session_id: impl Into<String>) {
        let _ = self.sender.send(ConversationTailWrite::Delete {
            definition_id,
            session_id: session_id.into(),
        });
    }

    pub fn delete_for_definition(&self, definition_id: i64) {
        let _ = self
            .sender
            .send(ConversationTailWrite::DeleteDefinition { definition_id });
    }

    pub fn delete_for_node(&self, node_id: u64) {
        let _ = self
            .sender
            .send(ConversationTailWrite::DeleteNode { node_id });
    }

    pub fn flush(&self) -> StorageResult<()> {
        let (done_tx, done_rx) = mpsc::sync_channel(0);
        self.sender
            .send(ConversationTailWrite::Flush(done_tx))
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "conversation tail writer channel closed",
                )
            })?;
        done_rx.recv().map_err(|_| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "conversation tail writer flush waiter dropped",
            )
        })?;
        Ok(())
    }
}
