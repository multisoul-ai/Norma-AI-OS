use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "e2e-test-support")]
use std::sync::Mutex;
use std::time::Duration;
use tts::{TtsModelCatalog, TtsModelDescriptor, TtsModelId};

// connect = 15s：本地网络/DNS 短暂抖动可恢复，但不无限占用安装任务。
// response headers = 30s：release 服务重定向与首包建立可有额外延迟。
// response body = 20min：最大内置包约 147MiB；按 1Mbit/s 约 20min，覆盖低速网络。
const TTS_DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const TTS_DOWNLOAD_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const TTS_DOWNLOAD_BODY_TIMEOUT: Duration = Duration::from_secs(20 * 60);

#[cfg(feature = "e2e-test-support")]
static E2E_FAKE_TTS_TRANSPORT: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "e2e-test-support")]
static E2E_FAKE_TTS_HOLD: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "e2e-test-support")]
static E2E_FAKE_TTS_RELEASE: Mutex<Option<async_channel::Receiver<()>>> = Mutex::new(None);
#[cfg(feature = "e2e-test-support")]
static E2E_FAKE_TTS_RELEASE_TX: Mutex<Option<async_channel::Sender<()>>> = Mutex::new(None);

#[cfg(feature = "e2e-test-support")]
pub(crate) fn set_e2e_fake_tts_transport(enabled: bool, hold_install: bool) {
    E2E_FAKE_TTS_TRANSPORT.store(enabled, Ordering::Release);
    E2E_FAKE_TTS_HOLD.store(enabled && hold_install, Ordering::Release);
    let (sender, receiver) = async_channel::bounded(1);
    *E2E_FAKE_TTS_RELEASE
        .lock()
        .expect("fake TTS release receiver lock should not be poisoned") =
        (enabled && hold_install).then_some(receiver);
    *E2E_FAKE_TTS_RELEASE_TX
        .lock()
        .expect("fake TTS release sender lock should not be poisoned") =
        (enabled && hold_install).then_some(sender);
}

#[cfg(feature = "e2e-test-support")]
pub(crate) fn e2e_fake_tts_transport_enabled() -> bool {
    E2E_FAKE_TTS_TRANSPORT.load(Ordering::Acquire)
}

#[cfg(feature = "e2e-test-support")]
pub(crate) fn take_e2e_fake_tts_release_receiver() -> Option<async_channel::Receiver<()>> {
    E2E_FAKE_TTS_RELEASE
        .lock()
        .expect("fake TTS release receiver lock should not be poisoned")
        .clone()
}

#[cfg(feature = "e2e-test-support")]
pub(crate) fn e2e_fake_tts_transport_is_held() -> bool {
    E2E_FAKE_TTS_HOLD.load(Ordering::Acquire)
}

