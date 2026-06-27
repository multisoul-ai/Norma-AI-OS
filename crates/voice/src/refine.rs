use crate::ollama::{
    post_ollama_generate, voice_trace_enabled, OllamaVoicePlanner, VoicePlanProviderError,
};
use std::time::Duration;

const REFINE_TIMEOUT_MS: u64 = 500;

pub struct TranscriptRefiner {
    endpoint: String,
    model: String,
}

#[derive(Debug, thiserror::Error)]
pub enum RefineError {
    #[error("transcript refinement timed out")]
    Timeout,
    #[error("ollama error: {0}")]
    OllamaError(String),
}

impl TranscriptRefiner {
    pub fn new(endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
        }
    }

    pub fn from_env_or_default() -> Self {
        let model = std::env::var("MULTISOUL_REFINE_MODEL")
            .unwrap_or_else(|_| OllamaVoicePlanner::configured_model());
        let endpoint = std::env::var("MULTISOUL_OLLAMA_ENDPOINT")
            .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string());
        Self::new(endpoint, model)
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn refine(&self, raw: &str) -> Result<String, RefineError> {
        let prompt = refine_prompt(raw);
        log_refiner_request(self, raw, &prompt, voice_trace_enabled());
        let response = post_ollama_generate(
            &self.endpoint,
            &self.model,
            &prompt,
            Duration::from_millis(REFINE_TIMEOUT_MS),
        )
        .map_err(|error| match error {
            VoicePlanProviderError::Io(_) => RefineError::Timeout,
            other => RefineError::OllamaError(other.to_string()),
        })?;
        let refined = parse_refine_response(&response.response)?;
        Ok(refined.trim().to_string())
    }
}

fn log_refiner_request(
    refiner: &TranscriptRefiner,
    raw_transcript: &str,
    prompt: &str,
    trace_full: bool,
) {
    tracing::info!(
        target: "voice::refine",
        endpoint = %refiner.endpoint,
        model = %refiner.model,
        raw_transcript = %raw_transcript,
        prompt_chars = prompt.chars().count(),
        "transcript refiner request"
    );
    if let Some(prompt) = trace_prompt(prompt, trace_full) {
        tracing::info!(
            target: "voice::refine",
            endpoint = %refiner.endpoint,
            model = %refiner.model,
            prompt = %prompt,
            "transcript refiner prompt"
        );
    }
}

fn trace_prompt(prompt: &str, trace_full: bool) -> Option<&str> {
    trace_full.then_some(prompt)
}

pub fn refine_prompt(raw: &str) -> String {
    format!(
        "You are a transcript correction assistant for a voice-controlled macOS app.\n\n\
         TASK: Correct the transcript below with minimal changes.\n\
         - Fix obvious homophone errors in Chinese, such as 在/再 and 的/地/得.\n\
         - Standardize technical terms spoken phonetically, such as terminal.\n\
         - Keep English technical terms in English: terminal, node, canvas, agent, Codex.\n\
         - Do NOT translate English or Latin-script words into Chinese.\n\
         - Preserve Latin-script spans exactly when they are already readable.\n\
         - Do NOT add content the user did not say.\n\
         - Do NOT rephrase or expand sentences.\n\
         - If no correction is needed, return the transcript unchanged.\n\n\
         Return exactly one JSON object: {{\"refined\":\"<corrected transcript>\"}}\n\n\
         TRANSCRIPT: {}",
        raw
    )
}

fn parse_refine_response(response: &str) -> Result<String, RefineError> {
    let value: serde_json::Value = serde_json::from_str(response)
        .map_err(|error| RefineError::OllamaError(error.to_string()))?;
    value
        .get("refined")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| RefineError::OllamaError("missing refined field".to_string()))
}

#[cfg(test)]
#[path = "../tests/unit/refine_tests.rs"]
mod tests;
