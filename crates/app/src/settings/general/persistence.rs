use super::LANGUAGE_KEY;
use async_channel::{Receiver, Sender};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc,
    },
};
use storage::SettingsStore;
use ui::i18n::LanguagePreference;

pub(super) struct WriteResult {
    pub(super) preference: LanguagePreference,
    pub(super) sequence: u64,
    pub(super) path: PathBuf,
    pub(super) result: Result<(), String>,
}

enum Request {
    Select(LanguagePreference, u64),
    Flush(Sender<Result<(), String>>),
    #[cfg(test)]
    Pause(mpsc::Receiver<()>),
    Shutdown(Sender<Result<(), String>>),
}

#[derive(Clone)]
pub(super) struct LanguagePersistence {
    sender: mpsc::Sender<Request>,
    sequence: Arc<AtomicU64>,
    pub(super) results: Receiver<WriteResult>,
    pub(super) path: PathBuf,
}

impl LanguagePersistence {
    pub(super) fn start(store: SettingsStore) -> Self {
        let path = store.path().to_path_buf();
        let (sender, requests) = mpsc::channel();
        let (results_tx, results) = async_channel::unbounded();
        std::thread::spawn(move || {
            let mut last_result = Ok(());
            while let Ok(request) = requests.recv() {
                match request {
                    Request::Select(preference, sequence) => {
                        last_result = store
                            .set_string(LANGUAGE_KEY, preference.as_str())
                            .map_err(|error| error.to_string());
                        let _ = results_tx.try_send(WriteResult {
                            preference,
                            sequence,
                            path: store.path().to_path_buf(),
                            result: last_result.clone(),
                        });
                    }
                    Request::Flush(reply) => {
                        let _ = reply.try_send(last_result.clone());
                    }
                    #[cfg(test)]
                    Request::Pause(resume) => {
                        let _ = resume.recv();
                    }
                    Request::Shutdown(reply) => {
                        let _ = reply.try_send(last_result.clone());
                        break;
                    }
                }
            }
        });
        Self {
            sender,
            sequence: Arc::new(AtomicU64::new(0)),
            results,
            path,
        }
    }

    pub(super) fn select(&self, preference: LanguagePreference) -> Result<u64, String> {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        self.sender
            .send(Request::Select(preference, sequence))
            .map_err(|error| error.to_string())?;
        Ok(sequence)
    }

    pub(super) async fn flush(&self) -> Result<(), String> {
        let (reply, done) = async_channel::bounded(1);
        self.sender
            .send(Request::Flush(reply))
            .map_err(|error| error.to_string())?;
        done.recv().await.map_err(|error| error.to_string())?
    }

    #[cfg(test)]
    pub(super) fn pause(&self) -> mpsc::Sender<()> {
        let (resume, wait) = mpsc::channel();
        self.sender
            .send(Request::Pause(wait))
            .expect("测试队列应仍开放");
        resume
    }

    pub(super) async fn shutdown(&self) {
        let (reply, done) = async_channel::bounded(1);
        if self.sender.send(Request::Shutdown(reply)).is_ok() {
            let _ = done.recv().await;
        }
    }
}
