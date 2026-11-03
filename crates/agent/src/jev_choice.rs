//! Optional Choice observations. No tool runtime or production routing authority.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
mod transport;
pub use transport::JevChoiceClient;
pub const JEV_ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
pub const JEV_MODEL: &str = "jev-latest";

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum JevError {
    #[error("invalid Jev Choice request")]
    InvalidRequest,
    #[error("invalid Jev Choice response")]
    InvalidResponse,
    #[error("Jev observation cancelled")]
    Cancelled,
    #[error("Jev observation timed out")]
    Timeout,
    #[error("Jev transport failed")]
    Network,
    #[error("Jev HTTP status {status}")]
    Http { status: u16 },
}

pub struct JevChoiceRequest {
    state: String,
    criteria: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct JevObservation {
    pub model: String,
    pub choice: String,
    pub confidence: f64,
    pub probabilities: BTreeMap<String, f64>,
}

#[derive(Deserialize)]
struct Response {
    model: String,
    answers: BTreeMap<String, Answer>,
}

#[derive(Deserialize)]
struct Answer {
    #[serde(rename = "type")]
    kind: String,
    choice: String,
    confidence: f64,
    probabilities: BTreeMap<String, f64>,
}

impl JevChoiceRequest {
    /// The caller supplies complete action/target plans and a clarification exit.
    pub fn new(state: String, criteria: BTreeMap<String, String>) -> Result<Self, JevError> {
        if state.trim().is_empty()
            || state.len() > 32_768
            || !(2..=255).contains(&criteria.len())
            || !criteria.contains_key("CLARIFY")
            || criteria.iter().any(|(id, description)| {
                id.is_empty()
                    || id.len() > 64
                    || !id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                    || description.trim().is_empty()
                    || description.len() > 4096
            })
        {
            return Err(JevError::InvalidRequest);
        }
        Ok(Self { state, criteria })
    }

    pub fn body(&self) -> Value {
        json!({"model":JEV_MODEL,"state":self.state,"questions":{"plan":{
            "type":"choice", "instructions":"Choose one complete action and target plan authorized by the user. Distinguish the executor from people mentioned. Choose CLARIFY when no plan is unambiguous.",
            "criteria":self.criteria
        }}})
    }

    pub fn decode(&self, value: Value) -> Result<JevObservation, JevError> {
        let response: Response =
            serde_json::from_value(value).map_err(|_| JevError::InvalidResponse)?;
        if !response.model.starts_with("jev-")
            || response.model.ends_with("latest")
            || response.model.ends_with("preview")
            || response.model.len() > 128
            || response.answers.len() != 1
        {
            return Err(JevError::InvalidResponse);
        }
        let answer = response
            .answers
            .into_iter()
            .find(|(id, _)| id == "plan")
            .map(|(_, answer)| answer)
            .ok_or(JevError::InvalidResponse)?;
        let valid_probability = |value: f64| value.is_finite() && (0.0..=1.0).contains(&value);
        if answer.kind != "choice"
            || !self.criteria.contains_key(&answer.choice)
            || !valid_probability(answer.confidence)
            || answer.probabilities.keys().ne(self.criteria.keys())
            || !answer.probabilities.values().all(|v| valid_probability(*v))
            || (answer.probabilities.values().sum::<f64>() - 1.0).abs() > 0.001
        {
            return Err(JevError::InvalidResponse);
        }
        let selected = answer.probabilities[&answer.choice];
        if answer.probabilities.values().any(|v| *v > selected) {
            return Err(JevError::InvalidResponse);
        }
        Ok(JevObservation {
            model: response.model,
            choice: answer.choice,
            confidence: answer.confidence,
            probabilities: answer.probabilities,
        })
    }
}

#[cfg(test)]
#[path = "../tests/unit/jev_transport_tests.rs"]
mod transport_tests;
