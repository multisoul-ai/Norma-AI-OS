use media_store::{MediaArtifact, MediaStore};
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

pub(super) enum DownloadEvent {
    CacheInvalidated,
    Progress(u64),
    Finished(Result<PathBuf, String>),
}

pub(super) async fn acquire_slot(
    cancellation: &DownloadCancellation,
) -> Result<tokio::sync::OwnedSemaphorePermit, String> {
    static SLOTS: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    let slots = SLOTS
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(2)))
        .clone();
    tokio::select! {
        biased;
        _ = cancellation.network.cancelled() => Err("cancelled".into()),
        permit = slots.acquire_owned() => permit.map_err(|error| error.to_string()),
    }
}

#[derive(Default)]
pub(super) struct DownloadCancellation {
    file: AtomicBool,
    network: CancellationToken,
}

impl DownloadCancellation {
    pub(super) fn cancel(&self) {
        self.file.store(true, Ordering::Release);
        self.network.cancel();
    }
}

pub(super) fn download(
    artifact: &MediaArtifact,
    store: &MediaStore,
    cancellation: &DownloadCancellation,
    events: &async_channel::Sender<DownloadEvent>,
) -> Result<PathBuf, String> {
    #[cfg(feature = "media-download-qa")]
    {
        let override_base = std::env::var("NORMA_MEDIA_QA_BASE_URL").ok();
        let url = super::qa::transport_url(
            artifact.url(),
            override_base.as_deref(),
            cfg!(feature = "production-data"),
        )?;
        download_from_url(artifact, store, cancellation, events, &url)
    }
    #[cfg(not(feature = "media-download-qa"))]
    download_from_url(artifact, store, cancellation, events, artifact.url())
}

fn download_from_url(
    artifact: &MediaArtifact,
    store: &MediaStore,
    cancellation: &DownloadCancellation,
    events: &async_channel::Sender<DownloadEvent>,
    url: &str,
) -> Result<PathBuf, String> {
    if cancellation.file.load(Ordering::Acquire) {
        return Err("cancelled".into());
    }
    if let Ok(Some(path)) = store.find_verified(artifact) {
        return Ok(path);
    }
    let _ = events.try_send(DownloadEvent::CacheInvalidated);
    let runtime = crate::app::shared_tokio_runtime()?;
    let _runtime_guard = runtime.enter();
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.url().scheme() != "https" || attempt.previous().len() >= 10 {
                attempt.error("unsafe or excessive media redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|error| error.to_string())?;
    let response = runtime.block_on(async {
        tokio::select! {
            _ = cancellation.network.cancelled() => Err("cancelled".into()),
            response = client.get(url).send() => response.and_then(reqwest::Response::error_for_status)
                .map_err(|error| error.to_string()),
        }
    })?;
    let mut reader = ResponseReader {
        response,
        runtime,
        chunk: Cursor::new(Vec::new()),
        received: 0,
        last_update: Instant::now(),
        events,
        cancellation,
    };
    store
        .install(artifact, &mut reader, &cancellation.file)
        .map_err(|error| error.to_string())
}

// Adapt asynchronous, cancellable HTTP chunks to the domain's bounded Read
// installer. This entire adapter runs on a background executor thread.
struct ResponseReader<'a> {
    response: reqwest::Response,
    runtime: &'static tokio::runtime::Runtime,
    chunk: Cursor<Vec<u8>>,
    received: u64,
    last_update: Instant,
    events: &'a async_channel::Sender<DownloadEvent>,
    cancellation: &'a DownloadCancellation,
}

impl Read for ResponseReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.cancellation.file.load(Ordering::Acquire) {
            return Err(cancelled_io());
        }
        while self.chunk.position() >= self.chunk.get_ref().len() as u64 {
            let chunk = self.runtime.block_on(async {
                tokio::select! {
                    _ = self.cancellation.network.cancelled() => Err(cancelled_io()),
                    result = self.response.chunk() => result.map_err(std::io::Error::other),
                }
            })?;
            let Some(chunk) = chunk else {
                return Ok(0);
            };
            self.chunk = Cursor::new(chunk.to_vec());
        }
        let count = self.chunk.read(buffer)?;
        self.received += count as u64;
        if self.last_update.elapsed() >= Duration::from_millis(100) {
            let _ = self.events.try_send(DownloadEvent::Progress(self.received));
            self.last_update = Instant::now();
        }
        Ok(count)
    }
}

fn cancelled_io() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Interrupted, "media download cancelled")
}

#[cfg(test)]
#[path = "../../tests/unit/media_download_tests.rs"]
mod tests;
