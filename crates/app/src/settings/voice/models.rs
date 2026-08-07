use asr::{
    install_model_archive, validate_installation, AsrModelCatalog, AsrModelDescriptor, AsrModelId,
    AvailableAsrModel,
};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "e2e-test-support")]
static E2E_FAKE_ASR_INSTALL: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "e2e-test-support")]
pub(crate) fn set_e2e_fake_asr_install(enabled: bool) {
    E2E_FAKE_ASR_INSTALL.store(enabled, Ordering::Release);
}

#[cfg(feature = "e2e-test-support")]
pub(crate) fn e2e_fake_asr_install_enabled() -> bool {
    E2E_FAKE_ASR_INSTALL.load(Ordering::Acquire)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AsrModelInstallState {
    NotInstalled,
    Installing { downloaded: u64, total: u64 },
    Ready,
    Failed(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AsrModelEntryState {
    descriptor: AsrModelDescriptor,
    installation: AsrModelInstallState,
}

impl AsrModelEntryState {
    pub(crate) fn descriptor(&self) -> &AsrModelDescriptor {
        &self.descriptor
    }

    pub(crate) fn installation(&self) -> &AsrModelInstallState {
        &self.installation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AsrModelManagerState {
    entries: Vec<AsrModelEntryState>,
}

impl AsrModelManagerState {
    pub(crate) fn unscanned(catalog: &AsrModelCatalog) -> Self {
        Self {
            entries: catalog
                .entries()
                .iter()
                .cloned()
                .map(|descriptor| AsrModelEntryState {
                    descriptor,
                    installation: AsrModelInstallState::NotInstalled,
                })
                .collect(),
        }
    }

    pub(crate) fn scan(catalog: &AsrModelCatalog, model_root: &Path) -> Self {
        Self::scan_with_vad(catalog, model_root, |root| {
            asr::validate_silero_vad_installation(root)
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
    }

    fn scan_with_vad(
        catalog: &AsrModelCatalog,
        model_root: &Path,
        validate_vad: impl Fn(&Path) -> Result<(), String>,
    ) -> Self {
        let mut state = Self::unscanned(catalog);
        for entry in &mut state.entries {
            let root = model_root.join(entry.descriptor.id().as_str());
            entry.installation = if !root.exists() {
                AsrModelInstallState::NotInstalled
            } else {
                match validate_installation(&root, &entry.descriptor)
                    .map_err(|error| error.to_string())
                    .and_then(|_| validate_vad(model_root))
                {
                    Ok(()) => AsrModelInstallState::Ready,
                    Err(error) => AsrModelInstallState::Failed(error),
                }
            };
        }
        state
    }

    pub(crate) fn entries(&self) -> &[AsrModelEntryState] {
        &self.entries
    }

    pub(crate) fn installation(&self, id: &AsrModelId) -> Option<&AsrModelInstallState> {
        self.entries
            .iter()
            .find(|entry| entry.descriptor.id() == id)
            .map(AsrModelEntryState::installation)
    }

    pub(crate) fn begin_install(&mut self, id: &AsrModelId, total: u64) {
        self.update(
            id,
            AsrModelInstallState::Installing {
                downloaded: 0,
                total,
            },
        );
    }

    pub(crate) fn update_progress(&mut self, id: &AsrModelId, downloaded: u64, total: u64) {
        self.update(id, AsrModelInstallState::Installing { downloaded, total });
    }

    pub(crate) fn mark_ready(&mut self, id: &AsrModelId) {
        self.update(id, AsrModelInstallState::Ready);
    }

    pub(crate) fn mark_failed(&mut self, id: &AsrModelId, message: impl Into<String>) {
        self.update(id, AsrModelInstallState::Failed(message.into()));
    }

    pub(crate) fn mark_not_installed(&mut self, id: &AsrModelId) {
        self.update(id, AsrModelInstallState::NotInstalled);
    }

    fn update(&mut self, id: &AsrModelId, installation: AsrModelInstallState) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.descriptor.id() == id)
        {
            entry.installation = installation;
        }
    }
}

#[cfg(test)]
pub(crate) fn download_model_archive(
    descriptor: &AsrModelDescriptor,
    download_root: &Path,
    progress: impl FnMut(u64, u64),
) -> Result<PathBuf, String> {
    fs::create_dir_all(download_root)
        .map_err(|error| format!("could not create model download directory: {error}"))?;
    let final_path = download_root.join(format!("{}.tar.bz2", descriptor.id().as_str()));
    let partial_path = download_root.join(format!("{}.tar.bz2.partial", descriptor.id().as_str()));
    download_file(
        descriptor.artifact().url(),
        descriptor.artifact().archive_bytes(),
        final_path,
        partial_path,
        None,
        progress,
    )
}

pub(crate) fn download_model_archive_cancellable(
    descriptor: &AsrModelDescriptor,
    download_root: &Path,
    cancelled: &AtomicBool,
    progress: impl FnMut(u64, u64),
) -> Result<PathBuf, String> {
    fs::create_dir_all(download_root)
        .map_err(|error| format!("could not create model download directory: {error}"))?;
    let final_path = download_root.join(format!("{}.tar.bz2", descriptor.id().as_str()));
    let partial_path = download_root.join(format!("{}.tar.bz2.partial", descriptor.id().as_str()));
    download_file(
        descriptor.artifact().url(),
        descriptor.artifact().archive_bytes(),
        final_path,
        partial_path,
        Some(cancelled),
        progress,
    )
}

pub(crate) fn download_silero_vad_cancellable(
    download_root: &Path,
    cancelled: &AtomicBool,
    progress: impl FnMut(u64, u64),
) -> Result<PathBuf, String> {
    fs::create_dir_all(download_root)
        .map_err(|error| format!("could not create VAD download directory: {error}"))?;
    download_file(
        asr::SILERO_VAD_ARCHIVE_URL,
        asr::SILERO_VAD_ARCHIVE_BYTES,
        download_root.join(asr::SILERO_VAD_FILE),
        download_root.join(format!("{}.partial", asr::SILERO_VAD_FILE)),
        Some(cancelled),
        progress,
    )
}

pub(crate) fn ensure_silero_vad(model_root: &Path) -> Result<(), String> {
    #[cfg(feature = "e2e-test-support")]
    {
        let _ = model_root;
        Ok(())
    }
    #[cfg(not(feature = "e2e-test-support"))]
    {
        if asr::validate_silero_vad_installation(model_root).is_ok() {
            return Ok(());
        }
        let vad_root = model_root.join(asr::SILERO_VAD_DIRECTORY);
        if vad_root.exists() {
            fs::remove_dir_all(&vad_root)
                .map_err(|error| format!("could not clear invalid VAD installation: {error}"))?;
        }
        let cancelled = AtomicBool::new(false);
        let vad_file =
            download_silero_vad_cancellable(&model_root.join(".downloads"), &cancelled, |_, _| {})?;
        asr::install_silero_vad_file(&vad_file, model_root).map_err(|error| error.to_string())?;
        let _ = fs::remove_file(vad_file);
        Ok(())
    }
}

fn download_file(
    url: &str,
    expected_bytes: u64,
    final_path: PathBuf,
    partial_path: PathBuf,
    cancelled: Option<&AtomicBool>,
    mut progress: impl FnMut(u64, u64),
) -> Result<PathBuf, String> {
    let result = (|| {
        let mut response = ureq::get(url)
            .call()
            .map_err(|error| format!("model download failed: {error}"))?;
        let total = response
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(expected_bytes);
        let mut reader = response.body_mut().as_reader();
        let mut output = fs::File::create(&partial_path)
            .map_err(|error| format!("could not create partial model archive: {error}"))?;
        let mut downloaded = 0_u64;
        let mut progress_gate = super::download_progress::DownloadProgressGate::new(total);
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            if cancelled.is_some_and(|flag| flag.load(Ordering::Acquire)) {
                return Err("model download cancelled".to_string());
            }
            let count = reader
                .read(&mut buffer)
                .map_err(|error| format!("model download interrupted: {error}"))?;
            if count == 0 {
                break;
            }
            output
                .write_all(&buffer[..count])
                .map_err(|error| format!("could not write model archive: {error}"))?;
            downloaded += count as u64;
            if progress_gate.should_publish(downloaded) {
                progress(downloaded.min(total), total);
            }
        }
        if progress_gate.should_publish_final(downloaded) {
            progress(downloaded.min(total), total);
        }
        output
            .sync_all()
            .map_err(|error| format!("could not flush model archive: {error}"))?;
        fs::rename(&partial_path, &final_path)
            .map_err(|error| format!("could not finalize model archive: {error}"))?;
        Ok(final_path.clone())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&partial_path);
    }
    result
}

pub(crate) fn install_downloaded_archive(
    descriptor: &AsrModelDescriptor,
    archive_path: &Path,
    model_root: &Path,
) -> Result<AvailableAsrModel, String> {
    install_model_archive(descriptor, archive_path, model_root).map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "../../../tests/unit/asr_model_manager_tests.rs"]
mod tests;
