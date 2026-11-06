use super::{IslandPreferences, SettingsStore, KEY};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc, Arc,
};

pub(super) struct WriteResult {
    pub revision: u64,
    pub result: Result<(), String>,
}
enum Request {
    Save(IslandPreferences, u64),
    Flush(async_channel::Sender<Result<(), String>>),
    #[cfg(test)]
    Pause(mpsc::Receiver<()>),
}

#[derive(Clone)]
pub(crate) struct Persistence {
    sender: mpsc::Sender<Request>,
    revision: Arc<AtomicU64>,
    pub(super) results: async_channel::Receiver<WriteResult>,
    #[cfg(test)]
    pub flushes: Arc<AtomicU64>,
}

impl Persistence {
    pub fn start(store: SettingsStore) -> Self {
        let (sender, requests) = mpsc::channel();
        let (results_tx, results) = async_channel::unbounded();
        std::thread::spawn(move || {
            let mut last = Ok(());
            for request in requests {
                match request {
                    Request::Save(preferences, revision) => {
                        last = serde_json::to_string(&preferences)
                            .map_err(|error| error.to_string())
                            .and_then(|json| {
                                store
                                    .set_string(KEY, &json)
                                    .map_err(|error| error.to_string())
                            });
                        let _ = results_tx.try_send(WriteResult {
                            revision,
                            result: last.clone(),
                        });
                    }
                    Request::Flush(reply) => {
                        let _ = reply.try_send(last.clone());
                    }
                    #[cfg(test)]
                    Request::Pause(resume) => {
                        let _ = resume.recv();
                    }
                }
            }
        });
        Self {
            sender,
            revision: Arc::new(AtomicU64::new(0)),
            results,
            #[cfg(test)]
            flushes: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn save(&self, preferences: IslandPreferences) -> Result<u64, String> {
        let revision = self.revision.fetch_add(1, Ordering::Relaxed) + 1;
        self.sender
            .send(Request::Save(preferences, revision))
            .map_err(|error| error.to_string())?;
        Ok(revision)
    }

    pub async fn flush(&self) -> Result<(), String> {
        #[cfg(test)]
        self.flushes.fetch_add(1, Ordering::SeqCst);
        let (reply, done) = async_channel::bounded(1);
        self.sender
            .send(Request::Flush(reply))
            .map_err(|error| error.to_string())?;
        done.recv().await.map_err(|error| error.to_string())?
    }

    #[cfg(test)]
    pub fn pause(&self) -> mpsc::Sender<()> {
        let (resume, wait) = mpsc::channel();
        self.sender
            .send(Request::Pause(wait))
            .expect("保存队列应开放");
        resume
    }
}
