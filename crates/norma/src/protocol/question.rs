//! A reply addresses one live question, never a new task or a permission decision.
use super::{AgentProfile, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum QuestionRequestId {
    Number(i64),
    String(String),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionRef {
    pub workspace_id: String,
    pub project_id: String,
    pub node_id: NodeId,
    pub provider: AgentProfile,
    pub run_id: String,
    pub connection_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    pub request_id: QuestionRequestId,
    pub revision: u64,
}

impl QuestionRef {
    pub fn validate(&self) -> Result<(), &'static str> {
        if [
            &self.workspace_id,
            &self.project_id,
            &self.run_id,
            &self.connection_id,
            &self.session_id,
        ]
        .iter()
        .any(|value| value.trim().is_empty())
            || self.revision == 0
            || matches!(&self.request_id, QuestionRequestId::String(value) if value.trim().is_empty())
            || self
                .turn_id
                .as_ref()
                .is_some_and(|value| value.trim().is_empty())
        {
            return Err("invalid_question_identity");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionAnswer {
    pub key: String,
    #[serde(default)]
    pub selected: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QuestionOption {
    /// Stable within this exact revision; not necessarily an upstream option ID.
    pub key: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QuestionItem {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    pub prompt: String,
    pub options: Vec<QuestionOption>,
    pub multiple: bool,
    pub allow_custom: bool,
    pub secret: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PendingQuestion {
    pub target: QuestionRef,
    pub items: Vec<QuestionItem>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionState {
    Pending,
    Submitting,
    Submitted,
    Answered,
    Failed,
    Unknown,
    AnsweredElsewhere,
    /// Exact tool consumption exists, but a lost transport acknowledgement prevents attribution.
    /// The original input is released; the durable delivery itself remains unconfirmed.
    ConsumedUnattributed,
    Cancelled,
    Expired,
}

impl QuestionState {
    pub fn blocks_input(self) -> bool {
        matches!(
            self,
            Self::Pending | Self::Submitting | Self::Submitted | Self::Failed | Self::Unknown
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QuestionSnapshot {
    pub question: PendingQuestion,
    pub state: QuestionState,
    pub delivery_id: Option<String>,
    pub error: Option<String>,
    pub evidence: Option<String>,
}

impl PendingQuestion {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.target.validate()?;
        validate_question_items(&self.items)
    }

    pub fn validate_answers(&self, answers: &[QuestionAnswer]) -> Result<(), &'static str> {
        self.target.validate()?;
        validate_question_answers(&self.items, answers)
    }
}

pub fn validate_question_items(items: &[QuestionItem]) -> Result<(), &'static str> {
    let mut keys = HashSet::new();
    if items.is_empty() || items.len() > 32 {
        return Err("invalid_question_count");
    }
    for item in items {
        let mut options = HashSet::new();
        if item.key.trim().is_empty()
            || !keys.insert(&item.key)
            || item.prompt.trim().is_empty()
            || item.prompt.len() > 32768
            || item.options.len() > 128
            || (item.options.is_empty() && !item.allow_custom)
            || item.options.iter().any(|option| {
                option.key.trim().is_empty()
                    || option.label.trim().is_empty()
                    || option
                        .description
                        .as_ref()
                        .is_some_and(|text| text.len() > 32768)
                    || !options.insert(&option.key)
            })
        {
            return Err("invalid_question_shape");
        }
    }
    Ok(())
}

pub fn validate_question_answers(
    items: &[QuestionItem],
    answers: &[QuestionAnswer],
) -> Result<(), &'static str> {
    validate_question_items(items)?;
    if answers.len() != items.len() {
        return Err("incomplete_answers");
    }
    let mut seen = HashSet::new();
    for answer in answers {
        let item = items
            .iter()
            .find(|item| item.key == answer.key)
            .ok_or("unknown_question")?;
        if !seen.insert(&answer.key) {
            return Err("duplicate_answer");
        }
        let custom = usize::from(answer.text.is_some());
        if answer
            .text
            .as_ref()
            .is_some_and(|text| !item.allow_custom || text.trim().is_empty() || text.len() > 32768)
            || answer.selected.len() + custom == 0
            || (!item.multiple && answer.selected.len() + custom != 1)
        {
            return Err("invalid_answer_cardinality");
        }
        let mut choices = HashSet::new();
        if answer.selected.iter().any(|key| {
            !choices.insert(key) || !item.options.iter().any(|option| option.key == *key)
        }) {
            return Err("invalid_answer_option");
        }
    }
    Ok(())
}
