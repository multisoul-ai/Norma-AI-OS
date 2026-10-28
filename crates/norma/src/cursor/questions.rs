//! Cursor 2026.10.18 native AskQuestion mapping. Sources must come from the bound live store.
use crate::protocol::{
    validate_question_answers, validate_question_items, AgentProfile, PendingQuestion,
    QuestionAnswer, QuestionItem, QuestionOption, QuestionRef, QuestionRequestId,
};
use serde_json::Value;
use std::{collections::HashSet, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub struct CursorQuestion {
    session: String,
    call: String,
    items: Vec<QuestionItem>,
    option_ids: Vec<Vec<String>>,
    original: Arc<Value>,
}
impl CursorQuestion {
    pub fn from_tool(session: &str, raw: &Value) -> Result<Self, &'static str> {
        if session.trim().is_empty() || session.len() > 4096 || raw.to_string().len() > 1_048_576 {
            return Err("cursor_question_invalid_source");
        }
        if raw["type"] != "tool-call" || raw["toolName"] != "AskQuestion" {
            return Err("cursor_not_question");
        }
        let call = raw["toolCallId"]
            .as_str()
            .filter(|id| !id.trim().is_empty() && id.len() <= 4096)
            .ok_or("cursor_question_missing_call")?;
        let questions = raw["args"]["questions"]
            .as_array()
            .filter(|v| !v.is_empty() && v.len() <= 32)
            .ok_or("cursor_question_invalid_shape")?;
        let mut ids = HashSet::new();
        let mut items = Vec::new();
        let mut option_ids = Vec::new();
        for (index, question) in questions.iter().enumerate() {
            let id = identifier(&question["id"])?;
            if !ids.insert(id) {
                return Err("cursor_duplicate_question");
            }
            let mut labels = HashSet::new();
            let mut keys = HashSet::new();
            let mut native_ids = Vec::new();
            let raw_options = question["options"]
                .as_array()
                .filter(|v| !v.is_empty() && v.len() <= 128)
                .ok_or("cursor_question_invalid_options")?;
            let count =
                raw_options.len()
                    - usize::from(raw_options.last().is_some_and(|option| {
                        option["label"].as_str().is_some_and(trailing_other)
                    }));
            let options = raw_options[..count]
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    let id = identifier(&option["id"])?;
                    if id.contains(',') || id == "__freeform_other__" {
                        return Err("cursor_ambiguous_option_id");
                    }
                    let label = identifier(&option["label"])?;
                    if !labels.insert(label) || !keys.insert(id) || label == "Other:" {
                        return Err("cursor_ambiguous_options");
                    }
                    native_ids.push(id.to_owned());
                    Ok(QuestionOption {
                        key: format!("option-{index}"),
                        label: label.into(),
                        description: None,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let multiple = match question.get("allow_multiple") {
                None => false,
                Some(Value::Bool(value)) => *value,
                _ => return Err("cursor_invalid_multiple"),
            };
            items.push(QuestionItem {
                key: format!("question-{index}"),
                provider_id: Some(id.into()),
                prompt: question["prompt"]
                    .as_str()
                    .ok_or("cursor_missing_prompt")?
                    .into(),
                options,
                multiple,
                allow_custom: true,
                secret: false,
            });
            option_ids.push(native_ids);
        }
        validate_question_items(&items)?;
        Ok(Self {
            session: session.into(),
            call: call.into(),
            items,
            option_ids,
            original: Arc::new(raw.clone()),
        })
    }
    pub fn session_id(&self) -> &str {
        &self.session
    }
    pub fn call_id(&self) -> &str {
        &self.call
    }
    pub fn items(&self) -> &[QuestionItem] {
        &self.items
    }
    pub fn original_tool(&self) -> &Value {
        &self.original
    }
    pub fn question(&self, target: QuestionRef) -> Result<PendingQuestion, &'static str> {
        target.validate()?;
        if target.provider != AgentProfile::Cursor
            || target.session_id != self.session
            || target.turn_id.is_some()
            || target.request_id != QuestionRequestId::String(self.call.clone())
        {
            return Err("cursor_question_target_mismatch");
        }
        Ok(PendingQuestion {
            target,
            items: self.items.clone(),
        })
    }
    pub fn expected_result(&self, answers: &[QuestionAnswer]) -> Result<String, &'static str> {
        validate_question_answers(&self.items, answers)?;
        let mut lines = vec!["User questions responses:".to_owned()];
        for (index, item) in self.items.iter().enumerate() {
            let answer = answers
                .iter()
                .find(|answer| answer.key == item.key)
                .ok_or("incomplete_answers")?;
            let value = if let Some(text) = &answer.text {
                if !answer.selected.is_empty() {
                    return Err("cursor_mixed_custom_unsupported");
                }
                text.clone()
            } else {
                // A deterministic preview; native consumption preserves click order instead.
                let ids = item
                    .options
                    .iter()
                    .enumerate()
                    .filter(|(_, option)| answer.selected.contains(&option.key))
                    .map(|(i, _)| self.option_ids[index][i].as_str())
                    .collect::<Vec<_>>();
                format!("Selected option(s) {}", ids.join(", "))
            };
            lines.push(format!(
                "Question {}: {value}",
                item.provider_id.as_deref().expect("validated original ID")
            ));
        }
        Ok(lines.join("\n"))
    }
    pub fn confirms_closed(&self, raw: &Value) -> bool {
        raw["type"] == "tool-result"
            && raw["toolName"] == "AskQuestion"
            && raw["toolCallId"] == self.call
            && raw["result"]
                .as_str()
                .is_some_and(|text| !text.trim().is_empty() && text.len() <= 1_048_576)
    }
    pub fn confirms_consumption(&self, raw: &Value) -> bool {
        self.confirms_closed(raw)
            && raw["result"]
                .as_str()
                .is_some_and(|text| text.starts_with("User questions responses:\n"))
    }
    /// A correlated final tool result closes the original request even when rejected or failed.
    /// Its text is never treated as an answer or used to infer which participant acted.
    pub fn confirms_cancelled(&self, raw: &Value) -> bool {
        self.confirms_closed(raw) && !self.confirms_consumption(raw)
    }
    pub fn confirms_answer(&self, raw: &Value, answers: &[QuestionAnswer]) -> bool {
        if !self.confirms_consumption(raw) || self.expected_result(answers).is_err() {
            return false;
        }
        let Some(text) = raw["result"].as_str() else {
            return false;
        };
        let lines = text.lines().skip(1).collect::<Vec<_>>();
        if lines.len() != self.items.len() {
            return false;
        }
        self.items.iter().enumerate().all(|(index, item)| {
            let Some(answer) = answers.iter().find(|answer| answer.key == item.key) else {
                return false;
            };
            let prefix = format!("Question {}: ", item.provider_id.as_deref().unwrap_or(""));
            let Some(value) = lines[index].strip_prefix(&prefix) else {
                return false;
            };
            if let Some(text) = &answer.text {
                return value == text;
            }
            let Some(selected) = value.strip_prefix("Selected option(s) ") else {
                return false;
            };
            let selected = selected.split(", ").collect::<Vec<_>>();
            let expected = item
                .options
                .iter()
                .enumerate()
                .filter(|(_, option)| answer.selected.contains(&option.key))
                .map(|(i, _)| self.option_ids[index][i].as_str())
                .collect::<HashSet<_>>();
            selected.len() == expected.len()
                && selected.iter().copied().collect::<HashSet<_>>() == expected
        })
    }
}
fn identifier(raw: &Value) -> Result<&str, &'static str> {
    raw.as_str()
        .filter(|value| {
            !value.trim().is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
        })
        .ok_or("cursor_invalid_question_identity")
}

// Mirrors the installed native display-option transformation; this is a freeform placeholder,
// not an ordinary selectable provider ID.
fn trailing_other(label: &str) -> bool {
    let label = label.trim().to_lowercase();
    label == "other"
        || label == "something else"
        || [
            "other:",
            "other -",
            "other (",
            "something else:",
            "something else -",
            "something else (",
        ]
        .iter()
        .any(|prefix| label.starts_with(prefix))
}
