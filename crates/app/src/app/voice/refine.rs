//! Voice transcript refinement helpers.

use crate::settings::voice::VoiceSettingsValues;
use voice::TranscriptRefiner;

pub(super) fn refine_speech_transcript(
    raw_transcript: &str,
    settings: &VoiceSettingsValues,
) -> (String, Option<String>) {
    let refiner = TranscriptRefiner::new(settings.ollama_endpoint(), settings.refine_model());
    match refiner.refine(raw_transcript) {
        Ok(refined) if !refined.trim().is_empty() && refined != raw_transcript => {
            let refined = refined.trim().to_string();
            (refined.clone(), Some(refined))
        }
        _ => (raw_transcript.to_string(), None),
    }
}
