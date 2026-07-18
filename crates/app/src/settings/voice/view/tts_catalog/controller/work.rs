use super::TtsInstallOutcome;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) fn synthesize_tts_preview(
    selected: &crate::app::dialogue::SelectedTtsModel,
) -> Result<audio::PcmAudioChunk, String> {
    #[cfg(feature = "e2e-test-support")]
    if crate::settings::voice::tts_models::e2e_fake_tts_transport_enabled() {
        return audio::PcmAudioChunk::new(audio::PcmFormat::Pcm16, 16_000, 1, vec![0; 320])
            .map_err(|error| format!("invalid fake preview PCM: {error:?}"));
    }
    let catalog = tts::builtin_model_catalog();
    let descriptor = catalog
        .find(&selected.model_id)
        .ok_or_else(|| format!("unknown TTS model {}", selected.model_id.as_str()))?;
    let available =
        tts::validate_installation(&selected.root.join(selected.model_id.as_str()), descriptor)
            .map_err(|error| error.to_string())?;
    let reference = descriptor
        .capabilities()
        .requires_voice_reference()
        .then(|| tts::load_zipvoice_reference(&selected.root))
        .transpose()
        .map_err(|error| error.to_string())?;
    let mut client = voice_sherpa::SherpaTtsClient::from_available_model_with_reference(
        &available,
        &selected.voice_id,
        reference.as_ref(),
    )
    .map_err(|error| error.to_string())?;
    dialogue::TtsClient::synthesize(&mut client, "你好，这是当前语音的试听。")
        .map_err(|error| error.to_string())
}

pub(super) fn validate_tts_selection(
    root: &std::path::Path,
    model_id: tts::TtsModelId,
    requested_voice: Option<tts::TtsVoiceId>,
) -> Result<crate::app::dialogue::SelectedTtsModel, String> {
    let catalog = tts::builtin_model_catalog();
    let descriptor = catalog
        .find(&model_id)
        .ok_or_else(|| format!("unknown TTS model {}", model_id.as_str()))?;
    let voice_id = descriptor
        .resolve_voice_id(requested_voice.as_ref().map(tts::TtsVoiceId::as_str))
        .clone();
    let reference = descriptor
        .capabilities()
        .requires_voice_reference()
        .then(|| tts::load_zipvoice_reference(root))
        .transpose()
        .map_err(|error| error.to_string())?;
    #[cfg(feature = "e2e-test-support")]
    if crate::settings::voice::tts_models::e2e_fake_tts_transport_enabled() {
        return Ok(crate::app::dialogue::SelectedTtsModel {
            root: root.to_path_buf(),
            model_id,
            voice_id,
        });
    }
    let available = tts::validate_installation(&root.join(model_id.as_str()), descriptor)
        .map_err(|error| error.to_string())?;
    voice_sherpa::SherpaTtsClient::from_available_model_with_reference(
        &available,
        &voice_id,
        reference.as_ref(),
    )
    .map_err(|error| error.to_string())?;
    Ok(crate::app::dialogue::SelectedTtsModel {
        root: root.to_path_buf(),
        model_id,
        voice_id,
    })
}

pub(super) async fn install_tts_package(
    descriptor: &tts::TtsModelDescriptor,
    root: &std::path::Path,
    cancelled: &AtomicBool,
    progress: impl FnMut(u64, u64),
) -> TtsInstallOutcome {
    #[cfg(feature = "e2e-test-support")]
    if crate::settings::voice::tts_models::e2e_fake_tts_transport_enabled() {
        let mut progress = progress;
        progress(0, 1);
        if crate::settings::voice::tts_models::e2e_fake_tts_transport_is_held() {
            if let Some(release) =
                crate::settings::voice::tts_models::take_e2e_fake_tts_release_receiver()
            {
                let _ = release.recv().await;
            }
        }
        if cancelled.load(Ordering::Acquire) {
            return TtsInstallOutcome::Cancelled;
        }
        progress(1, 1);
        return TtsInstallOutcome::Ready;
    }
    let files = match crate::settings::voice::tts_models::download_tts_package(
        descriptor,
        &root.join(".downloads"),
        cancelled,
        progress,
    ) {
        Ok(files) => files,
        Err(_) if cancelled.load(Ordering::Acquire) => return TtsInstallOutcome::Cancelled,
        Err(error) => return TtsInstallOutcome::Failed(error),
    };
    if cancelled.load(Ordering::Acquire) {
        cleanup_downloaded_package(&files);
        return TtsInstallOutcome::Cancelled;
    }
    let installation =
        tts::install_downloaded_package(descriptor, &files.archive, &files.support, root);
    cleanup_downloaded_package(&files);
    if cancelled.load(Ordering::Acquire) {
        let _ = tts::remove_installation(&root.join(descriptor.id().as_str()), descriptor, None);
        return TtsInstallOutcome::Cancelled;
    }
    installation
        .map(|_| TtsInstallOutcome::Ready)
        .unwrap_or_else(|error| TtsInstallOutcome::Failed(error.to_string()))
}

fn cleanup_downloaded_package(files: &crate::settings::voice::tts_models::DownloadedTtsPackage) {
    let _ = std::fs::remove_file(&files.archive);
    for file in &files.support {
        let _ = std::fs::remove_file(file);
    }
}