#[cfg(feature = "e2e-test-support")]
pub(crate) fn release_e2e_fake_tts_transport() {
    if let Some(sender) = E2E_FAKE_TTS_RELEASE_TX
        .lock()
        .expect("fake TTS release sender lock should not be poisoned")
        .as_ref()
    {
        let _ = sender.try_send(());
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TtsModelInstallState {
    NotInstalled,
    Installing { downloaded: u64, total: u64 },
    Cancelling { downloaded: u64, total: u64 },
    Ready,
    Failed(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TtsModelEntryState {
    descriptor: TtsModelDescriptor,
    installation: TtsModelInstallState,
}

impl TtsModelEntryState {
    pub(crate) fn descriptor(&self) -> &TtsModelDescriptor {
        &self.descriptor
    }
    pub(crate) fn installation(&self) -> &TtsModelInstallState {
        &self.installation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TtsModelManagerState {
    entries: Vec<TtsModelEntryState>,
}

impl TtsModelManagerState {
    pub(crate) fn unscanned(catalog: &TtsModelCatalog) -> Self {
        Self {
            entries: catalog
                .entries()
                .iter()
                .cloned()
                .map(|descriptor| TtsModelEntryState {
                    descriptor,
                    installation: TtsModelInstallState::NotInstalled,
                })
                .collect(),
        }
    }

    pub(crate) fn scan(catalog: &TtsModelCatalog, root: &Path) -> Self {
        let mut state = Self::unscanned(catalog);
        for entry in &mut state.entries {
            let model_root = root.join(entry.descriptor.id().as_str());
            entry.installation = if !model_root.exists() {
                TtsModelInstallState::NotInstalled
            } else {
                tts::validate_installation(&model_root, &entry.descriptor)
                    .map(|_| TtsModelInstallState::Ready)
                    .unwrap_or_else(|error| TtsModelInstallState::Failed(error.to_string()))
            };
        }
        state
    }

    pub(crate) fn entries(&self) -> &[TtsModelEntryState] {
        &self.entries
    }
    pub(crate) fn installation(&self, id: &TtsModelId) -> Option<&TtsModelInstallState> {
        self.entries
            .iter()
            .find(|entry| entry.descriptor.id() == id)
            .map(TtsModelEntryState::installation)
    }
    pub(crate) fn begin_install(&mut self, id: &TtsModelId, total: u64) {
        self.update(
            id,
            TtsModelInstallState::Installing {
                downloaded: 0,
                total,
            },
        );
    }
    pub(crate) fn update_progress(&mut self, id: &TtsModelId, downloaded: u64, total: u64) {
        if matches!(
            self.installation(id),
            Some(TtsModelInstallState::Installing { .. })
        ) {
            self.update(id, TtsModelInstallState::Installing { downloaded, total });
        }
    }
    pub(crate) fn begin_cancellation(&mut self, id: &TtsModelId) {
        let progress = self
            .installation(id)
            .and_then(|installation| match installation {
                TtsModelInstallState::Installing { downloaded, total } => {
                    Some((*downloaded, *total))
                }
                TtsModelInstallState::Cancelling { downloaded, total } => {
                    Some((*downloaded, *total))
                }
                TtsModelInstallState::NotInstalled
                | TtsModelInstallState::Ready
                | TtsModelInstallState::Failed(_) => None,
            });
        if let Some((downloaded, total)) = progress {
            self.update(id, TtsModelInstallState::Cancelling { downloaded, total });
        }
    }
    pub(crate) fn mark_ready(&mut self, id: &TtsModelId) {
        self.update(id, TtsModelInstallState::Ready);
    }
    pub(crate) fn mark_failed(&mut self, id: &TtsModelId, error: impl Into<String>) {
        self.update(id, TtsModelInstallState::Failed(error.into()));
    }
    pub(crate) fn mark_not_installed(&mut self, id: &TtsModelId) {
        self.update(id, TtsModelInstallState::NotInstalled);
    }
    fn update(&mut self, id: &TtsModelId, installation: TtsModelInstallState) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.descriptor.id() == id)
        {
            entry.installation = installation;
        }
    }
}

pub(crate) struct DownloadedTtsPackage {
    pub(crate) archive: PathBuf,
    pub(crate) support: Vec<PathBuf>,
}

pub(crate) fn download_tts_package(
    descriptor: &TtsModelDescriptor,
    downloads: &Path,
    cancelled: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<DownloadedTtsPackage, String> {
    fs::create_dir_all(downloads)
        .map_err(|error| format!("could not create TTS download directory: {error}"))?;
    let package = descriptor.package();
    let total = package.download_bytes();
    let archive = download_file(
        package.archive().url(),
        package.archive().archive_bytes(),
        downloads.join(format!("{}.tar.bz2", descriptor.id().as_str())),
        cancelled,
        |downloaded| progress(downloaded, total),
    )?;
    let mut completed = package.archive().archive_bytes();
    let mut support = Vec::new();
    for (index, artifact) in package.support_artifacts().iter().enumerate() {
        let path = match download_file(
            artifact.url(),
            artifact.bytes(),
            downloads.join(format!("{}-support-{index}", descriptor.id().as_str())),
            cancelled,
            |downloaded| progress(completed.saturating_add(downloaded), total),
        ) {
            Ok(path) => path,
            Err(error) => {
                remove_downloads(&archive, &support);
                return Err(error);
            }
        };
        completed = completed.saturating_add(artifact.bytes());
        support.push(path);
    }
    if cancelled.load(Ordering::Acquire) {
        remove_downloads(&archive, &support);
        return Err("TTS download cancelled".to_string());
    }
    Ok(DownloadedTtsPackage { archive, support })
}

#[cfg(test)]
#[path = "../../../tests/unit/tts_model_manager_tests.rs"]
mod tts_model_manager_tests;

fn download_file(
    url: &str,
    expected: u64,
    final_path: PathBuf,
    cancelled: &AtomicBool,
    progress: impl FnMut(u64),
) -> Result<PathBuf, String> {
    download_file_with_body_timeout(
        url,
        expected,
        final_path,
        cancelled,
        progress,
        TTS_DOWNLOAD_BODY_TIMEOUT,
    )
}

fn download_file_with_body_timeout(
    url: &str,
    expected: u64,
    final_path: PathBuf,
    cancelled: &AtomicBool,
    mut progress: impl FnMut(u64),
    body_timeout: Duration,
) -> Result<PathBuf, String> {
    let partial = final_path.with_extension("partial");
    let result = (|| {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(TTS_DOWNLOAD_CONNECT_TIMEOUT))
            .timeout_recv_response(Some(TTS_DOWNLOAD_RESPONSE_TIMEOUT))
            .timeout_recv_body(Some(body_timeout))
            .build()
            .into();
        let mut response = agent
            .get(url)
            .call()
            .map_err(|error| format!("TTS download failed: {error}"))?;
        let total = response
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(expected);
        let mut reader = response.body_mut().as_reader();
        let mut output = fs::File::create(&partial)
            .map_err(|error| format!("could not create partial TTS archive: {error}"))?;
        let mut buffer = [0_u8; 64 * 1024];
        let mut downloaded = 0_u64;
        let mut progress_gate = super::download_progress::DownloadProgressGate::new(total);
        loop {
            if cancelled.load(Ordering::Acquire) {
                return Err("TTS download cancelled".to_string());
            }
            let count = reader
                .read(&mut buffer)
                .map_err(|error| format!("TTS download interrupted: {error}"))?;
            if count == 0 {
                break;
            }
            output
                .write_all(&buffer[..count])
                .map_err(|error| format!("could not write TTS archive: {error}"))?;
            downloaded += count as u64;
            if progress_gate.should_publish(downloaded) {
                progress(downloaded.min(total));
            }
        }
        if progress_gate.should_publish_final(downloaded) {
            progress(downloaded.min(total));
        }
        output
            .sync_all()
            .map_err(|error| format!("could not flush TTS archive: {error}"))?;
        fs::rename(&partial, &final_path)
            .map_err(|error| format!("could not finalize TTS archive: {error}"))?;
        Ok(final_path.clone())
    })();
    if result.is_err() {
        let _ = fs::remove_file(partial);
    }
    result
}

fn remove_downloads(archive: &Path, support: &[PathBuf]) {
    let _ = fs::remove_file(archive);
    for path in support {
        let _ = fs::remove_file(path);
    }
}
