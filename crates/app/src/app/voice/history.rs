//! Voice history persistence helpers.

use crate::app::MultisoulApp;
use storage::VoiceHistoryEntry;
use voice::OllamaVoicePlanner;
use voice_sherpa::SpeechError;

pub(super) struct VoiceHistoryDraft {
    pub transcript: String,
    pub planner_json: String,
    pub confidence: f64,
    pub risk: &'static str,
    pub status: &'static str,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub error: Option<String>,
}

impl MultisoulApp {
    pub(crate) fn save_voice_history(
        &mut self,
        transcript: String,
        planner_json: String,
        confidence: f64,
        risk: &'static str,
        status: &'static str,
        error: Option<String>,
    ) {
        self.save_voice_history_with_provider(VoiceHistoryDraft {
            transcript,
            planner_json,
            confidence,
            risk,
            status,
            provider: Some("ollama".to_string()),
            model: Some(OllamaVoicePlanner::configured_model()),
            error,
        });
    }

    pub(super) fn save_voice_speech_error(&mut self, error: SpeechError) {
        self.save_voice_history_with_provider(VoiceHistoryDraft {
            transcript: "".into(),
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
        let entry = VoiceHistoryEntry {
            transcript: draft.transcript,
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
