//! Voice history persistence helpers.

use crate::app::MultisoulApp;
use storage::VoiceHistoryEntry;
use voice_sherpa::SpeechError;

/// Outcome label pair for a voice turn's history record. `risk` and `status`
/// are always set together and read together downstream, so grouping them
/// keeps `save_voice_history` under clippy's `too_many_arguments` threshold
/// without splitting a single concern across many parameters.
pub(crate) struct VoiceOutcome {
    pub risk: &'static str,
    pub status: &'static str,
}

impl VoiceOutcome {
    pub(crate) const fn new(risk: &'static str, status: &'static str) -> Self {
        Self { risk, status }
    }
}

pub(super) struct VoiceHistoryDraft {
    pub raw_transcript: String,
    pub refined_transcript: Option<String>,
    pub planner_json: String,
    pub confidence: f64,
    pub risk: &'static str,
    pub status: &'static str,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub error: Option<String>,
}

impl MultisoulApp {
    pub(super) fn record_empty_voice_transcript(&mut self) {
        self.voice.set_transcript_only("", "empty transcript");
        self.save_voice_history(
            "".into(),
            None,
            "{}".into(),
            0.0,
            VoiceOutcome::new("low", "transcript_only"),
            None,
        );
    }

    pub(crate) fn save_voice_history(
        &mut self,
        raw_transcript: String,
        refined_transcript: Option<String>,
        planner_json: String,
        confidence: f64,
        outcome: VoiceOutcome,
        error: Option<String>,
    ) {
        self.save_voice_history_with_provider(VoiceHistoryDraft {
            raw_transcript,
            refined_transcript,
            planner_json,
            confidence,
            risk: outcome.risk,
            status: outcome.status,
            provider: Some("ollama".to_string()),
            model: Some(self.voice_settings.values().planner_model().to_string()),
            error,
        });
    }

    pub(super) fn save_voice_speech_error(&mut self, error: SpeechError) {
        self.save_voice_history_with_provider(VoiceHistoryDraft {
            raw_transcript: "".into(),
            refined_transcript: None,
            planner_json: "{}".into(),
            confidence: 0.0,
            risk: "low",
            status: "provider_error",
            provider: Some("sherpa_onnx_paraformer".to_string()),
            model: None,
            error: Some(error.to_string()),
        });
    }

    fn save_voice_history_with_provider(&mut self, draft: VoiceHistoryDraft) {
        let transcript = draft
            .refined_transcript
            .clone()
            .unwrap_or_else(|| draft.raw_transcript.clone());
        let entry = VoiceHistoryEntry {
            transcript,
            raw_transcript: draft.raw_transcript,
            refined_transcript: draft.refined_transcript,
            planner_json: draft.planner_json,
            confidence: draft.confidence,
            risk: draft.risk.to_string(),
            status: draft.status.to_string(),
            provider: draft.provider,
            model: draft.model,
            target_node_id: None,
            error: draft.error,
        };
        if let Err(error) = self.persistence.save_voice_history(&entry) {
            tracing::warn!(%error, "voice history save failed");
        }
    }
}
