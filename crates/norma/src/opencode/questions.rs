//! OpenCode 1.19.3 question/reply mapping and exact tool consumption correlation.
use crate::protocol::{
    validate_question_answers, validate_question_items, AgentProfile, PendingQuestion,
    QuestionAnswer, QuestionItem, QuestionOption, QuestionRef, QuestionRequestId,
};
use serde_json::{json, Value};
use std::{collections::HashSet, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub struct OpenCodeQuestion {
    request_id: String,
    session_id: String,
    message_id: String,
    call_id: String,
    items: Vec<QuestionItem>,
    original: Arc<Value>,
}

impl OpenCodeQuestion {
    /// Only a GET/event from the owning instance may supply this value to the host registry.
    pub fn from_request(raw: &Value) -> Result<Self, &'static str> {
        if raw.to_string().len() > 1_048_576 {
            return Err("opencode_question_too_large");
        }
        let request_id = identifier(&raw["id"])?;
        let session_id = identifier(&raw["sessionID"])?;
        let message_id = identifier(&raw["tool"]["messageID"])?;
        let call_id = identifier(&raw["tool"]["callID"])?;
        let questions = raw["questions"]
            .as_array()
            .filter(|items| !items.is_empty() && items.len() <= 32)
            .ok_or("opencode_question_invalid_shape")?;
        let mut items = Vec::with_capacity(questions.len());
        for (index, question) in questions.iter().enumerate() {
            let mut labels = HashSet::new();
            let options = question["options"]
                .as_array()
                .filter(|options| options.len() <= 128)
                .ok_or("opencode_question_invalid_options")?
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    let label = identifier(&option["label"])?;
                    if !labels.insert(label) {
                        return Err("opencode_question_ambiguous_label");
                    }
                    let description = match option.get("description") {
                        None | Some(Value::Null) => None,
                        Some(Value::String(value)) => Some(value.clone()),
                        _ => return Err("opencode_question_invalid_description"),
                    };
                    Ok(QuestionOption {
                        key: format!("option-{index}"),
                        label: label.into(),
                        description,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            items.push(QuestionItem {
                key: format!("question-{index}"),
                provider_id: None,
                prompt: question["question"]
                    .as_str()
                    .ok_or("opencode_question_missing_prompt")?
                    .into(),
                options,
                multiple: capability(question, "multiple", false)?,
                allow_custom: capability(question, "custom", true)?,
                secret: false,
            });
        }
        validate_question_items(&items)?;
        Ok(Self {
            request_id: request_id.into(),
            session_id: session_id.into(),
            message_id: message_id.into(),
            call_id: call_id.into(),
            items,
            original: Arc::new(raw.clone()),
        })
    }

    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    pub fn message_id(&self) -> &str {
        &self.message_id
    }
    pub fn call_id(&self) -> &str {
        &self.call_id
    }
    pub fn original_request(&self) -> &Value {
        &self.original
    }

    pub fn question(&self, target: QuestionRef) -> Result<PendingQuestion, &'static str> {
        target.validate()?;
        if target.provider != AgentProfile::OpenCode
            || target.session_id != self.session_id
            || target.request_id != QuestionRequestId::String(self.request_id.clone())
            || target.turn_id.is_some()
        {
            return Err("opencode_question_target_mismatch");
        }
        Ok(PendingQuestion {
            target,
            items: self.items.clone(),
        })
    }

    pub fn response(&self, answers: &[QuestionAnswer]) -> Result<Value, &'static str> {
        validate_question_answers(&self.items, answers)?;
        let mapped = self
            .items
            .iter()
            .map(|item| {
                let answer = answers
                    .iter()
                    .find(|answer| answer.key == item.key)
                    .ok_or("incomplete_answers")?;
                let mut values = answer
                    .selected
                    .iter()
                    .map(|key| {
                        item.options
                            .iter()
                            .find(|option| option.key == *key)
                            .map(|option| option.label.clone())
                            .ok_or("invalid_answer_option")
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                values.extend(answer.text.iter().cloned());
                Ok(values)
            })
            .collect::<Result<Vec<_>, &'static str>>()?;
        Ok(json!({"answers":mapped}))
    }

    pub fn consumed_answers(&self, part: &Value) -> Option<Value> {
        if !self.matches_tool(part) || part["state"]["status"] != "completed" {
            return None;
        }
        let answers = part["state"]["metadata"]["answers"].as_array()?;
        if answers.len() != self.items.len()
            || answers.iter().zip(&self.items).any(|(values, item)| {
                values.as_array().is_none_or(|values| {
                    values.len() > 129
                        || (!item.multiple && values.len() > 1)
                        || values.iter().any(|value| {
                            value
                                .as_str()
                                .is_none_or(|text| text.trim().is_empty() || text.len() > 32768)
                        })
                })
            })
        {
            return None;
        }
        Some(json!({"answers":answers}))
    }

    pub fn confirms_termination(&self, part: &Value) -> bool {
        self.matches_tool(part)
            && part["state"]["status"] == "error"
            && part["state"]["error"]
                .as_str()
                .is_some_and(|error| !error.trim().is_empty() && error.len() <= 1_048_576)
    }

    fn matches_tool(&self, part: &Value) -> bool {
        part["type"] == "tool"
            && part["tool"] == "question"
            && part["callID"] == self.call_id
            && part["sessionID"] == self.session_id
            && part["messageID"] == self.message_id
            && part["state"]["input"]["questions"] == self.original["questions"]
    }

    pub fn confirms_answer(&self, part: &Value, answers: &[QuestionAnswer]) -> bool {
        self.response(answers)
            .ok()
            .is_some_and(|expected| self.consumed_answers(part) == Some(expected))
    }
}

fn identifier(value: &Value) -> Result<&str, &'static str> {
    value
        .as_str()
        .filter(|text| !text.trim().is_empty() && text.len() <= 4096)
        .ok_or("opencode_question_invalid_identity_or_label")
}
fn capability(value: &Value, key: &str, default: bool) -> Result<bool, &'static str> {
    match value.get(key) {
        None => Ok(default),
        Some(Value::Bool(value)) => Ok(*value),
        _ => Err("opencode_question_invalid_capability"),
    }
}
