use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tts::{TtsModelCatalog, TtsModelDescriptor, TtsModelId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TtsModelInstallState {
    NotInstalled,
    Installing { downloaded: u64, total: u64 },
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
        self.update(id, TtsModelInstallState::Installing { downloaded, total });
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
        |downloaded| progress(downloaded, total),
    )?;
    let mut completed = package.archive().archive_bytes();
    let mut support = Vec::new();
    for (index, artifact) in package.support_artifacts().iter().enumerate() {
        let path = download_file(
            artifact.url(),
            artifact.bytes(),
            downloads.join(format!("{}-support-{index}", descriptor.id().as_str())),
            |downloaded| progress(completed.saturating_add(downloaded), total),
        )?;
        completed = completed.saturating_add(artifact.bytes());
        support.push(path);
    }
    Ok(DownloadedTtsPackage { archive, support })
}

fn download_file(
    url: &str,
    expected: u64,
    final_path: PathBuf,
    mut progress: impl FnMut(u64),
) -> Result<PathBuf, String> {
    let partial = final_path.with_extension("partial");
    let result = (|| {
        let mut response = ureq::get(url)
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
        loop {
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
